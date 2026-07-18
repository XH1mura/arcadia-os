pub mod heap;
pub mod physmem;
pub mod vmm;

/// Initialize the memory subsystem.
///
/// # Prerequisites
///
/// `heap::init_heap()` must be called before this function (the kernel
/// entry point handles this before HVM memory map parsing).
pub fn init(total_memory: u64) {
    physmem::init(total_memory);
    vmm::init();
}

/// Initialize the memory subsystem with a bootloader-provided memory map.
///
/// # Prerequisites
///
/// `heap::init_heap()` must be called before this function (the kernel
/// entry point handles this before HVM memory map parsing).
pub fn init_with_memmap(total_memory: u64, entries: &[physmem::MemoryRegion]) {
    physmem::init_with_memmap(total_memory, entries);
    vmm::init();
}
