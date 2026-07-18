use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};
use spin::Mutex;

const SCANCODE_BUFFER_SIZE: usize = 256;
const MAX_INPUT_LEN: usize = 256;
const MAX_HISTORY: usize = 50;

static SCANCODE_BUFFER: Mutex<[u8; SCANCODE_BUFFER_SIZE]> = Mutex::new([0u8; SCANCODE_BUFFER_SIZE]);
static SCANCODE_HEAD: AtomicUsize = AtomicUsize::new(0);
static SCANCODE_TAIL: AtomicUsize = AtomicUsize::new(0);

pub fn push_scancode(scancode: u8) {
    let tail = SCANCODE_TAIL.load(Ordering::Relaxed);
    let next = (tail + 1) % SCANCODE_BUFFER_SIZE;
    if next == SCANCODE_HEAD.load(Ordering::Acquire) {
        return;
    }
    let mut buffer = SCANCODE_BUFFER.lock();
    buffer[tail] = scancode;
    SCANCODE_TAIL.store(next, Ordering::Release);
}

pub fn pop_scancode() -> Option<u8> {
    let head = SCANCODE_HEAD.load(Ordering::Relaxed);
    if head == SCANCODE_TAIL.load(Ordering::Acquire) {
        return None;
    }
    let scancode = x86_64::instructions::interrupts::without_interrupts(|| {
        let buffer = SCANCODE_BUFFER.lock();
        buffer[head]
    });
    let next = (head + 1) % SCANCODE_BUFFER_SIZE;
    SCANCODE_HEAD.store(next, Ordering::Release);
    Some(scancode)
}

pub struct Terminal {
    input_buffer: String,
    history: Vec<String>,
    history_index: Option<usize>,
    prompt: &'static str,
}

impl Terminal {
    pub fn new() -> Self {
        Terminal {
            input_buffer: String::new(),
            history: Vec::new(),
            history_index: None,
            prompt: "arcadia> ",
        }
    }

    pub fn run(&mut self) -> ! {
        crate::serial_println!();
        crate::serial_println!("+======================================+");
        crate::serial_println!("|     {}    |", crate::version::BANNER_VERSION);
        crate::serial_println!("|     Terminal Ready                    |");
        crate::serial_println!("+======================================+");
        crate::serial_println!();
        crate::serial_println!("Type 'help' for available commands.");
        crate::serial_println!();

        loop {
            crate::serial_print!("{}", self.prompt);
            self.read_line();
            self.process_command();
        }
    }

    fn read_line(&mut self) {
        use pc_keyboard::{layouts, DecodedKey, HandleControl, Keyboard, ScancodeSet1};

        let mut kbd = Keyboard::new(
            ScancodeSet1::new(),
            layouts::Us104Key,
            HandleControl::MapLettersToUnicode,
        );

        self.input_buffer.clear();

        loop {
            if let Some(scancode) = pop_scancode() {
                if let Ok(Some(key_event)) = kbd.add_byte(scancode) {
                    if let Some(key) = kbd.process_keyevent(key_event) {
                        match key {
                            DecodedKey::Unicode(c) => match c {
                                '\n' => {
                                    crate::serial_println!();
                                    if !self.input_buffer.is_empty() {
                                        self.history.push(self.input_buffer.clone());
                                        if self.history.len() > MAX_HISTORY {
                                            self.history.remove(0);
                                        }
                                        self.history_index = None;
                                    }
                                    return;
                                }
                                '\x08' => {
                                    if self.input_buffer.pop().is_some() {
                                        crate::serial_print!("\x08 \x08");
                                    }
                                }
                                c if !c.is_control() => {
                                    if self.input_buffer.len() < MAX_INPUT_LEN {
                                        self.input_buffer.push(c);
                                        crate::serial_print!("{}", c);
                                    }
                                }
                                _ => {}
                            },
                            DecodedKey::RawKey(key) => match key {
                                pc_keyboard::KeyCode::ArrowUp => {
                                    if self.history_index.map_or(false, |i| i > 0) {
                                        self.history_index = Some(self.history_index.unwrap() - 1);
                                    } else if self.history_index.is_none()
                                        && !self.history.is_empty()
                                    {
                                        self.history_index = Some(self.history.len() - 1);
                                    }
                                }
                                pc_keyboard::KeyCode::ArrowDown => {
                                    if let Some(i) = self.history_index {
                                        if i + 1 < self.history.len() {
                                            self.history_index = Some(i + 1);
                                        } else {
                                            self.history_index = None;
                                        }
                                    }
                                }
                                _ => {}
                            },
                        }
                    }
                }
            } else if let Some(c) = Self::serial_read_byte() {
                match c {
                    b'\r' | b'\n' => {
                        crate::serial_println!();
                        if !self.input_buffer.is_empty() {
                            self.history.push(self.input_buffer.clone());
                            if self.history.len() > MAX_HISTORY {
                                self.history.remove(0);
                            }
                            self.history_index = None;
                        }
                        return;
                    }
                    0x08 | 0x7F => {
                        if self.input_buffer.pop().is_some() {
                            crate::serial_print!("\x08 \x08");
                        }
                    }
                    b if b >= 0x20 && b < 0x7F => {
                        if self.input_buffer.len() < MAX_INPUT_LEN {
                            self.input_buffer.push(b as char);
                            crate::serial_print!("{}", b as char);
                        }
                    }
                    _ => {}
                }
            } else {
                for _ in 0..1000u32 {
                    core::hint::spin_loop();
                }
            }
        }
    }

    fn serial_read_byte() -> Option<u8> {
        unsafe {
            let lsr: u8;
            core::arch::asm!("in al, dx", in("dx") 0x3FDu16, out("al") lsr);
            if lsr & 1 != 0 {
                let data: u8;
                core::arch::asm!("in al, dx", in("dx") 0x3F8u16, out("al") data);
                Some(data)
            } else {
                None
            }
        }
    }

    fn process_command(&mut self) {
        let input = self.input_buffer.trim().to_string();
        if input.is_empty() {
            return;
        }

        let parts: Vec<&str> = input.split_whitespace().collect();
        let cmd = parts[0];
        let args = &parts[1..];

        match cmd {
            "help" => self.cmd_help(),
            "clear" => {
                crate::vga_buffer::clear_screen();
                crate::serial_println!("Screen cleared.");
            }
            "version" => self.cmd_version(),
            "mem" => self.cmd_memory(),
            "vmm" => self.cmd_vmm(),
            "reboot" => {
                crate::serial_println!("Rebooting...");
                unsafe {
                    x86_64::instructions::port::Port::<u8>::new(0x64).write(0xFE);
                }
            }
            "halt" => {
                crate::serial_println!("System halted.");
                loop {
                    x86_64::instructions::hlt();
                }
            }
            "echo" => crate::serial_println!("{}", args.join(" ")),
            "hostname" => crate::serial_println!("arcadia"),
            "whoami" => crate::serial_println!("root"),
            "uname" => crate::serial_println!(
                "{} {} {}",
                crate::version::OS_NAME,
                crate::version::VERSION,
                crate::version::ARCH
            ),
            "cpuinfo" => self.cmd_cpuinfo(),
            "meminfo" => self.cmd_meminfo(),
            "sysinfo" => self.cmd_sysinfo(),
            "neofetch" => self.cmd_neofetch(),
            "uptime" => self.cmd_uptime(),
            "ticks" => self.cmd_ticks(),
            "disk" => self.cmd_disk(args),
            "partitions" | "parts" => self.cmd_partitions(),
            "ls" => self.cmd_ls(args),
            "cat" => self.cmd_cat(args),
            "touch" => self.cmd_touch(args),
            "mkdir" => self.cmd_mkdir(args),
            "write" => self.cmd_write(args),
            "rm" => self.cmd_rm(args),
            "mount" => self.cmd_mount(),
            "cd" => self.cmd_cd(args),
            "pwd" => self.cmd_pwd(),
            "run" => self.cmd_run(),
            "ps" => self.cmd_ps(),
            "exit" => {
                crate::serial_println!("Shutting down...");
                loop {
                    x86_64::instructions::hlt();
                }
            }
            _ => {
                crate::serial_println!("Unknown command: '{}'. Type 'help' for commands.", cmd);
            }
        }
    }

    fn cmd_help(&self) {
        crate::serial_println!("Available commands:");
        crate::serial_println!("  help          - Show this help message");
        crate::serial_println!("  clear         - Clear the screen");
        crate::serial_println!("  version       - Show system version");
        crate::serial_println!("  uname         - Show system information");
        crate::serial_println!("  mem           - Show physical memory information");
        crate::serial_println!("  vmm           - Show virtual memory manager statistics");
        crate::serial_println!("  cpuinfo       - Show CPU information");
        crate::serial_println!("  meminfo       - Show memory information");
        crate::serial_println!("  sysinfo       - Show complete system information");
        crate::serial_println!("  echo [text]   - Print text");
        crate::serial_println!("  hostname      - Show hostname");
        crate::serial_println!("  whoami        - Show current user");
        crate::serial_println!("  neofetch      - Show system info");
        crate::serial_println!("  uptime        - Show system uptime");
        crate::serial_println!("  ticks         - Show raw tick count");
        crate::serial_println!("  disk [r|w]    - Disk info, read/write sectors");
        crate::serial_println!("  partitions    - Show MBR partition table");
        crate::serial_println!("  ls [path]     - List directory contents");
        crate::serial_println!("  cat <file>    - Read file contents");
        crate::serial_println!("  touch <file>  - Create empty file");
        crate::serial_println!("  mkdir <dir>   - Create directory");
        crate::serial_println!("  write <f> <s> - Write string to file");
        crate::serial_println!("  rm <file>     - Delete file");
        crate::serial_println!("  mount         - Show filesystem info");
        crate::serial_println!("  cd <path>     - Change directory");
        crate::serial_println!("  pwd           - Print working directory");
        crate::serial_println!("  run           - Run init process in Ring 3");
        crate::serial_println!("  ps            - List processes");
        crate::serial_println!("  reboot        - Reboot the system");
        crate::serial_println!("  halt          - Halt the system");
        crate::serial_println!("  exit          - Shut down");
    }

    fn cmd_version(&self) {
        crate::serial_println!(
            "{} ({})",
            crate::version::OS_NAME,
            crate::version::BANNER_VERSION
        );
        crate::serial_println!(
            "Kernel: {} {}",
            crate::version::KERNEL_NAME,
            crate::version::VERSION
        );
        crate::serial_println!("Architecture: {}", crate::version::ARCH);
    }

    fn cmd_memory(&self) {
        if crate::memory::physmem::is_initialized() {
            let stats = crate::memory::physmem::statistics();
            crate::serial_println!(
                "[MEMORY] Total: {} MiB ({} bytes)",
                stats.total_bytes / (1024 * 1024),
                stats.total_bytes
            );
            crate::serial_println!(
                "[MEMORY] Usable: {} MiB",
                stats.usable_bytes / (1024 * 1024)
            );
            crate::serial_println!(
                "[MEMORY] Reserved: {} MiB",
                stats.reserved_bytes / (1024 * 1024)
            );
            crate::serial_println!("[MEMORY] Allocated: {} frames", stats.allocated_frames);
            crate::serial_println!("[MEMORY] Free: {} frames", stats.free_frames);
            crate::serial_println!("[MEMORY] Total frames: {}", stats.total_frames);
        } else {
            crate::serial_println!("[MEMORY] Physical Memory Manager not initialized.");
        }
    }

    fn cmd_vmm(&self) {
        if crate::memory::vmm::is_initialized() {
            let stats = crate::memory::vmm::statistics();
            let valid = crate::memory::vmm::validate();
            crate::serial_println!("[VMM] Virtual Memory Manager Statistics:");
            crate::serial_println!("[VMM] Mapped pages: {} ({} bytes)",
                stats.total_mapped_pages, stats.total_mapped_bytes);
            crate::serial_println!("[VMM] Map operations: {}", stats.map_count);
            crate::serial_println!("[VMM] Unmap operations: {}", stats.unmap_count);
            crate::serial_println!("[VMM] Remap operations: {}", stats.remap_count);
            crate::serial_println!("[VMM] TLB flushes: {}", stats.tlb_flush_count);
            crate::serial_println!("[VMM] Page table frames: {}", stats.page_table_frames);
            crate::serial_println!("[VMM] Validation: {}",
                if valid { "PASS" } else { "FAIL" });
            crate::serial_println!("[VMM] Mapped regions:");
            for region in crate::memory::vmm::vmm().mapped_regions() {
                crate::serial_println!(
                    "  0x{:012X} -> 0x{:012X}  {} KiB  {}",
                    region.virtual_addr.as_u64(),
                    region.physical_addr.as_u64(),
                    region.size / 1024,
                    region.description
                );
            }
        } else {
            crate::serial_println!("[VMM] Virtual Memory Manager not initialized.");
        }
    }

    fn cmd_cpuinfo(&self) {
        if crate::sysinfo::cpu::is_initialized() {
            let cpu = crate::sysinfo::cpu::cpu_info();
            crate::serial_println!("CPU Information:");
            crate::serial_println!("  Vendor: {}", cpu.vendor.as_str());
            crate::serial_println!("  Brand: {}", cpu.brand_string.as_str());
            crate::serial_println!(
                "  Family: {} Model: {} Stepping: {}",
                cpu.signature.full_family(),
                cpu.signature.full_model(),
                cpu.signature.stepping
            );
            crate::serial_println!(
                "  Cores: {} physical, {} logical",
                cpu.physical_cores,
                cpu.logical_cores
            );
            crate::serial_println!("  Frequency: {} MHz", cpu.frequency_mhz);
            crate::serial_println!(
                "  Cache: L1={} KB, L2={} KB, L3={} KB",
                cpu.cache_l1,
                cpu.cache_l2,
                cpu.cache_l3
            );
            crate::serial_println!("  Features: {}", cpu.feature_count());
        } else {
            crate::serial_println!(
                "CPU information not available. System detection not initialized."
            );
        }
    }

    fn cmd_meminfo(&self) {
        if crate::sysinfo::memory::is_initialized() {
            let mem = crate::sysinfo::memory::memory_info();
            crate::serial_println!("Memory Information:");
            crate::serial_println!("  Total: {} MiB", mem.total_mib());
            crate::serial_println!("  Usable: {} MiB", mem.usable_mib());
            crate::serial_println!("  Reserved: {} MiB", mem.reserved_mib());
            crate::serial_println!("  Memory Regions: {}", mem.memory_map.len());
        } else {
            crate::serial_println!(
                "Memory information not available. System detection not initialized."
            );
        }
    }

    fn cmd_sysinfo(&self) {
        if crate::sysinfo::is_initialized() {
            let cpu = crate::sysinfo::cpu::cpu_info();
            let mem = crate::sysinfo::memory::memory_info();
            crate::serial_println!("System Information:");
            crate::serial_println!("  ==== CPU ====");
            crate::serial_println!("  Vendor: {}", cpu.vendor.as_str());
            crate::serial_println!("  Brand: {}", cpu.brand_string.as_str());
            crate::serial_println!(
                "  Family: {} Model: {} Stepping: {}",
                cpu.signature.full_family(),
                cpu.signature.full_model(),
                cpu.signature.stepping
            );
            crate::serial_println!(
                "  Cores: {} physical, {} logical",
                cpu.physical_cores,
                cpu.logical_cores
            );
            crate::serial_println!("  ==== Memory ====");
            crate::serial_println!("  Total: {} MiB", mem.total_mib());
            crate::serial_println!("  Usable: {} MiB", mem.usable_mib());
            crate::serial_println!("  Reserved: {} MiB", mem.reserved_mib());
        } else {
            crate::serial_println!(
                "System information not available. System detection not initialized."
            );
        }
    }

    fn cmd_neofetch(&self) {
        crate::serial_println!("       _              ");
        crate::serial_println!("      / \\   Arcadia   ");
        crate::serial_println!("     /   \\  --------  ");
        crate::serial_println!("    / Arc \\  OS v0.2  ");
        crate::serial_println!("   /       \\  x86_64  ");
        crate::serial_println!("  /_________\\          ");
        crate::serial_println!();
        crate::serial_println!(
            "  OS:     Arcadia {} ({})",
            crate::version::VERSION,
            crate::version::STAGE
        );
        crate::serial_println!("  Kernel: arcadia-kernel");
        crate::serial_println!("  Shell:  arcadia-sh");
        if crate::sysinfo::cpu::is_initialized() {
            let cpu = crate::sysinfo::cpu::cpu_info();
            crate::serial_println!(
                "  CPU:    {} ({} cores)",
                cpu.vendor.as_str(),
                cpu.logical_cores
            );
        } else {
            crate::serial_println!("  CPU:    x86_64");
        }
        if crate::sysinfo::memory::is_initialized() {
            let mem = crate::sysinfo::memory::memory_info();
            crate::serial_println!("  Memory: {} MiB", mem.total_mib());
        } else {
            crate::serial_println!("  Memory: 128 KiB heap");
        }
    }

    fn cmd_uptime(&self) {
        let ms = crate::time::uptime_ms();
        let secs = ms / 1000;
        let mins = secs / 60;
        let hours = mins / 60;
        let days = hours / 24;

        crate::serial_println!(
            "Uptime: {}d {:02}:{:02}:{:02}.{:03} ({} ms, {} ticks @ {} Hz)",
            days,
            hours % 24,
            mins % 60,
            secs % 60,
            ms % 1000,
            ms,
            crate::time::ticks(),
            crate::time::tick_rate()
        );
    }

    fn cmd_ticks(&self) {
        crate::serial_println!(
            "Ticks: {} ({} Hz, {} ms elapsed)",
            crate::time::ticks(),
            crate::time::tick_rate(),
            crate::time::uptime_ms()
        );
    }

    fn cmd_disk(&self, args: &[&str]) {
        use crate::drivers::ata::{ata, SECTOR_SIZE};
        use crate::drivers::ata::PRIMARY_BASE;

        let mut ata_lock = ata();
        let driver = match ata_lock.as_mut() {
            Some(d) => d,
            None => {
                crate::serial_println!("No ATA driver initialized.");
                return;
            }
        };

        if args.is_empty() {
            let info = driver.device_info(PRIMARY_BASE, false);
            if info.present && info.is_ata {
                crate::serial_println!("ATA Primary Master:");
                crate::serial_println!("  Model:    {}", core::str::from_utf8(&info.model).unwrap_or("Unknown"));
                crate::serial_println!("  Serial:   {}", core::str::from_utf8(&info.serial).unwrap_or("Unknown"));
                crate::serial_println!("  Firmware: {}", core::str::from_utf8(&info.firmware).unwrap_or("Unknown"));
                crate::serial_println!("  Sectors:  {} ({} MiB)", info.sectors_28, info.sectors_28 / 2048);
                crate::serial_println!("  LBA48:    {}", if info.lba48 { "Yes" } else { "No" });
            } else {
                crate::serial_println!("No ATA device on primary master.");
            }
            return;
        }

        match args[0] {
            "r" => {
                if args.len() < 2 {
                    crate::serial_println!("Usage: disk r <lba> [count]");
                    return;
                }
                let lba = match u32::from_str_radix(args[1], 10) {
                    Ok(v) => v,
                    Err(_) => {
                        crate::serial_println!("Invalid LBA.");
                        return;
                    }
                };
                let count = if args.len() >= 3 {
                    match u32::from_str_radix(args[2], 10) {
                        Ok(v) => v.min(4),
                        Err(_) => 1,
                    }
                } else {
                    1
                };

                let mut buf = alloc::vec![0u8; count as usize * SECTOR_SIZE];
                drop(ata_lock);

                let mut ata_lock = ata();
                let driver = ata_lock.as_mut().unwrap();
                match driver.read_sectors(PRIMARY_BASE, false, lba, count, &mut buf) {
                    Ok(()) => {
                        crate::serial_println!("Read {} sector(s) from LBA {}:", count, lba);
                        for sec in 0..count {
                            let off = sec as usize * SECTOR_SIZE;
                            let end = (off + 64).min(buf.len());
                            crate::serial_print!("  LBA {}: ", lba + sec);
                            for i in off..end {
                                crate::serial_print!("{:02X} ", buf[i]);
                            }
                            crate::serial_println!();
                        }
                    }
                    Err(e) => crate::serial_println!("Read error: {}", e),
                }
            }
            "w" => {
                if args.len() < 3 {
                    crate::serial_println!("Usage: disk w <lba> <hex-byte> [count]");
                    return;
                }
                let lba = match u32::from_str_radix(args[1], 10) {
                    Ok(v) => v,
                    Err(_) => {
                        crate::serial_println!("Invalid LBA.");
                        return;
                    }
                };
                let byte_val = match u8::from_str_radix(args[2], 16) {
                    Ok(v) => v,
                    Err(_) => {
                        crate::serial_println!("Invalid hex byte.");
                        return;
                    }
                };
                let count = if args.len() >= 4 {
                    match u32::from_str_radix(args[3], 10) {
                        Ok(v) => v.min(4),
                        Err(_) => 1,
                    }
                } else {
                    1
                };

                let mut buf = alloc::vec![byte_val; count as usize * SECTOR_SIZE];
                drop(ata_lock);

                let mut ata_lock = ata();
                let driver = ata_lock.as_mut().unwrap();
                match driver.write_sectors(PRIMARY_BASE, false, lba, count, &mut buf) {
                    Ok(()) => crate::serial_println!("Wrote {} sector(s) to LBA {}.", count, lba),
                    Err(e) => crate::serial_println!("Write error: {}", e),
                }
            }
            _ => {
                crate::serial_println!("Usage: disk [r <lba> [count] | w <lba> <byte> [count]]");
            }
        }
    }

    fn cmd_partitions(&self) {
        use crate::fs::mbr::read_mbr;

        let mbr = match read_mbr() {
            Ok(m) => m,
            Err(e) => {
                crate::serial_println!("Failed to read MBR: {}", e);
                return;
            }
        };

        if !mbr.valid {
            crate::serial_println!("Invalid MBR (missing 0xAA55 boot signature).");
            return;
        }

        crate::serial_println!("MBR Partition Table:");
        crate::serial_println!("  Boot Sig: 0x{:04X}", mbr.boot_sig);
        crate::serial_println!();
        crate::serial_println!("  #   Type             Status     LBA Start   LBA End       Sectors       Size");
        crate::serial_println!("  --- ---------------- ---------- ----------- ------------- ------------- ----------");

        for (i, part) in mbr.partitions.iter().enumerate() {
            let status = if part.is_active() {
                "Active"
            } else if part.is_empty() {
                "---"
            } else {
                "     "
            };

            let type_name = part.partition_type.name();
            let lba_end = part.lba_last();

            crate::serial_println!(
                "  {}   {:<16} {:<10} {:>11} {:>13} {:>13} {}",
                i + 1,
                type_name,
                status,
                part.lba_first,
                lba_end,
                part.sector_count,
                part.size_human()
            );
        }

        let count = mbr.partition_count();
        crate::serial_println!();
        if count == 0 {
            crate::serial_println!("No partitions found.");
        } else {
            crate::serial_println!("{} partition(s) found.", count);
        }
    }

    fn cmd_mount(&self) {
        let vfs_lock = crate::fs::vfs::vfs();
        let vfs = vfs_lock.as_ref().expect("VFS not initialized");
        if !vfs.is_mounted() {
            crate::serial_println!("No filesystem mounted.");
            return;
        }
        crate::serial_println!("Mounted: FAT32 on /");
        crate::serial_println!("  Root cluster: {}", vfs.root_cluster);
        if let Some(fs) = &vfs.fat_fs {
            crate::serial_println!("  Volume label: {}", fs.bpb.volume_label_str());
            crate::serial_println!(
                "  Cluster size: {} bytes",
                fs.bpb.cluster_size_bytes()
            );
            let total_sectors = fs.bpb.total_sectors_32;
            crate::serial_println!(
                "  Total sectors: {} ({} MiB)",
                total_sectors,
                total_sectors / 2048
            );
        }
    }

    fn cmd_ls(&self, args: &[&str]) {
        let vfs_lock = crate::fs::vfs::vfs();
        let vfs = vfs_lock.as_ref().expect("VFS not initialized");
        let path = if args.is_empty() { vfs.cwd.as_str() } else { args[0] };

        match vfs.list_dir(path) {
            Ok(entries) => {
                if entries.is_empty() {
                    crate::serial_println!("(empty)");
                    return;
                }
                crate::serial_println!("Directory: {}", path);
                crate::serial_println!("  {:<20} {:>8}  {}", "Name", "Size", "Type");
                crate::serial_println!("  {:<20} {:>8}  {}", "----", "----", "----");
                for entry in &entries {
                    let kind = if entry.is_dir { "dir" } else { "file" };
                    crate::serial_println!(
                        "  {:<20} {:>8}  {}",
                        entry.path,
                        entry.size,
                        kind
                    );
                }
                crate::serial_println!("{} item(s)", entries.len());
            }
            Err(e) => crate::serial_println!("ls error: {}", e),
        }
    }

    fn cmd_cat(&self, args: &[&str]) {
        if args.is_empty() {
            crate::serial_println!("Usage: cat <file>");
            return;
        }
        let vfs_lock = crate::fs::vfs::vfs();
        let vfs = vfs_lock.as_ref().expect("VFS not initialized");
        match vfs.read_file_content(args[0]) {
            Ok(data) => {
                for &byte in &data {
                    if byte == b'\r' {
                        continue;
                    }
                    crate::serial_print!("{}", byte as char);
                }
                if data.last() != Some(&b'\n') {
                    crate::serial_println!();
                }
            }
            Err(e) => crate::serial_println!("cat error: {}", e),
        }
    }

    fn cmd_touch(&self, args: &[&str]) {
        if args.is_empty() {
            crate::serial_println!("Usage: touch <file>");
            return;
        }
        let mut vfs_lock = crate::fs::vfs::vfs();
        let vfs = vfs_lock.as_mut().expect("VFS not initialized");
        match vfs.stat(args[0]) {
            Ok(_) => crate::serial_println!("Exists: {}", args[0]),
            Err(crate::fs::vfs::VfsError::NotFound) => {
                match vfs.create_file(args[0], b"") {
                    Ok(()) => crate::serial_println!("Created: {}", args[0]),
                    Err(e) => crate::serial_println!("touch error: {}", e),
                }
            }
            Err(e) => crate::serial_println!("touch error: {}", e),
        }
    }

    fn cmd_mkdir(&self, args: &[&str]) {
        if args.is_empty() {
            crate::serial_println!("Usage: mkdir <directory>");
            return;
        }
        let mut vfs_lock = crate::fs::vfs::vfs();
        let vfs = vfs_lock.as_mut().expect("VFS not initialized");
        match vfs.mkdir(args[0]) {
            Ok(()) => crate::serial_println!("Created directory: {}", args[0]),
            Err(e) => crate::serial_println!("mkdir error: {}", e),
        }
    }

    fn cmd_write(&self, args: &[&str]) {
        if args.len() < 2 {
            crate::serial_println!("Usage: write <file> <data>");
            return;
        }
        let path = args[0];
        let data = args[1..].join(" ");
        let mut vfs_lock = crate::fs::vfs::vfs();
        let vfs = vfs_lock.as_mut().expect("VFS not initialized");
        match vfs.create_file(path, data.as_bytes()) {
            Ok(()) => crate::serial_println!("Wrote {} bytes to {}", data.len(), path),
            Err(e) => crate::serial_println!("write error: {}", e),
        }
    }

    fn cmd_rm(&self, args: &[&str]) {
        if args.is_empty() {
            crate::serial_println!("Usage: rm <file>");
            return;
        }
        {
            let vfs_lock = crate::fs::vfs::vfs();
            let vfs = vfs_lock.as_ref().expect("VFS not initialized");
            match vfs.stat(args[0]) {
                Ok(node) => {
                    if node.is_dir {
                        crate::serial_println!("rm error: cannot remove directory (use rmdir)");
                        return;
                    }
                }
                Err(_) => {
                    crate::serial_println!("rm error: {}", crate::fs::vfs::VfsError::NotFound);
                    return;
                }
            }
        }
        let mut vfs_lock = crate::fs::vfs::vfs();
        let vfs = vfs_lock.as_mut().expect("VFS not initialized");
        match vfs.delete(args[0]) {
            Ok(()) => crate::serial_println!("Deleted: {}", args[0]),
            Err(e) => crate::serial_println!("rm error: {}", e),
        }
    }

    fn cmd_cd(&self, args: &[&str]) {
        let mut vfs_lock = crate::fs::vfs::vfs();
        let vfs = vfs_lock.as_mut().expect("VFS not initialized");
        if args.is_empty() {
            crate::serial_println!("{}", vfs.cwd);
            return;
        }
        let target = args[0];
        let new_path = if target.starts_with('/') {
            target.to_string()
        } else if target == ".." {
            let cwd = vfs.cwd.trim_end_matches('/');
            if let Some(pos) = cwd.rfind('/') {
                if pos == 0 {
                    "/".to_string()
                } else {
                    cwd[..pos].to_string()
                }
            } else {
                "/".to_string()
            }
        } else {
            let mut new = vfs.cwd.clone();
            if !new.ends_with('/') {
                new.push('/');
            }
            new.push_str(target);
            new
        };

        match vfs.resolve_path(&new_path) {
            Ok(_) => {
                vfs.cwd = new_path.clone();
                crate::serial_println!("{}", new_path);
            }
            Err(e) => crate::serial_println!("cd error: {}", e),
        }
    }

    fn cmd_pwd(&self) {
        let vfs_lock = crate::fs::vfs::vfs();
        let vfs = vfs_lock.as_ref().expect("VFS not initialized");
        crate::serial_println!("{}", vfs.cwd);
    }

    fn cmd_run(&mut self) {
        // Clear exit flag before launching (set by previous process exit).
        unsafe { crate::process::EXIT_REQUESTED = false; }

        crate::serial_println!("Preparing to launch init process...");

        // Save kernel state before entering user mode.
        // On process exit, execution resumes here. Check if we're returning
        // from exit and bail out to prevent double-launch.
        crate::process::save_kernel_state();
        if unsafe { crate::process::EXIT_REQUESTED } {
            return;
        }

        // Use embedded init binary
        let elf_data = crate::process::init::INIT_ELF;
        crate::serial_println!("Loaded init binary ({} bytes)", elf_data.len());

        // Launch the process
        match crate::process::launch_init_process(elf_data) {
            Ok(()) => {
                crate::serial_println!("Init process created. Entering Ring 3...");
                // This never returns (process runs and exits back to shell)
                crate::process::enter_user_mode();
            }
            Err(e) => {
                crate::serial_println!("Failed to launch process: {}", e);
            }
        }
    }

    fn cmd_ps(&self) {
        let table = crate::process::PROCESS_TABLE.lock();
        crate::serial_println!("PID  STATE      ELF_ENTRY       EXIT_CODE");
        crate::serial_println!("---  ---------  --------------- ----------");
        for proc in table.iter() {
            if proc.state != crate::process::pcb::ProcessState::Unused {
                let state = match proc.state {
                    crate::process::pcb::ProcessState::Unused => "unused",
                    crate::process::pcb::ProcessState::Ready => "ready",
                    crate::process::pcb::ProcessState::Running => "running",
                    crate::process::pcb::ProcessState::Blocked => "blocked",
                    crate::process::pcb::ProcessState::Sleeping => "sleeping",
                    crate::process::pcb::ProcessState::Exited => "exited ",
                };
                crate::serial_println!(
                    "{:3}  {:9}  0x{:014X}  {}",
                    proc.pid,
                    state,
                    proc.elf_entry,
                    proc.exit_code
                );
            }
        }
        drop(table);
        crate::serial_println!("Processes active.");
    }
}
