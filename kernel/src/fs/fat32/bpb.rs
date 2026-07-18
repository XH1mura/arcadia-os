use alloc::vec::Vec;

pub const FAT32_EOC: u32 = 0x0FFFFFF8;
pub const FAT32_BAD: u32 = 0x0FFFFFF7;
pub const FAT32_FREE: u32 = 0x00000000;

pub const ATTR_READ_ONLY: u8 = 0x01;
pub const ATTR_HIDDEN: u8 = 0x02;
pub const ATTR_SYSTEM: u8 = 0x04;
pub const ATTR_VOLUME_ID: u8 = 0x08;
pub const ATTR_DIRECTORY: u8 = 0x10;
pub const ATTR_ARCHIVE: u8 = 0x20;
pub const ATTR_LONG_FILE_NAME: u8 = 0x0F;

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct Fat32Bpb {
    pub jump_boot: [u8; 3],
    pub oem_name: [u8; 8],
    pub bytes_per_sector: u16,
    pub sectors_per_cluster: u8,
    pub reserved_sector_count: u16,
    pub num_fats: u8,
    pub root_entry_count: u16,
    pub total_sectors_16: u16,
    pub media_type: u8,
    pub fat_size_16: u16,
    pub sectors_per_track: u16,
    pub num_heads: u16,
    pub hidden_sectors: u32,
    pub total_sectors_32: u32,
    pub fat_size_32: u32,
    pub ext_flags: u16,
    pub fs_version: u16,
    pub root_cluster: u32,
    pub fs_info_sector: u16,
    pub backup_boot_sector: u16,
    pub reserved: [u8; 12],
    pub drive_number: u8,
    pub reserved_nt: u8,
    pub boot_signature: u8,
    pub volume_serial: u32,
    pub volume_label: [u8; 11],
    pub fs_type: [u8; 8],
}

impl Fat32Bpb {
    pub fn from_bytes(sector: &[u8]) -> Option<Self> {
        if sector.len() < 90 {
            return None;
        }

        let bpb = unsafe { core::ptr::read_unaligned(sector.as_ptr() as *const Fat32Bpb) };

        if bpb.fat_size_16 != 0 {
            return None;
        }
        if bpb.fat_size_32 == 0 {
            return None;
        }

        let fs_type_str = core::str::from_utf8(&bpb.fs_type).unwrap_or("");
        if !fs_type_str.starts_with("FAT32") {
            return None;
        }

        Some(bpb)
    }

    pub fn bytes_per_sector(&self) -> usize {
        self.bytes_per_sector as usize
    }

    pub fn cluster_size_bytes(&self) -> usize {
        self.sectors_per_cluster as usize * self.bytes_per_sector()
    }

    pub fn fat_start_sector(&self) -> u32 {
        self.reserved_sector_count as u32
    }

    pub fn data_start_sector(&self) -> u32 {
        self.reserved_sector_count as u32
            + self.num_fats as u32 * self.fat_size_32
    }

    pub fn cluster_to_sector(&self, cluster: u32) -> u32 {
        self.data_start_sector() + (cluster - 2) * self.sectors_per_cluster as u32
    }

    pub fn total_clusters(&self) -> u32 {
        let data_sectors = self.total_sectors_32.saturating_sub(self.data_start_sector());
        data_sectors / self.sectors_per_cluster as u32
    }

    pub fn volume_label_str(&self) -> &str {
        core::str::from_utf8(&self.volume_label).unwrap_or("           ").trim()
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct Fat32DirEntry {
    pub name: [u8; 8],
    pub ext: [u8; 3],
    pub attributes: u8,
    pub reserved_nt: u8,
    pub create_time_tenth: u8,
    pub create_time: u16,
    pub create_date: u16,
    pub access_date: u16,
    pub first_cluster_hi: u16,
    pub modify_time: u16,
    pub modify_date: u16,
    pub first_cluster_lo: u16,
    pub file_size: u32,
}

impl Fat32DirEntry {
    pub fn is_end(&self) -> bool {
        self.name[0] == 0x00
    }

    pub fn is_deleted(&self) -> bool {
        self.name[0] == 0xE5
    }

    pub fn is_long_name(&self) -> bool {
        self.attributes & ATTR_LONG_FILE_NAME == ATTR_LONG_FILE_NAME
    }

    pub fn is_directory(&self) -> bool {
        self.attributes & ATTR_DIRECTORY != 0
    }

    pub fn is_volume_label(&self) -> bool {
        self.attributes & ATTR_VOLUME_ID != 0
    }

    pub fn first_cluster(&self) -> u32 {
        ((self.first_cluster_hi as u32) << 16) | (self.first_cluster_lo as u32)
    }

    pub fn set_first_cluster(&mut self, cluster: u32) {
        self.first_cluster_hi = ((cluster >> 16) & 0xFFFF) as u16;
        self.first_cluster_lo = (cluster & 0xFFFF) as u16;
    }

    pub fn full_name(&self) -> Vec<u8> {
        let mut result = Vec::new();
        for &b in &self.name {
            if b == b' ' || b == 0 {
                break;
            }
            result.push(if b >= b'A' && b <= b'Z' { b + 32 } else { b });
        }
        if self.ext[0] != b' ' && self.ext[0] != 0 {
            result.push(b'.');
            for &b in &self.ext {
                if b == b' ' || b == 0 {
                    break;
                }
                result.push(if b >= b'A' && b <= b'Z' { b + 32 } else { b });
            }
        }
        result
    }

    pub fn full_name_upper(&self) -> Vec<u8> {
        let mut result = Vec::new();
        for &b in &self.name {
            if b == b' ' || b == 0 {
                break;
            }
            result.push(b);
        }
        if self.ext[0] != b' ' && self.ext[0] != 0 {
            result.push(b'.');
            for &b in &self.ext {
                if b == b' ' || b == 0 {
                    break;
                }
                result.push(b);
            }
        }
        result
    }

    pub fn set_name_83(&mut self, name_upper: &[u8]) {
        self.name = [b' '; 8];
        self.ext = [b' '; 3];

        let mut dot_pos = name_upper.len();
        for (i, &b) in name_upper.iter().enumerate() {
            if b == b'.' {
                dot_pos = i;
                break;
            }
        }

        let name_part = &name_upper[..dot_pos];
        let copy_len = name_part.len().min(8);
        self.name[..copy_len].copy_from_slice(&name_part[..copy_len]);

        if dot_pos < name_upper.len() - 1 {
            let ext_part = &name_upper[dot_pos + 1..];
            let copy_len = ext_part.len().min(3);
            self.ext[..copy_len].copy_from_slice(&ext_part[..copy_len]);
        }
    }
}
