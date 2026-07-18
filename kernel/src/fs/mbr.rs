use crate::drivers::ata::{ata, PRIMARY_BASE, SECTOR_SIZE};

pub const MBR_BOOT_SIG: u16 = 0xAA55;
pub const MAX_PARTITIONS: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PartitionType {
    Empty,
    Fat12,
    Fat16Small,
    Fat16Large,
    Fat32Chs,
    Fat32Lba,
    Ntfs,
    LinuxSwap,
    Linux,
    Extended,
    Unknown(u8),
}

impl PartitionType {
    pub fn from_byte(byte: u8) -> Self {
        match byte {
            0x00 => PartitionType::Empty,
            0x01 => PartitionType::Fat12,
            0x04 | 0x06 => PartitionType::Fat16Small,
            0x0B => PartitionType::Fat32Chs,
            0x0C => PartitionType::Fat32Lba,
            0x07 => PartitionType::Ntfs,
            0x82 => PartitionType::LinuxSwap,
            0x83 => PartitionType::Linux,
            0x05 | 0x0F => PartitionType::Extended,
            other => PartitionType::Unknown(other),
        }
    }

    pub fn name(&self) -> &str {
        match self {
            PartitionType::Empty => "Empty",
            PartitionType::Fat12 => "FAT12",
            PartitionType::Fat16Small => "FAT16",
            PartitionType::Fat16Large => "FAT16 (Large)",
            PartitionType::Fat32Chs => "FAT32 (CHS)",
            PartitionType::Fat32Lba => "FAT32 (LBA)",
            PartitionType::Ntfs => "NTFS",
            PartitionType::LinuxSwap => "Linux Swap",
            PartitionType::Linux => "Linux",
            PartitionType::Extended => "Extended",
            PartitionType::Unknown(_) => "Unknown",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PartitionEntry {
    pub boot_indicator: u8,
    pub partition_type: PartitionType,
    pub type_byte: u8,
    pub chs_first: [u8; 3],
    pub chs_last: [u8; 3],
    pub lba_first: u32,
    pub sector_count: u32,
}

impl PartitionEntry {
    pub fn is_active(&self) -> bool {
        self.boot_indicator == 0x80
    }

    pub fn is_empty(&self) -> bool {
        self.partition_type == PartitionType::Empty
    }

    pub fn lba_last(&self) -> u32 {
        if self.sector_count > 0 {
            self.lba_first + self.sector_count - 1
        } else {
            self.lba_first
        }
    }

    pub fn size_bytes(&self) -> u64 {
        self.sector_count as u64 * SECTOR_SIZE as u64
    }

    pub fn size_human(&self) -> alloc::string::String {
        let bytes = self.size_bytes();
        if bytes >= 1024 * 1024 * 1024 {
            alloc::format!("{:.1} GiB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
        } else if bytes >= 1024 * 1024 {
            alloc::format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
        } else if bytes >= 1024 {
            alloc::format!("{:.1} KiB", bytes as f64 / 1024.0)
        } else {
            alloc::format!("{} B", bytes)
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MbrTable {
    pub boot_sig: u16,
    pub partitions: [PartitionEntry; MAX_PARTITIONS],
    pub valid: bool,
}

impl MbrTable {
    pub fn new() -> Self {
        MbrTable {
            boot_sig: 0,
            partitions: [PartitionEntry {
                boot_indicator: 0,
                partition_type: PartitionType::Empty,
                type_byte: 0,
                chs_first: [0; 3],
                chs_last: [0; 3],
                lba_first: 0,
                sector_count: 0,
            }; MAX_PARTITIONS],
            valid: false,
        }
    }

    pub fn parse(sector: &[u8]) -> Self {
        if sector.len() < SECTOR_SIZE {
            return MbrTable::new();
        }

        let boot_sig = u16::from_le_bytes([sector[510], sector[511]]);
        let valid = boot_sig == MBR_BOOT_SIG;

        let mut partitions = [PartitionEntry {
            boot_indicator: 0,
            partition_type: PartitionType::Empty,
            type_byte: 0,
            chs_first: [0; 3],
            chs_last: [0; 3],
            lba_first: 0,
            sector_count: 0,
        }; MAX_PARTITIONS];

        for i in 0..MAX_PARTITIONS {
            let offset = 446 + i * 16;
            let entry = &sector[offset..offset + 16];

            let type_byte = entry[4];
            let boot_indicator = entry[0];
            let lba_first = u32::from_le_bytes([entry[8], entry[9], entry[10], entry[11]]);
            let sector_count = u32::from_le_bytes([entry[12], entry[13], entry[14], entry[15]]);

            let mut chs_first = [0u8; 3];
            let mut chs_last = [0u8; 3];
            chs_first.copy_from_slice(&entry[1..4]);
            chs_last.copy_from_slice(&entry[5..8]);

            partitions[i] = PartitionEntry {
                boot_indicator,
                partition_type: PartitionType::from_byte(type_byte),
                type_byte,
                chs_first,
                chs_last,
                lba_first,
                sector_count,
            };
        }

        MbrTable {
            boot_sig,
            partitions,
            valid,
        }
    }

    pub fn partition_count(&self) -> usize {
        if !self.valid {
            return 0;
        }
        self.partitions
            .iter()
            .filter(|p| !p.is_empty())
            .count()
    }

    pub fn active_partition(&self) -> Option<&PartitionEntry> {
        self.partitions.iter().find(|p| p.is_active())
    }
}

pub fn read_mbr() -> Result<MbrTable, &'static str> {
    let mut buf = alloc::vec![0u8; SECTOR_SIZE];

    {
        let mut ata_lock = ata();
        let driver = match ata_lock.as_mut() {
            Some(d) => d,
            None => return Err("No ATA driver"),
        };
        driver
            .read_sectors(PRIMARY_BASE, false, 0, 1, &mut buf)
            .map_err(|_| "Read error")?;
    }

    Ok(MbrTable::parse(&buf))
}
