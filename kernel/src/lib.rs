#![no_std]
#![feature(alloc_error_handler)]
#![feature(abi_x86_interrupt)]

extern crate alloc;

pub mod arch;
pub mod block;
pub mod drivers;
pub mod elf;
pub mod fs;
pub mod interrupts;
pub mod memory;
pub mod panic;
pub mod process;
pub mod serial;
pub mod sysinfo;
pub mod terminal;
pub mod time;
pub mod version;
pub mod vga_buffer;

use core::panic::PanicInfo;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    crate::panic::panic_handler(info)
}

#[alloc_error_handler]
fn alloc_error(layout: core::alloc::Layout) -> ! {
    panic!("Allocation error: {:?}", layout);
}
