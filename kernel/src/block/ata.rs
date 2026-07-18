use alloc::string::{String, ToString};
use crate::block::{BlockDevice, BlockError};
use crate::drivers::ata::{ata as get_ata, AtaDriver, PRIMARY_BASE, SECTOR_SIZE};

pub struct AtaBlockDevice {
    base_port: u16,
    slave: bool,
}

impl AtaBlockDevice {
    pub fn primary_master() -> Self {
        AtaBlockDevice {
            base_port: PRIMARY_BASE,
            slave: false,
        }
    }

    pub fn new(base_port: u16, slave: bool) -> Self {
        AtaBlockDevice { base_port, slave }
    }

    pub fn device_info(&self) -> Option<(String, String, u32)> {
        let lock = get_ata();
        let driver: &AtaDriver = lock.as_ref()?;
        let info = driver.device_info(self.base_port, self.slave);
        if !info.present {
            return None;
        }
        let model = String::from_utf8_lossy(&info.model).trim().to_string();
        let serial = String::from_utf8_lossy(&info.serial).trim().to_string();
        let sectors = info.sectors_28;
        Some((model, serial, sectors))
    }
}

impl BlockDevice for AtaBlockDevice {
    fn read_sector(&self, lba: u32, buf: &mut [u8]) -> Result<(), BlockError> {
        let mut lock = get_ata();
        let driver: &mut AtaDriver = lock.as_mut().ok_or(BlockError::DeviceNotFound)?;
        driver
            .read_sectors(self.base_port, self.slave, lba, 1, buf)
            .map_err(|_| BlockError::IoError)
    }

    fn write_sector(&self, lba: u32, buf: &[u8]) -> Result<(), BlockError> {
        let mut lock = get_ata();
        let driver: &mut AtaDriver = lock.as_mut().ok_or(BlockError::DeviceNotFound)?;
        driver
            .write_sectors(self.base_port, self.slave, lba, 1, buf)
            .map_err(|_| BlockError::IoError)
    }

    fn sector_size(&self) -> usize {
        SECTOR_SIZE
    }

    fn total_sectors(&self) -> u32 {
        let lock = get_ata();
        lock.as_ref()
            .map(|d| d.device_info(self.base_port, self.slave).sectors_28)
            .unwrap_or(0)
    }

    fn name(&self) -> &str {
        if self.slave {
            "ATA-primary-slave"
        } else {
            "ATA-primary-master"
        }
    }
}
