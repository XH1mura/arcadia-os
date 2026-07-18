use core::arch::naked_asm;

pub const SYS_WRITE: u64 = 1;
pub const SYS_EXIT: u64 = 2;
pub const SYS_YIELD: u64 = 3;

/// INT 0x80 entry point for syscalls from Ring 3.
///
/// CPU pushes SS, RSP, RFLAGS, CS, RIP on privilege change.
/// We save all GPRs, call the Rust dispatcher, then either IRETQ
/// (normal return) or restore the shell stack (on process exit).
#[unsafe(naked)]
pub extern "sysv64" fn syscall_entry() {
    naked_asm!(
        // Save all GPRs (15 pushes).
        "push rax",
        "push rbx",
        "push rcx",
        "push rdx",
        "push rsi",
        "push rdi",
        "push rbp",
        "push r8",
        "push r9",
        "push r10",
        "push r11",
        "push r12",
        "push r13",
        "push r14",
        "push r15",

        // Extract syscall arguments from saved GPRs.
        // [rsp + 14*8] = RAX (syscall number)
        // [rsp + 9*8]  = RDI (arg0)
        // [rsp + 10*8] = RSI (arg1)
        // [rsp + 11*8] = RDX (arg2)
        "mov rdi, [rsp + 14*8]",
        "mov rsi, [rsp + 9*8]",
        "mov rdx, [rsp + 10*8]",
        "mov rcx, [rsp + 11*8]",

        "call {dispatch}",

        // Check if the process requested exit.
        "cmp byte ptr [{exit_flag}], 0",
        "jne 2f",

        // Store return value in saved RAX.
        "mov [rsp + 14*8], rax",

        // Restore all GPRs.
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

        // Process exit path: restore shell stack.
        "2:",
        "mov rsp, [{shell_rsp}]",
        "ret",

        dispatch = sym syscall_dispatch,
        exit_flag = sym crate::process::EXIT_REQUESTED,
        shell_rsp = sym crate::process::SHELL_RSP,
    );
}

/// Write bytes from user-space buffer to the serial port.
///
/// Validates that `buf` is a valid user pointer before reading.
fn sys_write(buf: u64, len: u64) -> u64 {
    if len == 0 {
        return 0;
    }

    // Validate user pointer — prevent kernel crash and pointer leaks.
    if !crate::process::validate_user_ptr(buf, len) {
        return u64::MAX; // -EFAULT
    }

    let mut written = 0u64;
    for i in 0..len {
        let byte: u8;
        unsafe {
            byte = core::ptr::read_volatile((buf + i) as *const u8);
        }
        crate::serial_print!("{}", byte as char);
        written += 1;
    }
    written
}

/// Exit the current process with the given exit code.
fn sys_exit(code: i64) -> u64 {
    unsafe {
        crate::process::EXIT_REQUESTED = true;
    }
    crate::process::exit_process(code as i32);
}

/// Yield the CPU (no-op in cooperative scheduler).
fn sys_yield() -> u64 {
    0
}

/// Dispatch a syscall from INT 0x80.
///
/// # Safety
/// Called only from `syscall_entry` naked asm. Arguments come from
/// user-controlled GPRs; the dispatcher must validate them.
#[no_mangle]
pub extern "C" fn syscall_dispatch(
    syscall_num: u64,
    arg0: u64,
    arg1: u64,
    _arg2: u64,
) -> u64 {
    match syscall_num {
        SYS_WRITE => sys_write(arg0, arg1),
        SYS_EXIT => sys_exit(arg0 as i64),
        SYS_YIELD => sys_yield(),
        _ => u64::MAX, // -ENOSYS
    }
}
