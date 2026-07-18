#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec;
use core::fmt::Write;
use uefi::prelude::*;
use uefi::proto::media::file::{File, FileAttribute, FileInfo, FileMode};
use uefi::proto::media::fs::SimpleFileSystem;
use uefi::table::boot::{AllocateType, MemoryType};

const KERNEL_PATH: &str = r"\EFI\Arcadia\kernel.elf";
const KERNEL_MEMORY_TYPE: MemoryType = MemoryType::custom(0x80000000);

#[entry]
fn efi_main(image: Handle, mut system_table: SystemTable<Boot>) -> Status {
    system_table.stdout().clear().unwrap();
    writeln!(system_table.stdout(), "Arcadia Bootloader").unwrap();
    writeln!(system_table.stdout(), "Loading kernel...").unwrap();

    let kernel_data = match load_kernel(&system_table) {
        Some(data) => data,
        None => {
            writeln!(system_table.stdout(), "ERROR: Failed to load kernel").unwrap();
            return Status::LOAD_ERROR;
        }
    };

    writeln!(
        system_table.stdout(),
        "Kernel loaded: {} bytes",
        kernel_data.len()
    )
    .unwrap();

    let kernel_size_pages = (kernel_data.len() + 0xFFF) / 0x1000;
    let kernel_start = 0x100000u64;

    let _ = system_table.boot_services().allocate_pages(
        AllocateType::Address(kernel_start),
        KERNEL_MEMORY_TYPE,
        kernel_size_pages,
    );

    unsafe {
        let dest = kernel_start as *mut u8;
        core::ptr::copy_nonoverlapping(kernel_data.as_ptr(), dest, kernel_data.len());
    }

    writeln!(system_table.stdout(), "Kernel at 0x{:X}", kernel_start).unwrap();
    writeln!(system_table.stdout(), "Exiting boot services...").unwrap();

    let (_runtime, memory_map) = system_table.exit_boot_services(MemoryType::LOADER_DATA);

    let mut total_memory: u64 = 0;
    for entry in memory_map.entries() {
        let end = entry.phys_start + (entry.page_count * 0x1000);
        if end > total_memory {
            total_memory = end;
        }
    }

    let kernel_entry: extern "sysv64" fn(u64, u64) -> ! =
        unsafe { core::mem::transmute(kernel_start as *const ()) };

    kernel_entry(0, total_memory)
}

fn load_kernel(system_table: &SystemTable<Boot>) -> Option<alloc::vec::Vec<u8>> {
    let bs = system_table.boot_services();
    let handle = bs.get_handle_for_protocol::<SimpleFileSystem>().ok()?;
    let mut fs = bs
        .open_protocol_exclusive::<SimpleFileSystem>(handle)
        .ok()?;
    let mut root_dir = fs.open_volume().ok()?;

    let path =
        uefi::CString16::try_from(" \\ E F I \\ A r c a d i a \\ k e r n e l . e l f").ok()?;

    let mut file = root_dir
        .open(&path, FileMode::Read, FileAttribute::empty())
        .ok()?;
    let mut info_buf = vec![0u8; 512];
    let file_info = file.get_info::<FileInfo>(&mut info_buf).ok()?;
    let file_size = file_info.file_size() as usize;

    let mut kernel_data = vec![0u8; file_size];
    file.read(&mut kernel_data).ok()?;

    Some(kernel_data)
}
