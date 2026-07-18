pub mod context;
pub mod init;
pub mod pcb;
pub mod syscall;

use spin::Mutex;
use x86_64::PhysAddr;
use x86_64::registers::control::Cr3Flags;
use x86_64::structures::paging::PageTableFlags;
use crate::memory::physmem;
use crate::memory::vmm;
use pcb::*;

pub static PROCESS_TABLE: Mutex<[ProcessControlBlock; MAX_PROCESSES]> = Mutex::new({
    const EMPTY: ProcessControlBlock = ProcessControlBlock {
        pid: 0,
        state: ProcessState::Unused,
        kernel_rsp: 0,
        user_rsp: 0,
        pml4_phys: 0,
        elf_entry: 0,
        exit_code: 0,
        kernel_stack_phys: 0,
        user_stack_phys: 0,
        sleep_until: 0,
    };
    [EMPTY; MAX_PROCESSES]
});

pub static mut CURRENT_PID: u32 = 0;
pub static mut KERNEL_PML4_PHYS: u64 = 0;
pub static mut SHELL_RSP: u64 = 0;
pub static mut EXIT_REQUESTED: bool = false;
pub static mut EXIT_CODE: i32 = 0;

/// Save the current kernel RSP so we can return to the shell after process exit.
///
/// # Safety
/// Must be called from the shell context before entering user mode.
pub fn save_kernel_state() {
    let rsp: u64;
    unsafe {
        core::arch::asm!("mov {}, rsp", out(reg) rsp);
        SHELL_RSP = rsp;
        let (pml4, _) = x86_64::registers::control::Cr3::read();
        KERNEL_PML4_PHYS = pml4.start_address().as_u64();
    }
}

fn allocate_page() -> Option<u64> {
    physmem::allocate_frame().map(|f| f.as_u64())
}

fn zero_page(phys: u64) {
    unsafe {
        core::ptr::write_bytes(phys as *mut u8, 0, 4096);
    }
}

/// Free a 4 KiB physical frame back to the PMM.
fn free_page(phys: u64) {
    use x86_64::PhysAddr;
    let _ = physmem::free_frame(PhysAddr::new(phys));
}

/// Recursively free all page table levels below PML4 for user-space entries.
/// Frees intermediate page tables but NOT leaf page frames (caller frees those).
fn free_user_page_tables(table_phys: u64, level: u8) {
    let table = unsafe { &*(table_phys as *const x86_64::structures::paging::PageTable) };
    let limit = if level == 4 { 256 } else { 512 };

    for idx in 0..limit {
        let entry = &table[idx];
        if !entry.flags().contains(x86_64::structures::paging::PageTableFlags::PRESENT) {
            continue;
        }
        if entry.flags().contains(x86_64::structures::paging::PageTableFlags::HUGE_PAGE) {
            continue;
        }

        let child_phys = entry.addr().as_u64();
        if level > 1 {
            free_user_page_tables(child_phys, level - 1);
        }
        // Don't free level-1 leaf page frames here — they are freed separately
        // because we need to track user stack, kernel stack, and ELF pages.
        if level == 1 {
            free_page(child_phys);
        }
    }
}

/// Free all user-space page table resources for a process.
///
/// Walks PML4 user entries (0-255), frees intermediate page tables
/// and leaf frames. Does NOT touch kernel upper-half entries (shared).
fn free_process_address_space(pml4_phys: u64) {
    if pml4_phys == 0 {
        return;
    }
    free_user_page_tables(pml4_phys, 4);
    free_page(pml4_phys);
}

/// Create new page tables for a user process.
///
/// Copies the kernel's upper-half PML4 entries (256-511) so kernel code/data
/// remains accessible from Ring 0 during syscalls. Does NOT copy entry 0
/// (identity mapping) to prevent user-space access to low physical memory.
fn create_user_page_tables() -> Option<u64> {
    let new_pml4 = allocate_page()?;
    zero_page(new_pml4);

    let current_pml4 = unsafe { KERNEL_PML4_PHYS };
    unsafe {
        let src = (current_pml4 + 256 * 8) as *const u64;
        let dst = (new_pml4 + 256 * 8) as *mut u64;
        core::ptr::copy_nonoverlapping(src, dst, 256);
    }

    Some(new_pml4)
}

fn map_user_page(
    pml4_phys: u64,
    virt: u64,
    phys: u64,
    flags: PageTableFlags,
) -> Result<(), &'static str> {
    use x86_64::structures::paging::{Mapper, OffsetPageTable, Page, PhysFrame, Size4KiB};
    use x86_64::VirtAddr;

    unsafe {
        let pml4_ptr = pml4_phys as *mut x86_64::structures::paging::PageTable;
        let pml4_ref = &mut *pml4_ptr;
        let static_ref: &'static mut x86_64::structures::paging::PageTable =
            core::mem::transmute(pml4_ref);
        let mut mapper = OffsetPageTable::new(static_ref, VirtAddr::new(0));

        let page = Page::<Size4KiB>::containing_address(VirtAddr::new(virt));
        let frame = PhysFrame::<Size4KiB>::containing_address(PhysAddr::new(phys));
        let mut frame_alloc = vmm::PmmFrameAllocator;

        match mapper.map_to(page, frame, flags, &mut frame_alloc) {
            Ok(flush) => {
                flush.flush();
                Ok(())
            }
            Err(_) => Err("Failed to map page"),
        }
    }
}

pub fn launch_init_process(elf_data: &[u8]) -> Result<(), &'static str> {
    let elf_info = crate::elf::parse_elf(elf_data).map_err(|e| e)?;

    let mut table = PROCESS_TABLE.lock();
    let proc = table.iter_mut().find(|p| p.state == ProcessState::Unused)
        .ok_or("No free process slot")?;

    let pml4 = create_user_page_tables().ok_or("Failed to create page tables")?;
    proc.pml4_phys = pml4;

    // Map ELF segments into user address space
    for i in 0..elf_info.segment_count {
        let seg = &elf_info.segments[i];
        let page_count = ((seg.memsz + 4095) / 4096) as usize;

        for page_idx in 0..page_count {
            let page_phys = allocate_page().ok_or("Out of memory for ELF")?;
            zero_page(page_phys);

            let virt = seg.vaddr + (page_idx as u64) * 4096;
            let data_offset = seg.data_offset + (page_idx as u64) * 4096;

            if data_offset < seg.data_offset + seg.filesz {
                let copy_start = (page_idx as u64) * 4096;
                let copy_end = core::cmp::min(copy_start + 4096, seg.filesz - (page_idx as u64) * 4096);
                if copy_start < seg.filesz && copy_end > 0 {
                    let src = &elf_data[(data_offset as usize)..((data_offset + (copy_end - copy_start)) as usize)];
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            src.as_ptr(),
                            page_phys as *mut u8,
                            (copy_end - copy_start) as usize,
                        );
                    }
                }
            }

            let mut flags = PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;
            if seg.writable {
                flags |= PageTableFlags::WRITABLE;
            }

            map_user_page(pml4, virt, page_phys, flags)?;
        }
    }

    // Map user stack
    let user_stack = allocate_page().ok_or("Out of memory for user stack")?;
    zero_page(user_stack);
    let user_stack_virt = USER_STACK_TOP - USER_STACK_SIZE as u64;
    map_user_page(
        pml4,
        user_stack_virt,
        user_stack,
        PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE,
    )?;
    proc.user_stack_phys = user_stack;
    proc.user_rsp = USER_STACK_TOP;

    // Allocate kernel stack for this process (4 KiB, single page).
    let kernel_stack = allocate_page().ok_or("Out of memory for kernel stack")?;
    zero_page(kernel_stack);
    proc.kernel_stack_phys = kernel_stack;
    proc.kernel_rsp = kernel_stack + KERNEL_STACK_SIZE as u64;

    // CRITICAL: Map the kernel stack in the user PML4 at its physical address
    // (identity mapping). This is required because enter_user_mode accesses
    // the kernel stack AFTER switching CR3 to the user PML4. Without this
    // mapping, the kernel stack is inaccessible and the CPU triple-faults.
    map_user_page(
        pml4,
        kernel_stack,
        kernel_stack,
        PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
    )?;

    proc.elf_entry = elf_info.entry;
    proc.build_initial_kernel_stack();

    proc.state = ProcessState::Running;
    unsafe { CURRENT_PID = proc.pid; }

    Ok(())
}

/// Enter Ring 3 by IRETQ to the init process.
///
/// This switches CR3 to the user page tables and IRETQs to user code.
/// The kernel stack is accessible because it was identity-mapped in the
/// user PML4 by `launch_init_process`.
pub fn enter_user_mode() -> ! {
    use x86_64::registers::control::Cr3;

    let (kernel_rsp, pml4_phys) = {
        let table = PROCESS_TABLE.lock();
        let proc = table.iter().find(|p| p.state == ProcessState::Running)
            .expect("No running process");
        (proc.kernel_rsp, proc.pml4_phys)
    };

    crate::arch::gdt::set_tss_rsp0(kernel_rsp);

    // Switch CR3 to user page tables. The kernel stack is identity-mapped
    // in the user PML4, so we can still access it after the switch.
    unsafe {
        let pml4_frame = x86_64::structures::paging::PhysFrame::containing_address(
            PhysAddr::new(pml4_phys),
        );
        Cr3::write(pml4_frame, Cr3Flags::empty());
    }

    // Pop GPRs from kernel stack and IRETQ to Ring 3.
    // The kernel_rsp points to saved R15 (bottom of GPR saves).
    // Above it: R15..RAX, then IRETQ frame (RIP, CS, RFLAGS, RSP, SS).
    unsafe {
        core::arch::asm!(
            "mov rsp, {rsp}",
            "pop r15",
            "pop r14",
            "pop r13",
            "pop r12",
            "pop r11",
            "pop r10",
            "pop r9",
            "pop r8",
            "pop rbp",
            "pop rdi",
            "pop rsi",
            "pop rdx",
            "pop rcx",
            "pop rbx",
            "pop rax",
            "iretq",
            rsp = in(reg) kernel_rsp,
            options(noreturn)
        );
    }
}

/// Exit the current process and return to the shell.
///
/// Frees all process resources (address space, stacks), restores kernel
/// page tables, and returns to the shell via saved RSP. Never returns.
pub fn exit_process(code: i32) -> ! {
    use x86_64::registers::control::Cr3;

    // Extract resource addresses before dropping the lock.
    let (pml4_phys, kernel_stack_phys, user_stack_phys) = {
        let mut table = PROCESS_TABLE.lock();
        if let Some(proc) = table.iter_mut().find(|p| p.state == ProcessState::Running) {
            let pml4 = proc.pml4_phys;
            let kstack = proc.kernel_stack_phys;
            let ustack = proc.user_stack_phys;
            proc.state = ProcessState::Exited;
            proc.exit_code = code;
            proc.kernel_rsp = 0;
            proc.kernel_stack_phys = 0;
            proc.user_stack_phys = 0;
            (pml4, kstack, ustack)
        } else {
            (0u64, 0u64, 0u64)
        }
    };

    // Free user-space page tables and leaf frames.
    free_process_address_space(pml4_phys);

    // Free user stack and kernel stack frames.
    if user_stack_phys != 0 {
        free_page(user_stack_phys);
    }
    if kernel_stack_phys != 0 {
        free_page(kernel_stack_phys);
    }

    // Restore kernel page tables.
    unsafe {
        let pml4_frame = x86_64::structures::paging::PhysFrame::containing_address(
            PhysAddr::new(KERNEL_PML4_PHYS),
        );
        Cr3::write(pml4_frame, Cr3Flags::empty());
    }

    // Reset TSS RSP0 to boot stack for future interrupts.
    crate::arch::gdt::set_tss_rsp0(0x80000);

    // Reset current PID, set exit code, and mark exit as completed.
    // EXIT_REQUESTED is used by syscall_entry to skip iretq and return to shell.
    unsafe {
        CURRENT_PID = 0;
        EXIT_CODE = code;
        EXIT_REQUESTED = true;
    }

    let shell_rsp = unsafe { SHELL_RSP };
    unsafe {
        core::arch::asm!(
            "mov rsp, {}",
            "ret",
            in(reg) shell_rsp,
        );
    }
    loop {}
}

/// Validate that a user-space pointer range is accessible from Ring 3.
///
/// Returns `true` if every byte in `[ptr, ptr + len)` is mapped with
/// USER_ACCESSIBLE in the current page tables. Prevents kernel crashes
/// from invalid user pointers and kernel pointer leaks.
pub fn validate_user_ptr(ptr: u64, len: u64) -> bool {
    use x86_64::registers::control::Cr3;
    use x86_64::VirtAddr;

    if ptr == 0 || len == 0 {
        return false;
    }

    // Check for addresses in kernel space (upper half).
    if ptr >= 0xFFFF_8000_0000_0000 {
        return false;
    }

    // Check that the range doesn't overflow AND doesn't cross into kernel space.
    let end = ptr.saturating_add(len);
    if end < ptr || end > 0xFFFF_8000_0000_0000 {
        return false;
    }

    // Walk page tables to verify all pages in the range are user-accessible.
    let (pml4_frame, _) = Cr3::read();
    let pml4_phys = pml4_frame.start_address().as_u64();
    let pml4 = unsafe { &*(pml4_phys as *const x86_64::structures::paging::PageTable) };

    let mut addr = ptr & !0xFFF; // Align down to page boundary.
    let end_page = (end - 1) & !0xFFF;

    while addr <= end_page {
        let virt = VirtAddr::new(addr);
        let p4_idx = usize::from(virt.p4_index());
        let p3_idx = usize::from(virt.p3_index());
        let p2_idx = usize::from(virt.p2_index());
        let p1_idx = usize::from(virt.p1_index());

        // Walk PML4 -> PDP -> PD -> PT.
        let p4_entry = &pml4[p4_idx];
        if !p4_entry.flags().contains(x86_64::structures::paging::PageTableFlags::PRESENT) {
            return false;
        }
        let pdp = unsafe { &*(p4_entry.addr().as_u64() as *const x86_64::structures::paging::PageTable) };
        let p3_entry = &pdp[p3_idx];
        if !p3_entry.flags().contains(x86_64::structures::paging::PageTableFlags::PRESENT) {
            return false;
        }
        if p3_entry.flags().contains(x86_64::structures::paging::PageTableFlags::HUGE_PAGE) {
            addr = (addr & !((1u64 << 30) - 1)) + (1u64 << 30);
            continue;
        }
        let pd = unsafe { &*(p3_entry.addr().as_u64() as *const x86_64::structures::paging::PageTable) };
        let p2_entry = &pd[p2_idx];
        if !p2_entry.flags().contains(x86_64::structures::paging::PageTableFlags::PRESENT) {
            return false;
        }
        if p2_entry.flags().contains(x86_64::structures::paging::PageTableFlags::HUGE_PAGE) {
            addr = (addr & !((1u64 << 21) - 1)) + (1u64 << 21);
            continue;
        }
        let pt = unsafe { &*(p2_entry.addr().as_u64() as *const x86_64::structures::paging::PageTable) };
        let p1_entry = &pt[p1_idx];
        if !p1_entry.flags().contains(x86_64::structures::paging::PageTableFlags::PRESENT) {
            return false;
        }
        if !p1_entry.flags().contains(x86_64::structures::paging::PageTableFlags::USER_ACCESSIBLE) {
            return false;
        }

        addr += 4096;
    }

    true
}
