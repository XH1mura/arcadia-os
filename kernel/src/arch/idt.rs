use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};
use x86_64::PrivilegeLevel;

// IST index allocations for critical stacks.
// IST[0] = Double-fault stack (already set in GDT)
// IST[1] = NMI stack
// IST[2] = Debug stack
const IST_DF: u16 = 0;
const IST_NMI: u16 = 1;

lazy_static::lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();

        // Double fault — use IST[0] to guarantee a valid stack.
        unsafe {
            idt.double_fault
                .set_handler_fn(double_fault_handler)
                .set_stack_index(IST_DF);
        }

        idt.breakpoint.set_handler_fn(breakpoint_handler);
        idt.page_fault.set_handler_fn(page_fault_handler);

        // Non-Maskable Interrupt — use IST[1].
        unsafe {
            idt.non_maskable_interrupt
                .set_handler_fn(nmi_handler)
                .set_stack_index(IST_NMI);
        }

        // Hardware interrupts
        idt[32].set_handler_fn(timer_interrupt_handler);
        idt[33].set_handler_fn(keyboard_interrupt_handler);
        idt[36].set_handler_fn(serial_interrupt_handler);

        // INT 0x80 — syscall entry (Ring 3 accessible).
        // Does NOT use IST: runs on the kernel stack switched via TSS.RSP0.
        unsafe {
            let handler_fn: x86_64::structures::idt::HandlerFunc =
                core::mem::transmute(crate::process::syscall::syscall_entry as *const ());
            let opts = idt[0x80].set_handler_fn(handler_fn);
            opts.set_privilege_level(PrivilegeLevel::Ring3);
        }

        idt
    };
}

pub fn init() {
    IDT.load();
}

extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    crate::serial_println!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    _error_code: u64,
) -> ! {
    panic!("EXCEPTION: DOUBLE FAULT\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn nmi_handler(stack_frame: InterruptStackFrame) {
    crate::serial_println!("EXCEPTION: NMI\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    // Use serial_println only — println acquires the VGA WRITER lock which
    // can deadlock if the fault occurred during a VGA write.
    crate::serial_println!("EXCEPTION: PAGE FAULT");
    crate::serial_println!("Accessed Address: {:?}", x86_64::registers::control::Cr2::read());
    crate::serial_println!("Error Code: {:?}", error_code);
    crate::serial_println!("{:#?}", stack_frame);
    panic!("PAGE FAULT");
}

extern "x86-interrupt" fn timer_interrupt_handler(_stack_frame: InterruptStackFrame) {
    crate::time::tick();
    unsafe {
        x86_64::instructions::port::Port::<u8>::new(0x20).write(0x20u8);
    }
}

extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    use x86_64::instructions::port::Port;

    let scancode: u8 = unsafe { Port::<u8>::new(0x60).read() };

    crate::terminal::push_scancode(scancode);

    unsafe {
        crate::interrupts::PICS
            .lock()
            .notify_end_of_interrupt(crate::interrupts::HardwareInterrupt::Keyboard.as_u8());
    }
}

extern "x86-interrupt" fn serial_interrupt_handler(_stack_frame: InterruptStackFrame) {
    unsafe {
        crate::interrupts::PICS
            .lock()
            .notify_end_of_interrupt(crate::interrupts::HardwareInterrupt::Serial1.as_u8());
    }
}
