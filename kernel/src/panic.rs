use core::panic::PanicInfo;
use crate::{println, serial_println};

pub fn panic_handler(info: &PanicInfo) -> ! {
    x86_64::instructions::interrupts::disable();
    println!("[KERNEL PANIC]");
    println!("{}", info);
    serial_println!("[KERNEL PANIC]");
    serial_println!("{}", info);

    loop {
        x86_64::instructions::hlt();
    }
}