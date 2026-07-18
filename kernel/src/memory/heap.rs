use linked_list_allocator::LockedHeap;

const HEAP_START: usize = 0x400_000; // 4 MiB (within identity-mapped region)
const HEAP_SIZE: usize = 128 * 1024; // 128 KiB

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

pub fn init_heap() {
    unsafe {
        ALLOCATOR.lock().init(HEAP_START as *mut u8, HEAP_SIZE);
    }
}
