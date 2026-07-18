use pic8259::ChainedPics;
use spin::Mutex;

pub const PIC_1_OFFSET: u8 = 32;
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

pub static PICS: Mutex<ChainedPics> =
    Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum HardwareInterrupt {
    Timer = PIC_1_OFFSET,
    Keyboard,
    Cascade,
    Serial2,
    Serial1,
}

impl HardwareInterrupt {
    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

pub fn init() {
    // Reprogram PIT to periodic mode 2 (rate generator) at 100 Hz.
    crate::time::init();

    unsafe {
        // Reprogram 8259 PIC: vector offset 32, 8086 mode
        PICS.lock().initialize();
        // Unmask IRQ0 (timer) + IRQ1 (keyboard) on master PIC
        x86_64::instructions::port::Port::<u8>::new(0x21).write(0xFCu8);
        // Mask all slave PIC IRQs
        x86_64::instructions::port::Port::<u8>::new(0xA1).write(0xFFu8);
    }

    x86_64::instructions::interrupts::enable();
}