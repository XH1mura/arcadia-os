use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use crate::block::AtaBlockDevice;
use crate::fs::fat32::Fat32Fs;
use crate::fs::fat32::bpb::Fat32DirEntry;
use crate::fs::fat32::dir;
use crate::fs::fat32::fat;
use crate::fs::fat32::bpb;
use spin::Mutex;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VfsError {
    NotFound,
    NotADirectory,
    NotAFile,
    AlreadyExists,
    PermissionDenied,
    IoError,
    NoSpace,
    InvalidPath,
}

impl core::fmt::Display for VfsError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            VfsError::NotFound => write!(f, "Not found"),
            VfsError::NotADirectory => write!(f, "Not a directory"),
            VfsError::NotAFile => write!(f, "Not a file"),
            VfsError::AlreadyExists => write!(f, "Already exists"),
            VfsError::PermissionDenied => write!(f, "Permission denied"),
            VfsError::IoError => write!(f, "I/O error"),
            VfsError::NoSpace => write!(f, "No space left"),
            VfsError::InvalidPath => write!(f, "Invalid path"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FileMode {
    Read,
    Write,
    Create,
    CreateNew,
}

#[derive(Debug, Clone)]
pub struct VfsNode {
    pub path: String,
    pub is_dir: bool,
    pub size: u32,
    pub cluster: u32,
}

#[derive(Debug, Clone)]
pub struct FileDescriptor {
    pub fd: usize,
    pub path: String,
    pub mode: FileMode,
    pub pos: usize,
    pub size: u32,
    pub cluster: u32,
}

pub struct VfsManager {
    pub mount_point: String,
    pub fat_fs: Option<Fat32Fs>,
    pub root_cluster: u32,
    pub file_table: Vec<FileDescriptor>,
    pub next_fd: usize,
    pub cwd: String,
}

static GLOBAL_VFS: Mutex<Option<VfsManager>> = Mutex::new(None);

pub fn init_vfs() {
    let device: &'static AtaBlockDevice = {
        Box::leak(Box::new(AtaBlockDevice::primary_master()))
    };

    match Fat32Fs::mount(device, 0) {
        Ok(fs) => {
            let root_cluster = fs.bpb.root_cluster;
            let label = fs.bpb.volume_label_str().to_string();
            crate::serial_println!("[VFS] Mounted FAT32: \"{}\"", label);
            crate::serial_println!(
                "[VFS]   Cluster size: {} bytes, {} total clusters",
                fs.bpb.cluster_size_bytes(),
                fs.bpb.total_clusters()
            );

            *GLOBAL_VFS.lock() = Some(VfsManager {
                mount_point: String::from("/"),
                fat_fs: Some(fs),
                root_cluster,
                file_table: Vec::new(),
                next_fd: 3,
                cwd: String::from("/"),
            });
        }
        Err(e) => {
            crate::serial_println!("[VFS] No FAT32 filesystem found: {}", e);
            *GLOBAL_VFS.lock() = Some(VfsManager {
                mount_point: String::from("/"),
                fat_fs: None,
                root_cluster: 0,
                file_table: Vec::new(),
                next_fd: 3,
                cwd: String::from("/"),
            });
        }
    }
}

pub fn vfs() -> spin::MutexGuard<'static, Option<VfsManager>> {
    GLOBAL_VFS.lock()
}

impl VfsManager {
    pub fn is_mounted(&self) -> bool {
        self.fat_fs.is_some()
    }

    fn fs(&self) -> Result<&Fat32Fs, VfsError> {
        self.fat_fs.as_ref().ok_or(VfsError::IoError)
    }

    pub fn resolve_path(&self, path: &str) -> Result<(u32, &str), VfsError> {
        let fs = self.fs()?;
        let path = path.trim_start_matches('/');

        if path.is_empty() || path == "." {
            return Ok((self.root_cluster, ""));
        }

        let mut current_cluster = self.root_cluster;
        let mut remaining = path;

        loop {
            if remaining.is_empty() {
                return Ok((current_cluster, ""));
            }

            let slash_pos = remaining.find('/');
            let (component, rest) = match slash_pos {
                Some(pos) => (&remaining[..pos], &remaining[pos + 1..]),
                None => (remaining, ""),
            };

            if component.is_empty() || component == "." {
                remaining = rest;
                continue;
            }

            if component == ".." {
                if current_cluster == self.root_cluster {
                    remaining = rest;
                    continue;
                }

                let entries = dir::read_dir_entries(fs, current_cluster)
                    .map_err(|_| VfsError::IoError)?;
                let mut found = false;
                for entry in &entries {
                    if entry.name == ".." {
                        current_cluster = entry.cluster;
                        found = true;
                        break;
                    }
                }
                if !found {
                    current_cluster = self.root_cluster;
                }
                remaining = rest;
                continue;
            }

            let upper_name = to_upper_83(component);
            let entries = dir::read_dir_entries(fs, current_cluster)
                .map_err(|_| VfsError::IoError)?;

            let mut found = false;
            for entry in &entries {
                let entry_upper = to_upper_83(&entry.name);
                if entry_upper.as_str() == core::str::from_utf8(upper_name.as_bytes()).unwrap_or("") {
                    current_cluster = entry.cluster;
                    found = true;
                    break;
                }
            }

            if !found {
                return Err(VfsError::NotFound);
            }

            remaining = rest;
        }
    }

    pub fn list_dir(&self, path: &str) -> Result<Vec<VfsNode>, VfsError> {
        let fs = self.fs()?;
        let (cluster, _) = self.resolve_path(path)?;
        let entries = dir::read_dir_entries(fs, cluster).map_err(|_| VfsError::IoError)?;

        let mut result = Vec::new();
        for entry in entries {
            result.push(VfsNode {
                path: entry.name.clone(),
                is_dir: entry.is_dir,
                size: entry.size,
                cluster: entry.cluster,
            });
        }
        Ok(result)
    }

    pub fn open(&mut self, path: &str, mode: FileMode) -> Result<usize, VfsError> {
        let fs = self.fs()?;
        let path_clean = self.canonicalize(path);
        let (parent_cluster, file_name) = self.resolve_parent(&path_clean)?;

        if file_name.is_empty() {
            if mode == FileMode::Read {
                let (cluster, _) = self.resolve_path(&path_clean)?;
                let fd = self.next_fd;
                self.next_fd += 1;
                self.file_table.push(FileDescriptor {
                    fd,
                    path: path_clean,
                    mode,
                    pos: 0,
                    size: 0,
                    cluster,
                });
                return Ok(fd);
            }
            return Err(VfsError::InvalidPath);
        }

        let upper_name = to_upper_83(file_name);
        let existing = dir::find_entry(fs, parent_cluster, upper_name.as_bytes())
            .map_err(|_| VfsError::IoError)?;

        match mode {
            FileMode::Read => {
                let entry = existing.ok_or(VfsError::NotFound)?;
                if entry.is_directory() {
                    return Err(VfsError::NotAFile);
                }
                let fd = self.next_fd;
                self.next_fd += 1;
                self.file_table.push(FileDescriptor {
                    fd,
                    path: path_clean,
                    mode,
                    pos: 0,
                    size: entry.file_size,
                    cluster: entry.first_cluster(),
                });
                Ok(fd)
            }
            FileMode::Write => {
                let entry = existing.ok_or(VfsError::NotFound)?;
                if entry.is_directory() {
                    return Err(VfsError::NotAFile);
                }
                let fd = self.next_fd;
                self.next_fd += 1;
                self.file_table.push(FileDescriptor {
                    fd,
                    path: path_clean,
                    mode,
                    pos: 0,
                    size: entry.file_size,
                    cluster: entry.first_cluster(),
                });
                Ok(fd)
            }
            FileMode::Create | FileMode::CreateNew => {
                if let Some(entry) = existing {
                    if mode == FileMode::CreateNew {
                        return Err(VfsError::AlreadyExists);
                    }
                    if entry.is_directory() {
                        return Err(VfsError::NotAFile);
                    }
                    let fd = self.next_fd;
                    self.next_fd += 1;
                    self.file_table.push(FileDescriptor {
                        fd,
                        path: path_clean,
                        mode,
                        pos: 0,
                        size: entry.file_size,
                        cluster: entry.first_cluster(),
                    });
                    return Ok(fd);
                }

                let first_cluster = fs.allocate_and_write(b"").map_err(|_| VfsError::NoSpace)?;

                let mut new_entry = unsafe { core::mem::zeroed::<Fat32DirEntry>() };
                new_entry.set_name_83(upper_name.as_bytes());
                new_entry.attributes = 0x20;
                new_entry.set_first_cluster(first_cluster);
                new_entry.file_size = 0;

                dir::add_entry(fs, parent_cluster, &new_entry).map_err(|_| VfsError::IoError)?;

                let fd = self.next_fd;
                self.next_fd += 1;
                self.file_table.push(FileDescriptor {
                    fd,
                    path: path_clean,
                    mode,
                    pos: 0,
                    size: 0,
                    cluster: first_cluster,
                });
                Ok(fd)
            }
        }
    }

    pub fn read(&self, fd: usize, buf: &mut [u8]) -> Result<usize, VfsError> {
        let file = self.file_table.iter().find(|f| f.fd == fd).ok_or(VfsError::NotFound)?;
        let fs = self.fs()?;
        let bytes_left = (file.size as usize).saturating_sub(file.pos);
        if bytes_left == 0 {
            return Ok(0);
        }
        let to_read = buf.len().min(bytes_left);
        let mut file_buf = alloc::vec![0u8; file.size as usize];
        fs.read_file(file.cluster, file.size, &mut file_buf)
            .map_err(|_| VfsError::IoError)?;

        let end = (file.pos + to_read).min(file.size as usize);
        let available = end.saturating_sub(file.pos);
        if available > 0 {
            buf[..available].copy_from_slice(&file_buf[file.pos..end]);
            Ok(available)
        } else {
            Ok(0)
        }
    }

    pub fn write(&mut self, fd: usize, data: &[u8]) -> Result<usize, VfsError> {
        let file_idx = self.file_table.iter().position(|f| f.fd == fd).ok_or(VfsError::NotFound)?;

        {
            let file = &mut self.file_table[file_idx];
            if file.mode == FileMode::Read {
                return Err(VfsError::PermissionDenied);
            }
        }

        let file = &self.file_table[file_idx];
        let fs = self.fat_fs.as_ref().ok_or(VfsError::IoError)?;

        if file.pos == 0 {
            fs.write_file_data(file.cluster, data)
                .map_err(|_| VfsError::IoError)?;
        } else {
            let new_end = file.pos + data.len();
            let buf_size = new_end.max(file.size as usize);
            let mut full_data = alloc::vec![0u8; buf_size];
            if file.size > 0 {
                fs.read_file(file.cluster, file.size, &mut full_data)
                    .map_err(|_| VfsError::IoError)?;
            }
            let write_end = new_end.min(buf_size);
            let write_len = write_end.saturating_sub(file.pos);
            full_data[file.pos..write_end].copy_from_slice(&data[..write_len]);
            fs.write_file_data(file.cluster, &full_data)
                .map_err(|_| VfsError::IoError)?;
        }

        let file = &mut self.file_table[file_idx];
        file.pos += data.len();
        let new_size = file.pos.max(file.size as usize);
        file.size = new_size as u32;
        let path = file.path.clone();
        let size = file.size;

        if size > 0 {
            self.update_file_size(&path, size)?;
        }

        Ok(data.len())
    }

    fn update_file_size(&self, path: &str, new_size: u32) -> Result<(), VfsError> {
        let fs = self.fs()?;
        let (parent_cluster, file_name) = self.resolve_parent(path)?;
        if file_name.is_empty() {
            return Ok(());
        }

        let upper_name = to_upper_83(file_name);
        let chain = crate::fs::fat32::fat::read_chain(fs, parent_cluster)
            .map_err(|_| VfsError::IoError)?;
        let cluster_size = fs.bpb.cluster_size_bytes();
        let entry_size = 32;
        let entries_per_cluster = cluster_size / entry_size;

        for &cluster in &chain {
            let mut cluster_buf = alloc::vec![0u8; cluster_size];
            fs.read_cluster(cluster, &mut cluster_buf)
                .map_err(|_| VfsError::IoError)?;

            for i in 0..entries_per_cluster {
                let offset = i * entry_size;
                let mut entry = unsafe {
                    core::ptr::read(cluster_buf[offset..].as_ptr() as *const Fat32DirEntry)
                };

                if entry.is_end() {
                    return Ok(());
                }
                if entry.is_deleted() || entry.is_long_name() || entry.is_volume_label() {
                    continue;
                }

                let entry_name = entry.full_name_upper();
                if entry_name == upper_name.as_bytes() {
                    entry.file_size = new_size;
                    cluster_buf[offset..offset + entry_size].copy_from_slice(unsafe {
                        core::slice::from_raw_parts(
                            &entry as *const Fat32DirEntry as *const u8,
                            entry_size,
                        )
                    });
                    fs.write_cluster(cluster, &cluster_buf)
                        .map_err(|_| VfsError::IoError)?;
                    return Ok(());
                }
            }
        }

        Ok(())
    }

    pub fn close(&mut self, fd: usize) -> Result<(), VfsError> {
        self.file_table.retain(|f| f.fd != fd);
        Ok(())
    }

    pub fn stat(&self, path: &str) -> Result<VfsNode, VfsError> {
        let fs = self.fs()?;
        let (cluster, _) = self.resolve_path(path)?;

        let path_clean = self.canonicalize(path);
        let (_, file_name) = self.resolve_parent(&path_clean)?;

        if file_name.is_empty() {
            return Ok(VfsNode {
                path: path_clean,
                is_dir: true,
                size: 0,
                cluster,
            });
        }

        let (parent_cluster, _) = self.resolve_parent(&path_clean)?;
        let upper_name = to_upper_83(file_name);
        let entry = dir::find_entry(fs, parent_cluster, upper_name.as_bytes())
            .map_err(|_| VfsError::IoError)?
            .ok_or(VfsError::NotFound)?;

        Ok(VfsNode {
            path: path_clean,
            is_dir: entry.is_directory(),
            size: entry.file_size,
            cluster: entry.first_cluster(),
        })
    }

    pub fn mkdir(&self, path: &str) -> Result<(), VfsError> {
        let fs = self.fs()?;
        let (parent_cluster, dir_name) = self.resolve_parent(path)?;

        if dir_name.is_empty() {
            return Err(VfsError::InvalidPath);
        }

        let upper_name = to_upper_83(dir_name);
        let existing = dir::find_entry(fs, parent_cluster, upper_name.as_bytes())
            .map_err(|_| VfsError::IoError)?;
        if existing.is_some() {
            return Err(VfsError::AlreadyExists);
        }

        let new_cluster = fat::alloc_cluster(fs).map_err(|_| VfsError::NoSpace)?;

        let mut cluster_buf = alloc::vec![0u8; fs.bpb.cluster_size_bytes()];

        let mut dot_entry = unsafe { core::mem::zeroed::<Fat32DirEntry>() };
        dot_entry.name = *b".       ";
        dot_entry.attributes = 0x10;
        dot_entry.set_first_cluster(new_cluster);
        cluster_buf[0..32].copy_from_slice(unsafe {
            core::slice::from_raw_parts(&dot_entry as *const _ as *const u8, 32)
        });

        let mut dotdot_entry = unsafe { core::mem::zeroed::<Fat32DirEntry>() };
        dotdot_entry.name = *b"..      ";
        dotdot_entry.attributes = 0x10;
        dotdot_entry.set_first_cluster(parent_cluster);
        cluster_buf[32..64].copy_from_slice(unsafe {
            core::slice::from_raw_parts(&dotdot_entry as *const _ as *const u8, 32)
        });

        fs.write_cluster(new_cluster, &cluster_buf)
            .map_err(|_| VfsError::IoError)?;
        fat::write_fat_entry(fs, new_cluster, bpb::FAT32_EOC)
            .map_err(|_| VfsError::IoError)?;

        let mut new_entry = unsafe { core::mem::zeroed::<Fat32DirEntry>() };
        new_entry.set_name_83(upper_name.as_bytes());
        new_entry.attributes = 0x10;
        new_entry.set_first_cluster(new_cluster);
        new_entry.file_size = 0;

        dir::add_entry(fs, parent_cluster, &new_entry).map_err(|_| VfsError::IoError)?;

        Ok(())
    }

    pub fn create_file(&self, path: &str, data: &[u8]) -> Result<(), VfsError> {
        let fs = self.fs()?;
        let (parent_cluster, file_name) = self.resolve_parent(path)?;

        if file_name.is_empty() {
            return Err(VfsError::InvalidPath);
        }

        let upper_name = to_upper_83(file_name);
        let existing = dir::find_entry(fs, parent_cluster, upper_name.as_bytes())
            .map_err(|_| VfsError::IoError)?;

        if let Some(entry) = existing {
            if entry.is_directory() {
                return Err(VfsError::NotAFile);
            }

            if entry.first_cluster() >= 2 {
                fat::free_chain(fs, entry.first_cluster())
                    .map_err(|_| VfsError::IoError)?;
            }

            let new_cluster = if data.is_empty() {
                let c = fat::alloc_cluster(fs).map_err(|_| VfsError::NoSpace)?;
                fat::write_fat_entry(fs, c, bpb::FAT32_EOC)
                    .map_err(|_| VfsError::IoError)?;
                c
            } else {
                fs.allocate_and_write(data).map_err(|_| VfsError::NoSpace)?
            };

            self.update_dir_entry(parent_cluster, &upper_name, new_cluster, data.len() as u32)?;
        } else {
            let first_cluster = fs.allocate_and_write(data).map_err(|_| VfsError::NoSpace)?;

            let mut new_entry = unsafe { core::mem::zeroed::<Fat32DirEntry>() };
            new_entry.set_name_83(upper_name.as_bytes());
            new_entry.attributes = 0x20;
            new_entry.set_first_cluster(first_cluster);
            new_entry.file_size = data.len() as u32;

            dir::add_entry(fs, parent_cluster, &new_entry)
                .map_err(|_| VfsError::IoError)?;
        }

        Ok(())
    }

    fn update_dir_entry(
        &self,
        dir_cluster: u32,
        name_upper: &str,
        new_cluster: u32,
        new_size: u32,
    ) -> Result<(), VfsError> {
        let fs = self.fs()?;
        let chain = crate::fs::fat32::fat::read_chain(fs, dir_cluster)
            .map_err(|_| VfsError::IoError)?;
        let cluster_size = fs.bpb.cluster_size_bytes();
        let entry_size = 32;
        let entries_per_cluster = cluster_size / entry_size;

        for &cluster in &chain {
            let mut cluster_buf = alloc::vec![0u8; cluster_size];
            fs.read_cluster(cluster, &mut cluster_buf)
                .map_err(|_| VfsError::IoError)?;

            for i in 0..entries_per_cluster {
                let offset = i * entry_size;
                let mut e = unsafe {
                    core::ptr::read_unaligned(cluster_buf[offset..].as_ptr() as *const Fat32DirEntry)
                };
                if e.is_end() || e.is_deleted() || e.is_long_name() || e.is_volume_label() {
                    continue;
                }
                if e.full_name_upper() == name_upper.as_bytes() {
                    e.set_first_cluster(new_cluster);
                    e.file_size = new_size;
                    cluster_buf[offset..offset + entry_size].copy_from_slice(unsafe {
                        core::slice::from_raw_parts(&e as *const _ as *const u8, entry_size)
                    });
                    fs.write_cluster(cluster, &cluster_buf)
                        .map_err(|_| VfsError::IoError)?;
                    return Ok(());
                }
            }
        }
        Err(VfsError::NotFound)
    }

    pub fn delete(&self, path: &str) -> Result<(), VfsError> {
        let fs = self.fs()?;
        let (parent_cluster, file_name) = self.resolve_parent(path)?;

        if file_name.is_empty() {
            return Err(VfsError::InvalidPath);
        }

        let upper_name = to_upper_83(file_name);
        let removed =
            dir::remove_entry(fs, parent_cluster, upper_name.as_bytes())
                .map_err(|_| VfsError::IoError)?;

        if removed.first_cluster() >= 2 {
            fat::free_chain(fs, removed.first_cluster()).map_err(|_| VfsError::IoError)?;
        }

        Ok(())
    }

    pub fn read_file_content(&self, path: &str) -> Result<Vec<u8>, VfsError> {
        let fs = self.fs()?;
        let (_, file_name) = self.resolve_parent(path)?;

        if file_name.is_empty() {
            return Ok(Vec::new());
        }

        let (parent_cluster, _) = self.resolve_parent(path)?;
        let upper_name = to_upper_83(file_name);
        let entry = dir::find_entry(fs, parent_cluster, upper_name.as_bytes())
            .map_err(|_| VfsError::IoError)?
            .ok_or(VfsError::NotFound)?;

        if entry.is_directory() {
            return Err(VfsError::NotAFile);
        }

        if entry.file_size == 0 {
            return Ok(Vec::new());
        }

        let mut buf = alloc::vec![0u8; entry.file_size as usize];
        fs.read_file(entry.first_cluster(), entry.file_size, &mut buf)
            .map_err(|_| VfsError::IoError)?;
        Ok(buf)
    }

    fn canonicalize(&self, path: &str) -> String {
        if path.starts_with('/') {
            path.to_string()
        } else {
            let mut full = self.cwd.clone();
            if !full.ends_with('/') {
                full.push('/');
            }
            full.push_str(path);
            full
        }
    }

    fn resolve_parent<'a>(&self, path: &'a str) -> Result<(u32, &'a str), VfsError> {
        let path = path.trim_start_matches('/');
        if path.is_empty() {
            return Ok((self.root_cluster, ""));
        }

        if let Some(slash_pos) = path.rfind('/') {
            let parent = &path[..slash_pos];
            let name = &path[slash_pos + 1..];
            if parent.is_empty() {
                Ok((self.root_cluster, name))
            } else {
                let (cluster, _) = self.resolve_path(parent)?;
                Ok((cluster, name))
            }
        } else {
            Ok((self.root_cluster, path))
        }
    }
}

fn to_upper_83(name: &str) -> String {
    let upper: Vec<u8> = name
        .bytes()
        .map(|b| {
            if b >= b'a' && b <= b'z' {
                b - 32
            } else {
                b
            }
        })
        .collect();
    String::from_utf8_lossy(&upper).to_string()
}
