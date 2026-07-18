use alloc::vec::Vec;
use spin::Mutex;
use x86_64::instructions::port::Port;

use super::{Driver, DriverError, DriverStatus};

pub const PRIMARY_BASE: u16 = 0x1F0;
const PRIMARY_CONTROL: u16 = 0x3F6;
const SECONDARY_BASE: u16 = 0x170;
const SECONDARY_CONTROL: u16 = 0x376;

pub const SECTOR_SIZE: usize = 512;

const STATUS_ERR: u8 = 0x01;
const STATUS_DRQ: u8 = 0x08;
const STATUS_DF: u8 = 0x20;
const STATUS_BSY: u8 = 0x80;

const CMD_READ_SECTORS: u8 = 0x20;
const CMD_WRITE_SECTORS: u8 = 0x30;
const CMD_IDENTIFY_DRIVE: u8 = 0xEC;
const CMD_FLUSH_CACHE: u8 = 0xE7;

const MAX_RETRIES: u32 = 3;
const POLL_TIMEOUT: u32 = 100_000;

struct AtaChannel {
    base_port: u16,
    control_port: u16,
}

impl AtaChannel {
    const fn new(base_port: u16, control_port: u16) -> Self {
        AtaChannel {
            base_port,
            control_port,
        }
    }

    unsafe fn read_reg(&self, offset: u16) -> u8 {
        Port::<u8>::new(self.base_port + offset).read()
    }

    unsafe fn write_reg(&self, offset: u16, value: u8) {
        Port::<u8>::new(self.base_port + offset).write(value);
    }

    unsafe fn read_data_word(&self) -> u16 {
        Port::<u16>::new(self.base_port).read()
    }

    unsafe fn write_data_word(&self, value: u16) {
        Port::<u16>::new(self.base_port).write(value);
    }

    unsafe fn read_alt_status(&self) -> u8 {
        Port::<u8>::new(self.control_port).read()
    }

    unsafe fn write_control(&self, value: u8) {
        Port::<u8>::new(self.control_port).write(value);
    }
}

#[derive(Debug, Clone)]
pub struct AtaDeviceInfo {
    pub present: bool,
    pub is_ata: bool,
    pub sectors_28: u32,
    pub lba48: bool,
    pub model: Vec<u8>,
    pub serial: Vec<u8>,
    pub firmware: Vec<u8>,
}

impl AtaDeviceInfo {
    const fn new() -> Self {
        AtaDeviceInfo {
            present: false,
            is_ata: false,
            sectors_28: 0,
            lba48: false,
            model: Vec::new(),
            serial: Vec::new(),
            firmware: Vec::new(),
        }
    }
}

pub struct AtaDriver {
    status: DriverStatus,
    primary_master: AtaDeviceInfo,
    primary_slave: AtaDeviceInfo,
    secondary_master: AtaDeviceInfo,
    secondary_slave: AtaDeviceInfo,
}

impl AtaDriver {
    pub fn new() -> Self {
        AtaDriver {
            status: DriverStatus::Uninitialized,
            primary_master: AtaDeviceInfo::new(),
            primary_slave: AtaDeviceInfo::new(),
            secondary_master: AtaDeviceInfo::new(),
            secondary_slave: AtaDeviceInfo::new(),
        }
    }

    fn get_channel(&self, base_port: u16) -> AtaChannel {
        match base_port {
            PRIMARY_BASE => AtaChannel::new(PRIMARY_BASE, PRIMARY_CONTROL),
            SECONDARY_BASE => AtaChannel::new(SECONDARY_BASE, SECONDARY_CONTROL),
            _ => AtaChannel::new(PRIMARY_BASE, PRIMARY_CONTROL),
        }
    }

    pub fn device_info(&self, base_port: u16, slave: bool) -> &AtaDeviceInfo {
        match (base_port, slave) {
            (PRIMARY_BASE, false) => &self.primary_master,
            (PRIMARY_BASE, true) => &self.primary_slave,
            (SECONDARY_BASE, false) => &self.secondary_master,
            (SECONDARY_BASE, true) => &self.secondary_slave,
            _ => &self.primary_master,
        }
    }

    unsafe fn select_device(&self, channel: &AtaChannel, slave: bool) {
        let head: u8 = if slave { 0xB0 } else { 0xA0 };
        channel.write_reg(6, head);
        // ATA spec requires 400ns delay after device select.
        // 4 reads of the alternate status register (~100ns each) = ~400ns.
        for _ in 0..4 {
            channel.read_alt_status();
        }
    }

    unsafe fn software_reset(&self, channel: &AtaChannel) {
        channel.write_control(0x04);
        for _ in 0..4 {
            channel.read_alt_status();
        }
        channel.write_control(0x00);
        for _ in 0..4 {
            channel.read_alt_status();
        }
    }

    unsafe fn poll_bsy(&self, channel: &AtaChannel) -> Result<(), DriverError> {
        for _ in 0..POLL_TIMEOUT {
            let status = channel.read_reg(7);
            if status & STATUS_BSY == 0 {
                return Ok(());
            }
        }
        Err(DriverError::Timeout)
    }

    unsafe fn poll_drq(&self, channel: &AtaChannel) -> Result<(), DriverError> {
        for _ in 0..POLL_TIMEOUT {
            let status = channel.read_reg(7);
            if status & STATUS_BSY == 0 && status & STATUS_DRQ != 0 {
                return Ok(());
            }
            if status & (STATUS_ERR | STATUS_DF) != 0 {
                return Err(DriverError::IoError);
            }
        }
        Err(DriverError::Timeout)
    }

    unsafe fn poll_not_busy(&self, channel: &AtaChannel) -> Result<(), DriverError> {
        for _ in 0..POLL_TIMEOUT {
            let status = channel.read_reg(7);
            if status & STATUS_BSY == 0 {
                return Ok(());
            }
        }
        Err(DriverError::Timeout)
    }

    fn swap_bytes(word: u16) -> [u8; 2] {
        [(word >> 8) as u8, word as u8]
    }

    fn extract_string(data: &[u16; 256], start_word: usize, end_word: usize) -> Vec<u8> {
        let mut bytes = Vec::new();
        for i in start_word..=end_word {
            let swapped = Self::swap_bytes(data[i]);
            bytes.push(swapped[0]);
            bytes.push(swapped[1]);
        }
        while bytes.last() == Some(&b' ') || bytes.last() == Some(&0) {
            bytes.pop();
        }
        bytes
    }

    unsafe fn probe_device(&self, channel: &AtaChannel, slave: bool) -> AtaDeviceInfo {
        let mut info = AtaDeviceInfo::new();

        self.select_device(channel, slave);

        channel.write_reg(1, 0x00);
        channel.write_reg(2, 0x00);
        channel.write_reg(3, 0x00);
        channel.write_reg(4, 0x00);
        channel.write_reg(5, 0x00);
        channel.write_reg(7, CMD_IDENTIFY_DRIVE);

        let status = channel.read_reg(7);
        if status == 0 {
            return info;
        }

        if let Err(_) = self.poll_bsy(channel) {
            return info;
        }

        let mid = channel.read_reg(4);
        let high = channel.read_reg(5);
        if mid != 0 || high != 0 {
            return info;
        }

        if let Err(_) = self.poll_drq(channel) {
            return info;
        }

        info.present = true;

        let mut identify_data = [0u16; 256];
        for i in 0..256 {
            identify_data[i] = channel.read_data_word();
        }

        if let Err(_) = self.poll_not_busy(channel) {
            info.present = false;
            return info;
        }

        info.serial = Self::extract_string(&identify_data, 10, 19);

        info.firmware = Self::extract_string(&identify_data, 23, 26);
        info.model = Self::extract_string(&identify_data, 27, 46);

        info.sectors_28 = ((identify_data[61] as u32) << 16) | (identify_data[60] as u32);

        if identify_data[83] & (1 << 10) != 0 {
            info.lba48 = true;
        }

        info.is_ata = true;

        info
    }

    pub fn read_sectors(
        &mut self,
        channel_base: u16,
        slave: bool,
        lba: u32,
        count: u32,
        buf: &mut [u8],
    ) -> Result<(), DriverError> {
        let channel = self.get_channel(channel_base);
        let bytes_needed = (count as usize) * SECTOR_SIZE;
        if buf.len() < bytes_needed {
            return Err(DriverError::BufferTooSmall);
        }
        // Prevent overflow on LBA + sector_offset.
        if count > 0 && lba.checked_add(count - 1).is_none() {
            return Err(DriverError::InvalidParam);
        }

        for sector_offset in 0..count {
            let sector_lba = lba + sector_offset;
            let mut last_error = Err(DriverError::IoError);

            for _attempt in 0..MAX_RETRIES {
                unsafe {
                    self.select_device(&channel, slave);
                    channel.write_reg(1, 0x00);
                    channel.write_reg(2, 0x01);
                    channel.write_reg(3, (sector_lba & 0xFF) as u8);
                    channel.write_reg(4, ((sector_lba >> 8) & 0xFF) as u8);
                    channel.write_reg(5, ((sector_lba >> 16) & 0xFF) as u8);
                    channel.write_reg(
                        6,
                        0xE0 | if slave { 0x10 } else { 0x00 }
                            | ((sector_lba >> 24) & 0x0F) as u8,
                    );
                    channel.write_reg(7, CMD_READ_SECTORS);

                    match self.poll_drq(&channel) {
                        Ok(()) => {}
                        Err(e) => {
                            last_error = Err(e);
                            self.software_reset(&channel);
                            continue;
                        }
                    }

                    let buf_offset = (sector_offset as usize) * SECTOR_SIZE;
                    for i in 0..256 {
                        let word = channel.read_data_word();
                        let byte_idx = buf_offset + i * 2;
                        if byte_idx + 1 < buf.len() {
                            buf[byte_idx] = (word & 0xFF) as u8;
                            buf[byte_idx + 1] = (word >> 8) as u8;
                        }
                    }

                    if let Err(e) = self.poll_not_busy(&channel) {
                        last_error = Err(e);
                        self.software_reset(&channel);
                        continue;
                    }

                    let status = channel.read_reg(7);
                    if status & (STATUS_ERR | STATUS_DF) != 0 {
                        last_error = Err(DriverError::IoError);
                        self.software_reset(&channel);
                        continue;
                    }

                    last_error = Ok(());
                    break;
                }
            }

            last_error?;
        }

        Ok(())
    }

    pub fn write_sectors(
        &mut self,
        channel_base: u16,
        slave: bool,
        lba: u32,
        count: u32,
        buf: &[u8],
    ) -> Result<(), DriverError> {
        let channel = self.get_channel(channel_base);
        let bytes_needed = (count as usize) * SECTOR_SIZE;
        if buf.len() < bytes_needed {
            return Err(DriverError::BufferTooSmall);
        }
        // Prevent overflow on LBA + sector_offset.
        if count > 0 && lba.checked_add(count - 1).is_none() {
            return Err(DriverError::InvalidParam);
        }

        for sector_offset in 0..count {
            let sector_lba = lba + sector_offset;
            let mut last_error = Err(DriverError::IoError);

            for _attempt in 0..MAX_RETRIES {
                unsafe {
                    self.select_device(&channel, slave);
                    channel.write_reg(1, 0x00);
                    channel.write_reg(2, 0x01);
                    channel.write_reg(3, (sector_lba & 0xFF) as u8);
                    channel.write_reg(4, ((sector_lba >> 8) & 0xFF) as u8);
                    channel.write_reg(5, ((sector_lba >> 16) & 0xFF) as u8);
                    channel.write_reg(
                        6,
                        0xE0 | if slave { 0x10 } else { 0x00 }
                            | ((sector_lba >> 24) & 0x0F) as u8,
                    );
                    channel.write_reg(7, CMD_WRITE_SECTORS);

                    match self.poll_drq(&channel) {
                        Ok(()) => {}
                        Err(e) => {
                            last_error = Err(e);
                            self.software_reset(&channel);
                            continue;
                        }
                    }

                    let buf_offset = (sector_offset as usize) * SECTOR_SIZE;
                    for i in 0..256 {
                        let byte_idx = buf_offset + i * 2;
                        let word: u16 = if byte_idx + 1 < buf.len() {
                            (buf[byte_idx] as u16) | ((buf[byte_idx + 1] as u16) << 8)
                        } else {
                            0
                        };
                        channel.write_data_word(word);
                    }

                    if let Err(e) = self.poll_not_busy(&channel) {
                        last_error = Err(e);
                        self.software_reset(&channel);
                        continue;
                    }

                    let status = channel.read_reg(7);
                    if status & (STATUS_ERR | STATUS_DF) != 0 {
                        last_error = Err(DriverError::IoError);
                        self.software_reset(&channel);
                        continue;
                    }

                    channel.write_reg(7, CMD_FLUSH_CACHE);

                    if let Err(e) = self.poll_not_busy(&channel) {
                        last_error = Err(e);
                        self.software_reset(&channel);
                        continue;
                    }

                    let status = channel.read_reg(7);
                    if status & (STATUS_ERR | STATUS_DF) != 0 {
                        last_error = Err(DriverError::IoError);
                        self.software_reset(&channel);
                        continue;
                    }

                    last_error = Ok(());
                    break;
                }
            }

            last_error?;
        }

        Ok(())
    }
}

impl Driver for AtaDriver {
    fn name(&self) -> &str {
        "ATA PIO Driver"
    }

    fn init(&mut self) -> Result<(), DriverError> {
        self.status = DriverStatus::Initializing;

        let primary = self.get_channel(PRIMARY_BASE);
        let secondary = self.get_channel(SECONDARY_BASE);

        unsafe {
            self.software_reset(&primary);
            self.software_reset(&secondary);
        }

        unsafe {
            self.primary_master = self.probe_device(&primary, false);
            self.primary_slave = self.probe_device(&primary, true);
            self.secondary_master = self.probe_device(&secondary, false);
            self.secondary_slave = self.probe_device(&secondary, true);
        }

        let mut found = false;

        if self.primary_master.present && self.primary_master.is_ata {
            crate::serial_println!(
                "[ATA] Primary Master: {}",
                core::str::from_utf8(&self.primary_master.model).unwrap_or("Unknown")
            );
            found = true;
        }
        if self.primary_slave.present && self.primary_slave.is_ata {
            crate::serial_println!(
                "[ATA] Primary Slave: {}",
                core::str::from_utf8(&self.primary_slave.model).unwrap_or("Unknown")
            );
            found = true;
        }
        if self.secondary_master.present && self.secondary_master.is_ata {
            crate::serial_println!(
                "[ATA] Secondary Master: {}",
                core::str::from_utf8(&self.secondary_master.model).unwrap_or("Unknown")
            );
            found = true;
        }
        if self.secondary_slave.present && self.secondary_slave.is_ata {
            crate::serial_println!(
                "[ATA] Secondary Slave: {}",
                core::str::from_utf8(&self.secondary_slave.model).unwrap_or("Unknown")
            );
            found = true;
        }

        if found {
            self.status = DriverStatus::Ready;
            Ok(())
        } else {
            self.status = DriverStatus::Error;
            Err(DriverError::DeviceNotFound)
        }
    }

    fn status(&self) -> DriverStatus {
        self.status
    }

    fn reset(&mut self) -> Result<(), DriverError> {
        self.status = DriverStatus::Initializing;

        let primary = self.get_channel(PRIMARY_BASE);
        let secondary = self.get_channel(SECONDARY_BASE);

        unsafe {
            self.software_reset(&primary);
            self.software_reset(&secondary);
        }

        unsafe {
            self.primary_master = self.probe_device(&primary, false);
            self.primary_slave = self.probe_device(&primary, true);
            self.secondary_master = self.probe_device(&secondary, false);
            self.secondary_slave = self.probe_device(&secondary, true);
        }

        let found = self.primary_master.present
            || self.primary_slave.present
            || self.secondary_master.present
            || self.secondary_slave.present;

        if found {
            self.status = DriverStatus::Ready;
        } else {
            self.status = DriverStatus::Error;
        }

        Ok(())
    }

    fn shutdown(&mut self) {
        self.status = DriverStatus::Disabled;
    }

    fn probe(&self) -> bool {
        self.primary_master.present
            || self.primary_slave.present
            || self.secondary_master.present
            || self.secondary_slave.present
    }
}

static ATA: Mutex<Option<AtaDriver>> = Mutex::new(None);

pub fn ata() -> spin::MutexGuard<'static, Option<AtaDriver>> {
    ATA.lock()
}

pub fn init_ata_driver() -> Result<(), DriverError> {
    let mut driver = AtaDriver::new();
    driver.init()?;
    *ATA.lock() = Some(driver);
    Ok(())
}
