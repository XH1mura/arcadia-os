use core::arch::naked_asm;

use super::pcb::Context;

/// Save the current CPU register state into `ctx`.
///
/// Must be called from a naked ISR stub that has pushed all 15 GPRs
/// onto the kernel stack. `rdi` must point to a valid `Context`.
///
/// # Safety
/// `ctx` must point to a valid, aligned `Context` struct.
pub unsafe fn save_context(ctx: *mut Context) {
    unsafe {
        core::arch::asm!(
            "mov [rdi + 0*8],  r15",
            "mov [rdi + 1*8],  r14",
            "mov [rdi + 2*8],  r13",
            "mov [rdi + 3*8],  r12",
            "mov [rdi + 4*8],  r11",
            "mov [rdi + 5*8],  r10",
            "mov [rdi + 6*8],  r9",
            "mov [rdi + 7*8],  r8",
            "mov [rdi + 8*8],  rbp",
            "mov [rdi + 9*8],  rdi",
            "mov [rdi + 10*8], rsi",
            "mov [rdi + 11*8], rdx",
            "mov [rdi + 12*8], rcx",
            "mov [rdi + 13*8], rbx",
            "mov [rdi + 14*8], rax",
            // Interrupt frame: read from kernel stack above GPR saves.
            "mov r8,  [rsp + 15*8]",   // RIP
            "mov [rdi + 15*8], r8",
            "mov r8,  [rsp + 16*8]",   // CS
            "mov [rdi + 16*8], r8",
            "mov r8,  [rsp + 17*8]",   // RFLAGS
            "mov [rdi + 17*8], r8",
            "mov r8,  [rsp + 18*8]",   // RSP
            "mov [rdi + 18*8], r8",
            "mov r8,  [rsp + 19*8]",   // SS
            "mov [rdi + 19*8], r8",
            in("rdi") ctx,
            lateout("r8") _,
            options(nostack),
        );
    }
}

/// Restore CPU register state from `ctx` and return via IRETQ.
///
/// Never returns to the caller. Jumps to `ctx.rip` in the privilege
/// level specified by `ctx.cs`.
///
/// # Safety
/// `ctx` must contain a valid register state previously saved by
/// `save_context` or `build_initial_kernel_stack`.
#[unsafe(naked)]
pub unsafe extern "sysv64" fn restore_context(ctx: *const Context) -> ! {
    naked_asm!(
        "mov r15, [rdi + 0*8]",
        "mov r14, [rdi + 1*8]",
        "mov r13, [rdi + 2*8]",
        "mov r12, [rdi + 3*8]",
        "mov r11, [rdi + 4*8]",
        "mov r10, [rdi + 5*8]",
        "mov r9,  [rdi + 6*8]",
        "mov r8,  [rdi + 7*8]",
        "mov rbp, [rdi + 8*8]",
        "mov rsi, [rdi + 10*8]",
        "mov rdx, [rdi + 11*8]",
        "mov rcx, [rdi + 12*8]",
        "mov rbx, [rdi + 13*8]",
        "mov rax, [rdi + 14*8]",
        "lea rsp, [rdi + 15*8]",
        "mov rdi, [rdi + 9*8]",
        "iretq",
    )
}

/// Context switch: save current kernel RSP into `old_pcb`, load `new_rsp`,
/// pop GPRs, IRETQ to the restored process.
///
/// Must be reached via `jmp` (NOT `call`) so no return address pollutes the
/// kernel stack layout. The ISR must push all 15 GPRs before jumping here,
/// creating the standard Context layout on the kernel stack.
///
/// # Safety
/// - `old_rsp_ptr` must point to the `kernel_rsp` field of the current
///   process's PCB.
/// - `new_rsp` must be a valid kernel stack pointer whose top contains a
///   saved Context (GPRs + interrupt frame).
/// - Interrupts must be disabled before calling.
#[unsafe(naked)]
pub unsafe extern "sysv64" fn switch_context(
    old_rsp_ptr: *mut u64,
    new_rsp: u64,
) -> ! {
    naked_asm!(
        "mov [rdi], rsp",
        "mov rsp, rsi",
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
    )
}
