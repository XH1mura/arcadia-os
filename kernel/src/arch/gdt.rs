use x86_64::VirtAddr;
use x86_64::structures::gdt::{GlobalDescriptorTable, Descriptor, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;

pub const STACK_IST_INDEX: u16 = 0;

pub const USER_CS: u16 = 0x2B;
pub const USER_SS: u16 = 0x23;

pub static mut TSS: TaskStateSegment = TaskStateSegment::new();

struct KernelGdt {
    gdt: GlobalDescriptorTable,
    code_selector: SegmentSelector,
    data_selector: SegmentSelector,
    tss_selector: SegmentSelector,
    #[allow(dead_code)]
    user_code_selector: SegmentSelector,
    #[allow(dead_code)]
    user_data_selector: SegmentSelector,
}

static GDT: spin::Lazy<KernelGdt> = spin::Lazy::new(|| {
    let mut gdt = GlobalDescriptorTable::new();
    let code_selector = gdt.append(Descriptor::kernel_code_segment());
    let data_selector = gdt.append(Descriptor::kernel_data_segment());
    let tss_selector = gdt.append(unsafe {
        #[allow(static_mut_refs)]
        Descriptor::tss_segment(&TSS)
    });
    let user_data_selector = gdt.append(Descriptor::user_data_segment());
    let user_code_selector = gdt.append(Descriptor::user_code_segment());
    KernelGdt {
        gdt,
        code_selector,
        data_selector,
        tss_selector,
        user_code_selector,
        user_data_selector,
    }
});

pub fn init() {
    use x86_64::instructions::segmentation::{CS, DS, Segment};
    use x86_64::instructions::tables::load_tss;

    // Initialize IST entries for critical exception stacks.
    // IST[0] = Double-fault stack (at 0x65000, 20 KiB region 0x60000-0x65000).
    // IST[1] = NMI stack (at 0x6A000, 20 KiB region 0x65000-0x6A000).
    // These addresses are reserved by the PMM during init_standard_reservations.
    unsafe {
        TSS.interrupt_stack_table[0] = VirtAddr::new(0x65000);
        TSS.interrupt_stack_table[1] = VirtAddr::new(0x6A000);
    }

    unsafe {
        GDT.gdt.load_unsafe();
        CS::set_reg(GDT.code_selector);
        DS::set_reg(GDT.data_selector);
        load_tss(GDT.tss_selector);
    }
}

pub fn set_tss_rsp0(rsp: u64) {
    unsafe {
        TSS.privilege_stack_table[0] = VirtAddr::new(rsp);
    }
}
