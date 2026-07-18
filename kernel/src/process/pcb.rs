use core::sync::atomic::{AtomicU32, Ordering};

pub const MAX_PROCESSES: usize = 8;
pub const USER_STACK_SIZE: usize = 8 * 1024;
pub const KERNEL_STACK_SIZE: usize = 8 * 1024;

pub const USER_STACK_TOP: u64 = 0x0000_7FFF_FFFF_F000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    Unused,
    Ready,
    Running,
    Blocked,
    Sleeping,
    Exited,
}

/// Saved CPU register state for context switching.
///
/// Layout must match the push/pop order in save_context/restore_context asm.
/// Fields are ordered bottom-to-top as they appear on the kernel stack
/// after the interrupt frame and our register saves.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Context {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rbp: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,
    /// RIP from interrupt frame (or initial entry point).
    pub rip: u64,
    /// CS from interrupt frame (KERNEL_CS or USER_CS).
    pub cs: u64,
    /// RFLAGS from interrupt frame.
    pub rflags: u64,
    /// RSP from interrupt frame (kernel or user stack).
    pub rsp: u64,
    /// SS from interrupt frame (KERNEL_SS or USER_SS).
    pub ss: u64,
}

pub struct ProcessControlBlock {
    pub pid: u32,
    pub state: ProcessState,
    pub kernel_rsp: u64,
    pub user_rsp: u64,
    pub pml4_phys: u64,
    pub elf_entry: u64,
    pub exit_code: i32,
    pub kernel_stack_phys: u64,
    pub user_stack_phys: u64,
    /// Tick at which a sleeping process should be woken (0 = not sleeping).
    pub sleep_until: u64,
}

static NEXT_PID: AtomicU32 = AtomicU32::new(1);

impl ProcessControlBlock {
    pub fn new() -> Self {
        let pid = NEXT_PID.fetch_add(1, Ordering::Relaxed);
        ProcessControlBlock {
            pid,
            state: ProcessState::Unused,
            kernel_rsp: 0,
            user_rsp: USER_STACK_TOP,
            pml4_phys: 0,
            elf_entry: 0,
            exit_code: 0,
            kernel_stack_phys: 0,
            user_stack_phys: 0,
            sleep_until: 0,
        }
    }

    pub fn build_initial_kernel_stack(&mut self) {
        let stack_base = self.kernel_stack_phys;
        let rsp = stack_base + KERNEL_STACK_SIZE as u64;

        let mut frame = rsp;

        // Interrupt frame for IRETQ (pushed by CPU on interrupt, or manually here)
        frame -= 8;
        unsafe { core::ptr::write_volatile(frame as *mut u64, crate::arch::gdt::USER_SS as u64); }  // SS
        frame -= 8;
        unsafe { core::ptr::write_volatile(frame as *mut u64, self.user_rsp); }  // RSP
        frame -= 8;
        unsafe { core::ptr::write_volatile(frame as *mut u64, 0x200u64); }  // RFLAGS (IF=1)
        frame -= 8;
        unsafe { core::ptr::write_volatile(frame as *mut u64, crate::arch::gdt::USER_CS as u64); }  // CS
        frame -= 8;
        unsafe { core::ptr::write_volatile(frame as *mut u64, self.elf_entry); }  // RIP

        // GPRs (pushed by our context switch stub)
        frame -= 8; unsafe { core::ptr::write_volatile(frame as *mut u64, 0); }  // RAX
        frame -= 8; unsafe { core::ptr::write_volatile(frame as *mut u64, 0); }  // RBX
        frame -= 8; unsafe { core::ptr::write_volatile(frame as *mut u64, 0); }  // RCX
        frame -= 8; unsafe { core::ptr::write_volatile(frame as *mut u64, 0); }  // RDX
        frame -= 8; unsafe { core::ptr::write_volatile(frame as *mut u64, 0); }  // RSI
        frame -= 8; unsafe { core::ptr::write_volatile(frame as *mut u64, 0); }  // RDI
        frame -= 8; unsafe { core::ptr::write_volatile(frame as *mut u64, 0); }  // RBP
        frame -= 8; unsafe { core::ptr::write_volatile(frame as *mut u64, 0); }  // R8
        frame -= 8; unsafe { core::ptr::write_volatile(frame as *mut u64, 0); }  // R9
        frame -= 8; unsafe { core::ptr::write_volatile(frame as *mut u64, 0); }  // R10
        frame -= 8; unsafe { core::ptr::write_volatile(frame as *mut u64, 0); }  // R11
        frame -= 8; unsafe { core::ptr::write_volatile(frame as *mut u64, 0); }  // R12
        frame -= 8; unsafe { core::ptr::write_volatile(frame as *mut u64, 0); }  // R13
        frame -= 8; unsafe { core::ptr::write_volatile(frame as *mut u64, 0); }  // R14
        frame -= 8; unsafe { core::ptr::write_volatile(frame as *mut u64, 0); }  // R15

        self.kernel_rsp = frame;
    }
}
