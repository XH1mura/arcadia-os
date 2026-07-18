/// PVH kernel entry point.
/// Called from boot64.asm _pvh_start (via 32->64 long mode transition).
///
/// Initialization sequence:
/// 1. Early serial output (pre-Rust, raw I/O)
/// 2. GDT/TSS setup
/// 3. IDT + interrupt controller
/// 4. Memory management (frame allocator + heap)
/// 5. PCI bus scan
/// 6. Shell launch

#[used]
static ENTRY_POINT: extern "C" fn(u64, u64) -> ! = arcadia_kernel_main;

// -- Early serial helpers (before Rust modules are initialized) ---------------

#[inline]
unsafe fn early_outb(port: u16, val: u8) {
    core::arch::asm!("out dx, al", in("dx") port, in("al") val);
}

#[inline]
unsafe fn early_inb(port: u16) -> u8 {
    let val: u8;
    core::arch::asm!("in al, dx", out("al") val, in("dx") port);
    val
}

fn early_serial_write(b: u8) {
    unsafe {
        let mut timeout: u32 = 100_000;
        while early_inb(0x3FD) & 0x20 == 0 {
            timeout = timeout.saturating_sub(1);
            if timeout == 0 {
                break;
            }
        }
        early_outb(0x3F8, b);
    }
}

fn early_serial_print(s: &str) {
    for &b in s.as_bytes() {
        if b == b'\n' {
            early_serial_write(b'\r');
        }
        early_serial_write(b);
    }
}

// -- Boot progress display ---------------------------------------------------

fn boot_progress(vga: bool, serial: bool, phase: &str, pct: u8) {
    // Serial output
    if serial {
        early_serial_print("[BOOT] ");
        early_serial_print(phase);
        early_serial_print(" [");
        let filled = (pct / 5) as usize;
        for i in 0..20 {
            if i < filled {
                early_serial_write(b'#');
            } else {
                early_serial_write(b'-');
            }
        }
        early_serial_print("] ");
        // Manual u8 to string (no alloc before heap init)
        let mut buf = [0u8; 3];
        let s = if pct >= 100 {
            buf[0] = b'0' + (pct / 100) as u8;
            buf[1] = b'0' + ((pct / 10) % 10) as u8;
            buf[2] = b'0' + (pct % 10) as u8;
            core::str::from_utf8(&buf).unwrap_or("?")
        } else if pct >= 10 {
            buf[0] = b'0' + (pct / 10) as u8;
            buf[1] = b'0' + (pct % 10) as u8;
            core::str::from_utf8(&buf[..2]).unwrap_or("?")
        } else {
            buf[0] = b'0' + pct;
            core::str::from_utf8(&buf[..1]).unwrap_or("?")
        };
        early_serial_print(s);
        early_serial_print("%\r\n");
    }

    // VGA output (write to last line of VGA buffer)
    if vga {
        unsafe {
            let vga = 0xB8000 as *mut u8;
            let row = 24; // last row (0-indexed)
            let msg = phase.as_bytes();
            // Clear the row first
            for col in 0..80 {
                *vga.add((row * 80 + col) * 2) = b' ';
                *vga.add((row * 80 + col) * 2 + 1) = 0x07;
            }
            // Write phase name
            let offset = b"[BOOT] ".len();
            let prefix = b"[BOOT] ";
            for (i, &ch) in prefix.iter().enumerate() {
                *vga.add((row * 80 + i) * 2) = ch;
                *vga.add((row * 80 + i) * 2 + 1) = 0x0B; // light cyan
            }
            for (i, &ch) in msg.iter().enumerate().take(40) {
                *vga.add((row * 80 + offset + i) * 2) = ch;
                *vga.add((row * 80 + offset + i) * 2 + 1) = 0x0F; // white
            }
            // Write progress bar
            let bar_start = offset + msg.len() + 1;
            *vga.add((row * 80 + bar_start) * 2) = b'[';
            *vga.add((row * 80 + bar_start) * 2 + 1) = 0x08;
            for i in 0..20 {
                let ch = if i < (pct / 5) as usize { b'#' } else { b'-' };
                let color = if i < (pct / 5) as usize { 0x0A } else { 0x08 };
                *vga.add((row * 80 + bar_start + 1 + i) * 2) = ch;
                *vga.add((row * 80 + bar_start + 1 + i) * 2 + 1) = color;
            }
            *vga.add((row * 80 + bar_start + 21) * 2) = b']';
            *vga.add((row * 80 + bar_start + 21) * 2 + 1) = 0x08;
        }
    }
}

// -- Memory detection ---------------------------------------------------------

/// Write a u64 as hex to early serial (no heap allocation).
fn early_serial_write_hex(mut val: u64) {
    if val == 0 {
        early_serial_write(b'0');
        return;
    }
    let mut buf = [0u8; 16];
    let mut i = 0;
    while val > 0 {
        let nibble = (val & 0xF) as u8;
        buf[i] = if nibble < 10 {
            b'0' + nibble
        } else {
            b'A' + nibble - 10
        };
        i += 1;
        val >>= 4;
    }
    // Print in reverse
    while i > 0 {
        i -= 1;
        early_serial_write(buf[i]);
    }
}

/// Write a usize as decimal to early serial (no heap allocation).
fn early_serial_write_dec(mut val: usize) {
    if val == 0 {
        early_serial_write(b'0');
        return;
    }
    let mut buf = [0u8; 20];
    let mut i = 0;
    while val > 0 {
        buf[i] = b'0' + (val % 10) as u8;
        i += 1;
        val /= 10;
    }
    while i > 0 {
        i -= 1;
        early_serial_write(buf[i]);
    }
}

#[allow(dead_code)]
fn early_serial_write_hex8(val: u32) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    early_serial_write(HEX[((val >> 4) & 0xF) as usize]);
    early_serial_write(HEX[(val & 0xF) as usize]);
}

/// Detect CPU vendor for feature-adaptive behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CpuVendor { Intel, Amd, Other }

fn detect_cpu_vendor() -> CpuVendor {
    let cpuid = core::arch::x86_64::__cpuid(0);
    let vendor = [cpuid.ebx, cpuid.edx, cpuid.ecx];
    const INTEL: [u32; 3] = [0x756E6547, 0x49656E65, 0x6C65746E]; // "GenuineIntel"
    const AMD: [u32; 3] = [0x68747541, 0x69746E65, 0x444D4163];   // "AuthenticAMD"
    match vendor {
        INTEL => CpuVendor::Intel,
        AMD => CpuVendor::Amd,
        _ => CpuVendor::Other,
    }
}

/// Detect total physical memory using CPUID.
/// Returns the total memory in bytes.
///
/// Both Intel and AMD support CPUID leaf 0x80000008 for max physical
/// address width. If the leaf is unsupported, falls back to 128 MiB.
fn detect_total_memory() -> u64 {
    let max_leaf = core::arch::x86_64::__cpuid(0x80000000);
    if max_leaf.eax >= 0x80000008 {
        let result = core::arch::x86_64::__cpuid(0x80000008);
        let physical_bits = (result.eax & 0xFF) as u8;
        if physical_bits > 0 && physical_bits < 64 {
            return 1u64.checked_shl(physical_bits as u32).unwrap_or(u64::MAX);
        }
    }
    128 * 1024 * 1024
}

// -- Kernel entry point ------------------------------------------------------

#[no_mangle]
#[link_section = ".text.boot"]
pub extern "C" fn arcadia_kernel_main(hvm_info_paddr: u64, _rsi: u64) -> ! {
    // Phase 1: Early serial init (raw I/O, before Rust modules)
    unsafe {
        early_outb(0x3FB, 0x80);
        early_outb(0x3F8, 0x03);
        early_outb(0x3F9, 0x00);
        early_outb(0x3FB, 0x03);
        early_outb(0x3FC, 0x00);
        early_outb(0x3F9, 0x00);
    }

    early_serial_print("\r\n");
    early_serial_print("========================================\r\n");
    early_serial_print("  ");
    early_serial_print(crate::version::BANNER_VERSION);
    early_serial_print("\r\n");
    early_serial_print("  Kernel: ");
    early_serial_print(crate::version::KERNEL_NAME);
    early_serial_print("\r\n");
    early_serial_print("  Arch:  ");
    early_serial_print(crate::version::ARCH);
    early_serial_print(" \r\n");
    early_serial_print("========================================\r\n");
    early_serial_print("\r\n");

    // Clear VGA buffer
    unsafe {
        let vga = 0xB8000 as *mut u8;
        for i in 0..(80 * 25 * 2) {
            core::ptr::write_volatile(vga.add(i), if i % 2 == 0 { b' ' } else { 0x07 });
        }
    }

    // Display header on VGA
    unsafe {
        let vga = 0xB8000 as *mut u8;
        let header = crate::version::BANNER_VERSION;
        for (i, &ch) in header.as_bytes().iter().enumerate() {
            *vga.add(i * 2) = ch;
            *vga.add(i * 2 + 1) = 0x0B; // light cyan
        }
    }

    // Phase 2: Memory management (must be before GDT/IDT which use lazy_static/heap)
    boot_progress(true, true, "Memory", 15);
    let total_memory: u64 = detect_total_memory();

    // Heap must be initialized before HVM parsing (Vec allocation).
    crate::memory::heap::init_heap();

    // Parse PVH HVM start info memory map if available.
    let hvm_regions = if hvm_info_paddr != 0 {
        early_serial_print("[BOOT] HVM start info at 0x");
        early_serial_write_hex(hvm_info_paddr);
        early_serial_print("\r\n");
        unsafe { crate::memory::physmem::parse_hvm_start_info(hvm_info_paddr) }
    } else {
        early_serial_print("[BOOT] No HVM start info - using CPUID fallback\r\n");
        alloc::vec::Vec::new()
    };

    if !hvm_regions.is_empty() {
        early_serial_print("[BOOT] Memory map: ");
        early_serial_write_dec(hvm_regions.len());
        early_serial_print(" region(s)\r\n");
        for r in &hvm_regions {
            early_serial_print("  0x");
            early_serial_write_hex(r.base.as_u64());
            early_serial_print(" - 0x");
            early_serial_write_hex(r.base.as_u64() + r.size);
            early_serial_print("  ");
            early_serial_print(r.region_type.as_str());
            early_serial_print(" (");
            early_serial_write_dec((r.size / (1024 * 1024)) as usize);
            early_serial_print(" MiB)\r\n");
        }
        crate::memory::init_with_memmap(total_memory, &hvm_regions);
    } else {
        crate::memory::init(total_memory);
    }
    boot_progress(true, true, "Memory", 25);

    // Phase 2.5: System information detection
    boot_progress(true, true, "System Info", 28);
    crate::sysinfo::init(total_memory);
    boot_progress(true, true, "System Info", 30);

    // Phase 3: GDT/TSS
    crate::arch::gdt::init();
    boot_progress(true, true, "CPU", 40);

    // Phase 4: IDT + Interrupts
    crate::arch::idt::init();
    boot_progress(true, true, "Interrupts", 55);
    crate::interrupts::init();
    boot_progress(true, true, "Interrupts", 65);

    // Phase 5: PCI bus scan
    let pci_devices = crate::drivers::pci::scan_pci_bus();
    let pci_count = pci_devices.len();
    boot_progress(true, true, "PCI", 75);

    // Report PCI devices to serial
    if pci_count > 0 {
        early_serial_print(&alloc::format!(
            "[BOOT] Found {} PCI device(s)\r\n",
            pci_count
        ));
        for dev in pci_devices.iter() {
            early_serial_print(&alloc::format!(
                "  PCI {:02X}:{:02X}.{} - Vendor: 0x{:04X} Device: 0x{:04X} Class: {:02X}:{:02X}\r\n",
                dev.bus, dev.device, dev.function,
                dev.vendor_id, dev.device_id, dev.class, dev.subclass
            ));
        }
    } else {
        early_serial_print("[BOOT] No PCI devices found\r\n");
    }

    // Phase 6: Storage detection (ATA PIO)
    boot_progress(true, true, "Storage", 85);
    match crate::drivers::ata::init_ata_driver() {
        Ok(()) => {
            early_serial_print("[BOOT] ATA driver initialized.\r\n");
        }
        Err(e) => {
            early_serial_print("[BOOT] ATA: ");
            early_serial_print(alloc::format!("{}", e).as_str());
            early_serial_print(" (no disk)\r\n");
        }
    }

    // Phase 6.5: VFS initialization
    crate::fs::vfs::init_vfs();

    // Phase 7: Launch shell
    boot_progress(true, true, "ArcShell", 95);

    crate::vga_buffer::clear_screen();

    early_serial_print("[BOOT] Arcadia OS ready.\r\n");
    early_serial_print("\r\n");

    let mut terminal = crate::terminal::Terminal::new();
    terminal.run()
}
