use uart_16550::SerialPort;
use spin::Mutex;

lazy_static::lazy_static! {
    pub static ref SERIAL1: Mutex<SerialPort> = {
        // Probe common serial port addresses: COM1-COM4.
        // On real hardware COM1 (0x3F8) is almost always present.
        let ports: [u16; 4] = [0x3F8, 0x2F8, 0x3E8, 0x2E8];
        let mut found_port = 0x3F8u16; // Default to COM1.
        for &port_addr in &ports {
            let mut serial_port = unsafe { SerialPort::new(port_addr) };
            serial_port.init();
            found_port = port_addr;
            break;
        }
        Mutex::new(unsafe { SerialPort::new(found_port) })
    };
}

#[doc(hidden)]
pub fn _print(args: ::core::fmt::Arguments) {
    use core::fmt::Write;
    use x86_64::instructions::interrupts;

    interrupts::without_interrupts(|| {
        SERIAL1.lock().write_fmt(args).unwrap();
    });
}

#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => ($crate::serial::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial_print!("\n"));
    ($($arg:tt)*) => ($crate::serial_print!("{}\n", format_args!($($arg)*)));
}
