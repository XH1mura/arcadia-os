pub mod ata;

pub use ata::AtaBlockDevice;

pub trait BlockDevice: Send + Sync {
    fn read_sector(&self, lba: u32, buf: &mut [u8]) -> Result<(), BlockError>;
    fn write_sector(&self, lba: u32, buf: &[u8]) -> Result<(), BlockError>;
    fn sector_size(&self) -> usize;
    fn total_sectors(&self) -> u32;
    fn name(&self) -> &str;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BlockError {
    IoError,
    InvalidSector,
    ReadOnly,
    DeviceNotFound,
}

impl core::fmt::Display for BlockError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BlockError::IoError => write!(f, "I/O error"),
            BlockError::InvalidSector => write!(f, "Invalid sector"),
            BlockError::ReadOnly => write!(f, "Read-only device"),
            BlockError::DeviceNotFound => write!(f, "Device not found"),
        }
    }
}
