use x86_64::instructions::port::Port;

#[derive(Debug, Clone, Copy)]
pub struct PciDevice {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class: u8,
    pub subclass: u8,
    pub header_type: u8,
}

impl PciDevice {
    pub fn new(bus: u8, device: u8, function: u8) -> Option<Self> {
        let vendor_id = pci_read_u16(bus, device, function, 0);
        if vendor_id == 0xFFFF {
            return None;
        }
        let device_id = pci_read_u16(bus, device, function, 2);
        let class_rev = pci_read_u32(bus, device, function, 8);
        let header_type = pci_read_u8(bus, device, function, 14);

        Some(PciDevice {
            bus,
            device,
            function,
            vendor_id,
            device_id,
            class: (class_rev >> 24) as u8,
            subclass: (class_rev >> 16) as u8,
            header_type,
        })
    }

    pub fn is_bridge(&self) -> bool {
        self.class == 0x06 && self.subclass == 0x04
    }
}

pub fn scan_pci_bus() -> alloc::vec::Vec<PciDevice> {
    let mut devices = alloc::vec::Vec::new();

    for bus in 0..256u16 {
        for device in 0..32 {
            if let Some(pci_device) = PciDevice::new(bus as u8, device, 0) {
                // Check if multi-function device (bit 7 of header_type).
                let multi_func = pci_device.header_type & 0x80 != 0;
                devices.push(pci_device);

                if pci_device.is_bridge() {
                    let sub_bus = pci_read_u8(bus as u8, device, 0, 0x19);
                    for sub_device in 0..32 {
                        if let Some(sub_pci) = PciDevice::new(sub_bus, sub_device, 0) {
                            devices.push(sub_pci);
                        }
                    }
                }

                // Scan additional functions 1-7 for multi-function devices.
                if multi_func {
                    for func in 1u8..8 {
                        if let Some(func_dev) = PciDevice::new(bus as u8, device, func) {
                            devices.push(func_dev);
                        }
                    }
                }
            }
        }
    }

    devices
}

fn pci_read_u8(bus: u8, device: u8, function: u8, offset: u8) -> u8 {
    let mut port = Port::<u32>::new(0xCF8);
    let address = 0x80000000u32
        | ((bus as u32) << 16)
        | ((device as u32) << 11)
        | ((function as u32) << 8)
        | ((offset as u32) & 0xFC);
    unsafe {
        port.write(address);
        Port::<u32>::new(0xCFC).read() as u8
    }
}

fn pci_read_u16(bus: u8, device: u8, function: u8, offset: u8) -> u16 {
    let mut port = Port::<u32>::new(0xCF8);
    let address = 0x80000000u32
        | ((bus as u32) << 16)
        | ((device as u32) << 11)
        | ((function as u32) << 8)
        | ((offset as u32) & 0xFC);
    unsafe {
        port.write(address);
        Port::<u32>::new(0xCFC).read() as u16
    }
}

fn pci_read_u32(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    let mut port = Port::<u32>::new(0xCF8);
    let address = 0x80000000u32
        | ((bus as u32) << 16)
        | ((device as u32) << 11)
        | ((function as u32) << 8)
        | ((offset as u32) & 0xFC);
    unsafe {
        port.write(address);
        Port::<u32>::new(0xCFC).read()
    }
}
