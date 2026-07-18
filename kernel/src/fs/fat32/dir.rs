use alloc::string::{String, ToString};
use alloc::vec::Vec;
use crate::fs::fat32::bpb::Fat32DirEntry;
use crate::fs::fat32::fat;
use crate::fs::fat32::Fat32Fs;

#[derive(Debug, Clone)]
pub struct DirEntryInfo {
    pub name: String,
    pub is_dir: bool,
    pub size: u32,
    pub cluster: u32,
}

pub fn read_dir_entries(
    fs: &Fat32Fs,
    dir_cluster: u32,
) -> Result<Vec<DirEntryInfo>, &'static str> {
    let chain = fat::read_chain(fs, dir_cluster)?;
    let cluster_size = fs.bpb.cluster_size_bytes();
    let entry_size = 32;
    let entries_per_cluster = cluster_size / entry_size;
    let mut result = Vec::new();

    for &cluster in &chain {
        let mut cluster_buf = alloc::vec![0u8; cluster_size];
        fs.read_cluster(cluster, &mut cluster_buf)?;

        for i in 0..entries_per_cluster {
            let offset = i * entry_size;
            let entry = unsafe {
                core::ptr::read_unaligned(cluster_buf[offset..].as_ptr() as *const Fat32DirEntry)
            };

            if entry.is_end() {
                return Ok(result);
            }

            if entry.is_deleted() {
                continue;
            }

            if entry.is_long_name() {
                continue;
            }

            if entry.is_volume_label() {
                continue;
            }

            let name_bytes = entry.full_name();
            let name = String::from_utf8_lossy(&name_bytes).to_string();

            if name == "." || name == ".." {
                continue;
            }

            result.push(DirEntryInfo {
                name,
                is_dir: entry.is_directory(),
                size: entry.file_size,
                cluster: entry.first_cluster(),
            });
        }
    }

    Ok(result)
}

pub fn find_entry(
    fs: &Fat32Fs,
    dir_cluster: u32,
    name_upper: &[u8],
) -> Result<Option<Fat32DirEntry>, &'static str> {
    let chain = fat::read_chain(fs, dir_cluster)?;
    let cluster_size = fs.bpb.cluster_size_bytes();
    let entry_size = 32;
    let entries_per_cluster = cluster_size / entry_size;

    for &cluster in &chain {
        let mut cluster_buf = alloc::vec![0u8; cluster_size];
        fs.read_cluster(cluster, &mut cluster_buf)?;

        for i in 0..entries_per_cluster {
            let offset = i * entry_size;
            let entry = unsafe {
                core::ptr::read_unaligned(cluster_buf[offset..].as_ptr() as *const Fat32DirEntry)
            };

            if entry.is_end() {
                return Ok(None);
            }
            if entry.is_deleted() || entry.is_long_name() || entry.is_volume_label() {
                continue;
            }

            let entry_name = entry.full_name_upper();
            if entry_name == name_upper {
                return Ok(Some(entry));
            }
        }
    }

    Ok(None)
}

pub fn add_entry(
    fs: &Fat32Fs,
    dir_cluster: u32,
    new_entry: &Fat32DirEntry,
) -> Result<(), &'static str> {
    let chain = fat::read_chain(fs, dir_cluster)?;
    let cluster_size = fs.bpb.cluster_size_bytes();
    let entry_size = 32;
    let entries_per_cluster = cluster_size / entry_size;

    for &cluster in &chain {
        let mut cluster_buf = alloc::vec![0u8; cluster_size];
        fs.read_cluster(cluster, &mut cluster_buf)?;

        for i in 0..entries_per_cluster {
            let offset = i * entry_size;
            let entry = unsafe {
                core::ptr::read_unaligned(cluster_buf[offset..].as_ptr() as *const Fat32DirEntry)
            };

            if entry.is_end() || entry.is_deleted() {
                cluster_buf[offset..offset + entry_size]
                    .copy_from_slice(unsafe {
                        core::slice::from_raw_parts(
                            new_entry as *const Fat32DirEntry as *const u8,
                            entry_size,
                        )
                    });
                fs.write_cluster(cluster, &cluster_buf)?;
                return Ok(());
            }
        }
    }

    let new_cluster = fat::alloc_cluster(fs)?;
    let mut cluster_buf = alloc::vec![0u8; cluster_size];
    cluster_buf[..entry_size].copy_from_slice(unsafe {
        core::slice::from_raw_parts(
            new_entry as *const Fat32DirEntry as *const u8,
            entry_size,
        )
    });

    let last_cluster = *chain.last().ok_or("Empty chain")?;
    fat::write_fat_entry(fs, last_cluster, new_cluster)?;
    fat::write_fat_entry(fs, new_cluster, crate::fs::fat32::bpb::FAT32_EOC)?;
    fs.write_cluster(new_cluster, &cluster_buf)?;

    Ok(())
}

pub fn remove_entry(
    fs: &Fat32Fs,
    dir_cluster: u32,
    name_upper: &[u8],
) -> Result<Fat32DirEntry, &'static str> {
    let chain = fat::read_chain(fs, dir_cluster)?;
    let cluster_size = fs.bpb.cluster_size_bytes();
    let entry_size = 32;
    let entries_per_cluster = cluster_size / entry_size;

    for &cluster in &chain {
        let mut cluster_buf = alloc::vec![0u8; cluster_size];
        fs.read_cluster(cluster, &mut cluster_buf)?;

        for i in 0..entries_per_cluster {
            let offset = i * entry_size;
            let entry = unsafe {
                core::ptr::read_unaligned(cluster_buf[offset..].as_ptr() as *const Fat32DirEntry)
            };

            if entry.is_end() {
                return Err("File not found");
            }
            if entry.is_deleted() || entry.is_long_name() || entry.is_volume_label() {
                continue;
            }

            let entry_name = entry.full_name_upper();
            if entry_name == name_upper {
                let removed = entry;
                cluster_buf[offset] = 0xE5;
                fs.write_cluster(cluster, &cluster_buf)?;
                return Ok(removed);
            }
        }
    }

    Err("File not found")
}
