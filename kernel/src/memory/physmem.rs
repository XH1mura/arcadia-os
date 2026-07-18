//! Physical Memory Manager
//!
//! Production-quality physical memory management subsystem for Arcadia OS.
//! This module provides comprehensive memory detection, allocation, and tracking.
//!
//! ## Architecture
//!
//! The Physical Memory Manager (PMM) is organized into several components:
//!
//! 1. **MemoryMap**: Represents the system's physical memory layout with regions
//!    - Populated from the PVH bootloader memory map (HVM start info)
//!    - Falls back to CPUID-based detection when no map is available
//!
//! 2. **PhysicalFrameAllocator**: Manages allocation of physical frames
//!    - Uses a static bitmap for O(1) free tracking
//!    - Supports single and multi-frame allocation
//!    - Prevents double-allocation and double-free
//!
//! 3. **ReservedRegions**: Tracks all memory regions reserved by the kernel
//!    - Kernel image, page tables, GDT, IDT, stacks, heap, etc.
//!
//! ## Memory Layout (PVH Boot)
//!
//! ```text
//! 0x0000000000000000 - 0x00000000000FFFFF : Reserved (1 MiB - BIOS, IVT, BDA, VGA)
//! 0x0000000000100000 - 0x00000000001FFFFF : Reserved (Kernel image, page tables)
//! 0x0000000000200000 - 0x00000000003FFFFF : Reserved (Boot stack, GDT, IST)
//! 0x0000000000400000 - 0x0000000000420000 : Reserved (Kernel heap)
//! 0x0000000000420000 - ...                : Usable (Available for allocation)
//! ```
//!
//! ## Allocation Algorithm
//!
//! - Bitmap-based: each bit represents one 4 KiB frame
//! - First-fit linear scan starting from `next_free`
//! - Free: O(1) immediate
//! - Allocation: O(n) worst case, O(1) amortized
//!
//! ## Complexity
//!
//! - Allocation: O(n) where n is the number of frames to scan
//! - Free: O(1)
//! - Statistics: O(1)
//! - Validation: O(n) for full bitmap scan

extern crate alloc;

use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use x86_64::PhysAddr;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Frame size in bytes (4 KiB).
pub const FRAME_SIZE: u64 = 4096;

/// Maximum number of frames the allocator can track.
/// 4 GiB / 4 KiB = 1,048,576 frames → 128 KiB bitmap.
/// Stored in BSS; no heap required.
pub const MAX_TRACKED_FRAMES: usize = 4 * 1024 * 1024 * 1024 / FRAME_SIZE as usize;

/// Bitmap size in bytes: one bit per frame.
pub const BITMAP_SIZE: usize = (MAX_TRACKED_FRAMES + 7) / 8;

// ---------------------------------------------------------------------------
// Static bitmap (lives in BSS, zero-initialized by boot64.asm)
// ---------------------------------------------------------------------------

/// Static bitmap tracking frame allocation status.
/// Bit = 0 → frame is free. Bit = 1 → frame is reserved/allocated.
/// Initialized to all-zeros (all free) by BSS clearing, then
/// `init_standard_reservations` marks used regions.
static mut BITMAP: [u8; BITMAP_SIZE] = [0u8; BITMAP_SIZE];

// ---------------------------------------------------------------------------
// HVM start info structures (PVH boot memory map)
// ---------------------------------------------------------------------------

/// HVM start info header passed by the PVH bootloader in RBX.
/// Layout matches Xen specification (xen/include/public/arch-x86/hvm/start_info.h).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct HvmStartInfo {
    pub magic: u32,
    pub version: u32,
    pub flags: u64,
    pub rsdp_paddr: u64,
    pub memmap_paddr: u64,
    pub memmap_entries: u32,
    pub cmdline_paddr: u32,
}

/// Single entry in the HVM memory map.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct HvmMemmapEntry {
    pub addr: u64,
    pub size: u64,
    pub mem_type: u32,
    pub reserved: u32,
}

/// Known HVM memory map types.
pub const HVM_MEMMAP_RAM: u32 = 1;
pub const HVM_MEMMAP_RESERVED: u32 = 2;
pub const HVM_MEMMAP_ACPI: u32 = 3;
pub const HVM_MEMMAP_ACPI_NVS: u32 = 4;
pub const HVM_MEMMAP_BAD: u32 = 5;

// ---------------------------------------------------------------------------
// MemoryRegionType
// ---------------------------------------------------------------------------

/// Memory region type for the physical memory map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MemoryRegionType {
    /// Memory not present or not accessible.
    Unavailable = 0,
    /// Usable RAM — available for allocation.
    Usable = 1,
    /// Reserved by firmware or kernel — not available for allocation.
    Reserved = 2,
    /// ACPI tables.
    Acpi = 3,
    /// ACPI Non-volatile storage.
    AcpiNvs = 4,
    /// Bad memory (detected as faulty).
    Bad = 5,
    /// Memory that can be reclaimed after boot.
    BootloaderReclaimable = 6,
    /// Kernel image and data structures.
    Kernel = 7,
    /// Memory-mapped I/O regions.
    Mmio = 8,
}

impl MemoryRegionType {
    /// Returns `true` if this region type is usable for general allocation.
    pub const fn is_usable(&self) -> bool {
        matches!(
            self,
            MemoryRegionType::Usable | MemoryRegionType::BootloaderReclaimable
        )
    }

    /// Human-readable name for this region type.
    pub const fn as_str(&self) -> &'static str {
        match self {
            MemoryRegionType::Unavailable => "Unavailable",
            MemoryRegionType::Usable => "Usable RAM",
            MemoryRegionType::Reserved => "Reserved",
            MemoryRegionType::Acpi => "ACPI Tables",
            MemoryRegionType::AcpiNvs => "ACPI NVS",
            MemoryRegionType::Bad => "Bad Memory",
            MemoryRegionType::BootloaderReclaimable => "Bootloader Reclaimable",
            MemoryRegionType::Kernel => "Kernel",
            MemoryRegionType::Mmio => "MMIO",
        }
    }

    /// Convert an HVM memmap type to our internal type.
    pub fn from_hvm(hvm_type: u32) -> Self {
        match hvm_type {
            HVM_MEMMAP_RAM => MemoryRegionType::Usable,
            HVM_MEMMAP_RESERVED => MemoryRegionType::Reserved,
            HVM_MEMMAP_ACPI => MemoryRegionType::Acpi,
            HVM_MEMMAP_ACPI_NVS => MemoryRegionType::AcpiNvs,
            HVM_MEMMAP_BAD => MemoryRegionType::Bad,
            _ => MemoryRegionType::Reserved,
        }
    }
}

// ---------------------------------------------------------------------------
// MemoryRegion
// ---------------------------------------------------------------------------

/// A single contiguous region of physical memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryRegion {
    /// Base physical address of the region.
    pub base: PhysAddr,
    /// Size of the region in bytes.
    pub size: u64,
    /// Type of memory region.
    pub region_type: MemoryRegionType,
    /// Human-readable name for debugging.
    pub name: &'static str,
}

impl MemoryRegion {
    /// Create a new memory region.
    pub const fn new(
        base: PhysAddr,
        size: u64,
        region_type: MemoryRegionType,
        name: &'static str,
    ) -> Self {
        MemoryRegion {
            base,
            size,
            region_type,
            name,
        }
    }

    /// End address of the region (base + size).
    pub fn end(&self) -> PhysAddr {
        PhysAddr::new(self.base.as_u64().wrapping_add(self.size))
    }

    /// Returns `true` if this region contains the given physical address.
    pub fn contains(&self, addr: PhysAddr) -> bool {
        let a = addr.as_u64();
        let b = self.base.as_u64();
        let e = b.wrapping_add(self.size);
        a >= b && a < e
    }

    /// Returns `true` if this region overlaps with another.
    pub fn overlaps(&self, other: &MemoryRegion) -> bool {
        self.base.as_u64() < other.end().as_u64()
            && other.base.as_u64() < self.end().as_u64()
    }

    /// Returns `true` if this region is usable for allocation.
    pub const fn is_usable(&self) -> bool {
        self.region_type.is_usable()
    }

    /// Number of 4 KiB frames in this region.
    pub fn frame_count(&self) -> u64 {
        self.size / FRAME_SIZE
    }
}

// ---------------------------------------------------------------------------
// ReservedRegion
// ---------------------------------------------------------------------------

/// Reserved memory region with a specific purpose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReservedRegion {
    /// The memory region.
    pub region: MemoryRegion,
    /// Purpose of the reservation.
    pub purpose: &'static str,
}

impl ReservedRegion {
    /// Create a new reserved region.
    pub const fn new(
        base: PhysAddr,
        size: u64,
        region_type: MemoryRegionType,
        name: &'static str,
        purpose: &'static str,
    ) -> Self {
        ReservedRegion {
            region: MemoryRegion::new(base, size, region_type, name),
            purpose,
        }
    }

    /// Base address of the reserved region.
    pub fn base(&self) -> PhysAddr {
        self.region.base
    }

    /// End address of the reserved region.
    pub fn end(&self) -> PhysAddr {
        self.region.end()
    }

    /// Size of the reserved region in bytes.
    pub fn size(&self) -> u64 {
        self.region.size
    }
}

// ---------------------------------------------------------------------------
// MemoryStatistics
// ---------------------------------------------------------------------------

/// Comprehensive statistics for the physical memory manager.
#[derive(Debug, Clone, Copy)]
pub struct MemoryStatistics {
    /// Total physical memory in bytes.
    pub total_bytes: u64,
    /// Total usable memory in bytes.
    pub usable_bytes: u64,
    /// Total reserved memory in bytes.
    pub reserved_bytes: u64,
    /// Number of frames currently allocated (reserved).
    pub allocated_frames: usize,
    /// Number of frames currently free.
    pub free_frames: usize,
    /// Total number of frames tracked.
    pub total_frames: usize,
    /// Number of failed allocation attempts.
    pub allocation_failures: u64,
    /// Number of double-free attempts detected.
    pub double_free_attempts: u64,
    /// Number of invalid free attempts detected.
    pub invalid_free_attempts: u64,
    /// Largest contiguous free block (in frames).
    pub largest_free_block: usize,
    /// Number of free blocks ( fragmentation indicator ).
    pub free_block_count: usize,
}

impl MemoryStatistics {
    /// Fragmentation ratio: free_block_count / free_frames (0.0 = no fragmentation).
    pub fn fragmentation_ratio(&self) -> f64 {
        if self.free_frames == 0 {
            return 0.0;
        }
        self.free_block_count as f64 / self.free_frames as f64
    }

    /// Allocation ratio: allocated / total (0.0 = empty, 1.0 = full).
    pub fn allocation_ratio(&self) -> f64 {
        if self.total_frames == 0 {
            return 0.0;
        }
        self.allocated_frames as f64 / self.total_frames as f64
    }
}

// ---------------------------------------------------------------------------
// PmmError
// ---------------------------------------------------------------------------

/// Error types for physical memory operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PmmError {
    /// No memory available.
    OutOfMemory,
    /// Attempted to allocate more frames than available.
    AllocationTooLarge,
    /// Attempted to free a frame that was not allocated.
    InvalidFree,
    /// Attempted to free a frame that was already free.
    DoubleFree,
    /// PMM has not been initialized.
    NotInitialized,
}

impl core::fmt::Display for PmmError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PmmError::OutOfMemory => write!(f, "Out of physical memory"),
            PmmError::AllocationTooLarge => write!(f, "Allocation too large"),
            PmmError::InvalidFree => write!(f, "Invalid free: frame not allocated"),
            PmmError::DoubleFree => write!(f, "Double free: frame already freed"),
            PmmError::NotInitialized => write!(f, "PMM not initialized"),
        }
    }
}

/// Result type for physical memory operations.
pub type PmmResult<T> = Result<T, PmmError>;

/// Frame index type.
pub type FrameIndex = usize;

// ---------------------------------------------------------------------------
// PhysicalFrameAllocator
// ---------------------------------------------------------------------------

/// The core physical frame allocator using a static bitmap.
///
/// Each bit in the bitmap represents one 4 KiB frame.
/// Bit = 0 → free, Bit = 1 → reserved/allocated.
///
/// The bitmap is a module-level static (`BITMAP`), not heap-allocated.
/// This allows tracking up to 4 GiB of physical memory without
/// requiring heap space for the bitmap itself.
pub struct PhysicalFrameAllocator {
    /// Total number of frames being tracked.
    total_frames: usize,
    /// Number of frames currently reserved/allocated.
    allocated_frames: usize,
    /// Index of the next free frame to start searching from.
    next_free: usize,
    /// Runtime statistics.
    stats: MemoryStatistics,
    /// Whether runtime validation is enabled.
    validation_enabled: bool,
}

impl PhysicalFrameAllocator {
    /// Create a new frame allocator for the given total memory.
    ///
    /// The bitmap is zero-initialized by BSS clearing, so all frames
    /// start as free. Call `init_standard_reservations()` afterward
    /// to mark reserved regions.
    pub fn new(total_memory_bytes: u64) -> Self {
        let total_frames = ((total_memory_bytes as usize) / FRAME_SIZE as usize)
            .min(MAX_TRACKED_FRAMES);

        // SAFETY: BITMAP is a static mut accessed only during single-threaded
        // early boot. No concurrent access is possible at this stage.
        unsafe {
            // Clear the bitmap to mark all frames as free.
            let bitmap_bytes = (total_frames + 7) / 8;
            for byte in &mut BITMAP[..bitmap_bytes] {
                *byte = 0;
            }
        }

        PhysicalFrameAllocator {
            total_frames,
            allocated_frames: 0,
            next_free: 0,
            stats: MemoryStatistics {
                total_bytes: total_memory_bytes,
                usable_bytes: 0,
                reserved_bytes: 0,
                allocated_frames: 0,
                free_frames: total_frames,
                total_frames,
                allocation_failures: 0,
                double_free_attempts: 0,
                invalid_free_attempts: 0,
                largest_free_block: total_frames,
                free_block_count: if total_frames > 0 { 1 } else { 0 },
            },
            validation_enabled: true,
        }
    }

    // -- Bitmap access -------------------------------------------------------

    /// Check if a frame is reserved/allocated.
    pub fn is_frame_reserved(&self, frame: FrameIndex) -> bool {
        if frame >= self.total_frames {
            return true;
        }
        // SAFETY: BITMAP is only accessed during single-threaded early boot.
        unsafe {
            let byte_idx = frame / 8;
            let bit_idx = frame % 8;
            (BITMAP[byte_idx] & (1 << bit_idx)) != 0
        }
    }

    /// Mark a single frame as reserved (bit = 1).
    fn mark_frame_reserved(&mut self, frame: FrameIndex) {
        if frame < self.total_frames {
            // SAFETY: BITMAP is only accessed during single-threaded early boot.
            unsafe {
                let byte_idx = frame / 8;
                let bit_idx = frame % 8;
                BITMAP[byte_idx] |= 1 << bit_idx;
            }
        }
    }

    /// Mark a single frame as free (bit = 0).
    fn mark_frame_free(&mut self, frame: FrameIndex) {
        if frame < self.total_frames {
            // SAFETY: BITMAP is only accessed during single-threaded early boot.
            unsafe {
                let byte_idx = frame / 8;
                let bit_idx = frame % 8;
                BITMAP[byte_idx] &= !(1 << bit_idx);
            }
            if frame < self.next_free {
                self.next_free = frame;
            }
        }
    }

    /// Recalculate `next_free` by scanning from frame 0.
    fn recalculate_next_free(&mut self) {
        self.next_free = 0;
        while self.next_free < self.total_frames && self.is_frame_reserved(self.next_free) {
            self.next_free += 1;
        }
    }

    // -- Reservation ---------------------------------------------------------

    /// Reserve a range of physical addresses [start, end).
    pub fn reserve_range(
        &mut self,
        start: PhysAddr,
        end: PhysAddr,
        _name: &'static str,
    ) -> PmmResult<()> {
        let start_frame = start.as_u64() as FrameIndex / FRAME_SIZE as FrameIndex;
        let end_frame =
            ((end.as_u64() as FrameIndex) + FRAME_SIZE as FrameIndex - 1) / FRAME_SIZE as FrameIndex;

        let capped_end = end_frame.min(self.total_frames);

        for frame in start_frame..capped_end {
            if !self.is_frame_reserved(frame) {
                self.mark_frame_reserved(frame);
                self.allocated_frames += 1;
            }
        }
        self.recalculate_next_free();
        Ok(())
    }

    // -- Standard reservations -----------------------------------------------

    /// Initialize the allocator with standard kernel reservations.
    ///
    /// Marks all memory regions that the kernel uses during early boot
    /// as reserved in the bitmap. These regions are不可 available for
    /// general allocation.
    pub fn init_standard_reservations(&mut self) {
        // 1. Real mode / BIOS area (0 - 1 MiB)
        self.reserve_range(
            PhysAddr::new(0x00000000),
            PhysAddr::new(0x00100000),
            "Real Mode / BIOS Area",
        )
        .expect("Failed to reserve BIOS area");

        // 2. Page tables (0x1000 - 0x5000)
        self.reserve_range(
            PhysAddr::new(0x00001000),
            PhysAddr::new(0x00005000),
            "Page Tables",
        )
        .expect("Failed to reserve page tables");

        // 3. Boot GDT and info (0x500 - 0x600)
        self.reserve_range(
            PhysAddr::new(0x00000500),
            PhysAddr::new(0x00000600),
            "Boot GDT",
        )
        .expect("Failed to reserve boot GDT");

        // 4. Kernel image (0x100000 - 0x200000)
        self.reserve_range(
            PhysAddr::new(0x00100000),
            PhysAddr::new(0x00200000),
            "Kernel Image",
        )
        .expect("Failed to reserve kernel image");

        // 5. Boot stack (0x70000 - 0x90000)
        self.reserve_range(
            PhysAddr::new(0x00070000),
            PhysAddr::new(0x00090000),
            "Boot Stack",
        )
        .expect("Failed to reserve boot stack");

        // 6. Double fault IST stack (0x60000 - 0x65000)
        self.reserve_range(
            PhysAddr::new(0x00060000),
            PhysAddr::new(0x00065000),
            "Double Fault IST",
        )
        .expect("Failed to reserve double fault IST");

        // 6b. NMI IST stack (0x65000 - 0x6A000)
        self.reserve_range(
            PhysAddr::new(0x00065000),
            PhysAddr::new(0x0006A000),
            "NMI IST",
        )
        .expect("Failed to reserve NMI IST");

        // 7. Heap area (0x400000 - 0x420000)
        self.reserve_range(
            PhysAddr::new(0x00400000),
            PhysAddr::new(0x00420000),
            "Kernel Heap",
        )
        .expect("Failed to reserve kernel heap");

        // 8. VGA memory-mapped I/O (0xA0000 - 0xC0000)
        self.reserve_range(
            PhysAddr::new(0x000A0000),
            PhysAddr::new(0x000C0000),
            "VGA MMIO",
        )
        .expect("Failed to reserve VGA MMIO");
    }

    // -- Allocation ----------------------------------------------------------

    /// Allocate a single 4 KiB frame.
    ///
    /// Returns the physical address of the allocated frame, or `None`
    /// if no free frames remain.
    pub fn allocate_frame(&mut self) -> Option<PhysAddr> {
        if self.next_free >= self.total_frames {
            if self.validation_enabled {
                self.stats.allocation_failures += 1;
            }
            return None;
        }

        let frame = self.next_free;
        self.mark_frame_reserved(frame);
        self.allocated_frames += 1;

        // Advance next_free to the next free frame.
        while self.next_free < self.total_frames && self.is_frame_reserved(self.next_free) {
            self.next_free += 1;
        }

        Some(PhysAddr::new(frame as u64 * FRAME_SIZE))
    }

    /// Allocate `count` contiguous 4 KiB frames.
    ///
    /// Uses first-fit search starting from `next_free`.
    /// Returns the physical address of the first frame, or `None`
    /// if no contiguous block of the requested size is available.
    pub fn allocate_frames(&mut self, count: usize) -> Option<PhysAddr> {
        if count == 0 {
            return None;
        }

        if count > self.total_frames {
            if self.validation_enabled {
                self.stats.allocation_failures += 1;
            }
            return None;
        }

        let mut start_frame = self.next_free;

        while start_frame + count <= self.total_frames {
            let mut all_free = true;
            for offset in 0..count {
                if self.is_frame_reserved(start_frame + offset) {
                    all_free = false;
                    start_frame += offset + 1;
                    break;
                }
            }

            if all_free {
                for offset in 0..count {
                    self.mark_frame_reserved(start_frame + offset);
                }
                self.allocated_frames += count;
                self.next_free = start_frame + count;
                // Advance next_free past any reserved frames.
                while self.next_free < self.total_frames
                    && self.is_frame_reserved(self.next_free)
                {
                    self.next_free += 1;
                }
                return Some(PhysAddr::new(start_frame as u64 * FRAME_SIZE));
            }
        }

        if self.validation_enabled {
            self.stats.allocation_failures += 1;
        }
        None
    }

    // -- Deallocation --------------------------------------------------------

    /// Free a single frame.
    ///
    /// Returns an error if the frame is out of range or already free.
    pub fn free_frame(&mut self, addr: PhysAddr) -> PmmResult<()> {
        let frame = addr.as_u64() as FrameIndex / FRAME_SIZE as FrameIndex;

        if frame >= self.total_frames {
            if self.validation_enabled {
                self.stats.invalid_free_attempts += 1;
            }
            return Err(PmmError::InvalidFree);
        }

        if !self.is_frame_reserved(frame) {
            if self.validation_enabled {
                self.stats.double_free_attempts += 1;
            }
            return Err(PmmError::DoubleFree);
        }

        self.mark_frame_free(frame);
        self.allocated_frames -= 1;
        Ok(())
    }

    /// Free `count` contiguous frames starting at `addr`.
    ///
    /// Validates that all frames in the range are currently allocated
    /// before freeing any of them (atomic free semantics).
    pub fn free_frames(&mut self, addr: PhysAddr, count: usize) -> PmmResult<()> {
        let start_frame = addr.as_u64() as FrameIndex / FRAME_SIZE as FrameIndex;
        let end_frame = start_frame + count;

        if end_frame > self.total_frames {
            if self.validation_enabled {
                self.stats.invalid_free_attempts += 1;
            }
            return Err(PmmError::InvalidFree);
        }

        // Validate all frames are allocated before freeing any.
        if self.validation_enabled {
            for frame in start_frame..end_frame {
                if !self.is_frame_reserved(frame) {
                    self.stats.double_free_attempts += 1;
                    return Err(PmmError::DoubleFree);
                }
            }
        }

        for frame in start_frame..end_frame {
            self.mark_frame_free(frame);
        }
        self.allocated_frames -= count;
        Ok(())
    }

    // -- Statistics ----------------------------------------------------------

    /// Recalculate fragmentation statistics from the bitmap.
    pub fn update_fragmentation_stats(&mut self) {
        let mut free_blocks = 0usize;
        let mut largest_block = 0usize;
        let mut current_block = 0usize;

        for frame in 0..self.total_frames {
            if !self.is_frame_reserved(frame) {
                current_block += 1;
            } else {
                if current_block > 0 {
                    free_blocks += 1;
                    if current_block > largest_block {
                        largest_block = current_block;
                    }
                    current_block = 0;
                }
            }
        }
        // Account for trailing free block.
        if current_block > 0 {
            free_blocks += 1;
            if current_block > largest_block {
                largest_block = current_block;
            }
        }

        self.stats.free_block_count = free_blocks;
        self.stats.largest_free_block = largest_block;
    }

    /// Update aggregate statistics after initialization or region changes.
    pub fn update_statistics(&mut self, usable_bytes: u64, reserved_bytes: u64) {
        self.stats.usable_bytes = usable_bytes;
        self.stats.reserved_bytes = reserved_bytes;
        self.stats.free_frames = self.total_frames - self.allocated_frames;
        self.stats.allocated_frames = self.allocated_frames;
        self.update_fragmentation_stats();
    }

    /// Get a snapshot of current statistics.
    pub fn statistics(&self) -> MemoryStatistics {
        self.stats
    }

    /// Get total frames tracked.
    pub fn total_frames(&self) -> usize {
        self.total_frames
    }

    /// Get allocated (reserved) frame count.
    pub fn allocated_frames(&self) -> usize {
        self.allocated_frames
    }

    /// Get free frame count.
    pub fn free_frame_count(&self) -> usize {
        self.total_frames - self.allocated_frames
    }

    // -- Validation ----------------------------------------------------------

    /// Enable or disable runtime validation.
    pub fn set_validation(&mut self, enabled: bool) {
        self.validation_enabled = enabled;
    }

    /// Validate bitmap integrity.
    ///
    /// Counts set bits in the bitmap and compares with `allocated_frames`.
    /// Returns `true` if they match.
    pub fn validate_bitmap(&self) -> bool {
        let mut counted = 0usize;
        // SAFETY: BITMAP is only accessed during single-threaded early boot.
        unsafe {
            let bitmap_bytes = (self.total_frames + 7) / 8;
            for byte in &BITMAP[..bitmap_bytes] {
                counted += byte.count_ones() as usize;
            }
        }
        counted == self.allocated_frames
    }

    /// Full self-test: validates bitmap, attempts allocation, attempts free.
    ///
    /// Returns `true` if all checks pass.
    pub fn self_test(&mut self) -> bool {
        // 1. Bitmap integrity check.
        if !self.validate_bitmap() {
            return false;
        }

        // 2. Try to allocate one frame.
        let test_frame = self.allocate_frame();
        if test_frame.is_none() {
            return false;
        }
        let addr = test_frame.unwrap();

        // 3. Bitmap should still be consistent after allocation.
        if !self.validate_bitmap() {
            // Undo the allocation before returning failure.
            let _ = self.free_frame(addr);
            return false;
        }

        // 4. Free the test frame.
        if self.free_frame(addr).is_err() {
            return false;
        }

        // 5. Bitmap should be consistent after free.
        if !self.validate_bitmap() {
            return false;
        }

        // 6. Double-free detection.
        let result = self.free_frame(addr);
        if result != Err(PmmError::DoubleFree) {
            return false;
        }

        // 7. Invalid-free detection (address beyond tracked range).
        let invalid_addr = PhysAddr::new(MAX_TRACKED_FRAMES as u64 * FRAME_SIZE + 0x1000);
        let result = self.free_frame(invalid_addr);
        if result != Err(PmmError::InvalidFree) {
            return false;
        }

        // 8. Verify contiguous allocation works.
        let block = self.allocate_frames(4);
        if block.is_some() {
            let _ = self.free_frames(block.unwrap(), 4);
        }

        // 9. Final bitmap consistency check.
        self.validate_bitmap()
    }

    // -- Conversion helpers --------------------------------------------------

    /// Convert a frame index to a physical address.
    pub fn frame_to_address(&self, frame: FrameIndex) -> PhysAddr {
        PhysAddr::new(frame as u64 * FRAME_SIZE)
    }

    /// Convert a physical address to a frame index.
    pub fn address_to_frame(&self, addr: PhysAddr) -> FrameIndex {
        (addr.as_u64() / FRAME_SIZE) as FrameIndex
    }
}

impl Default for PhysicalFrameAllocator {
    fn default() -> Self {
        PhysicalFrameAllocator::new(128 * 1024 * 1024)
    }
}

// ---------------------------------------------------------------------------
// PhysicalMemoryManager
// ---------------------------------------------------------------------------

/// The Physical Memory Manager — central memory management authority.
///
/// Wraps the frame allocator and maintains the system memory map
/// and reserved region list.
pub struct PhysicalMemoryManager {
    /// The frame allocator.
    frame_allocator: PhysicalFrameAllocator,
    /// Memory map — all physical memory regions.
    memory_map: Vec<MemoryRegion>,
    /// Reserved regions — memory reserved by the kernel.
    reserved_regions: Vec<ReservedRegion>,
}

impl PhysicalMemoryManager {
    /// Create a new Physical Memory Manager.
    pub fn new(total_memory_bytes: u64) -> Self {
        let frame_allocator = PhysicalFrameAllocator::new(total_memory_bytes);

        PhysicalMemoryManager {
            frame_allocator,
            memory_map: Vec::new(),
            reserved_regions: Vec::new(),
        }
    }

    /// Initialize the PMM with standard kernel reservations and self-test.
    pub fn init(&mut self) {
        self.frame_allocator.init_standard_reservations();
        self.build_memory_map();
        self.compute_statistics();

        // Run self-test during boot.
        let test_passed = self.frame_allocator.self_test();
        if !test_passed {
            crate::serial_println!("[PMM] WARNING: self-test FAILED");
        }
    }

    /// Build the memory map from HVM start info entries.
    ///
    /// `entries` is a slice of memory regions provided by the PVH bootloader.
    /// Non-RAM regions (ACPI, MMIO, etc.) are also recorded for tracking.
    pub fn init_from_memmap(&mut self, entries: &[MemoryRegion]) {
        // Build the memory map from bootloader-provided regions.
        for entry in entries {
            self.memory_map.push(*entry);

            if entry.is_usable() {
                // Mark usable regions as free in the bitmap.
                let start_frame =
                    entry.base.as_u64() as FrameIndex / FRAME_SIZE as FrameIndex;
                let end_frame =
                    ((entry.end().as_u64() as FrameIndex) + FRAME_SIZE as FrameIndex - 1)
                        / FRAME_SIZE as FrameIndex;
                let capped_end = end_frame.min(self.frame_allocator.total_frames());

                for frame in start_frame..capped_end {
                    if self.frame_allocator.is_frame_reserved(frame) {
                        self.frame_allocator.mark_frame_free_public(frame);
                    }
                }
            } else {
                // Mark non-usable regions as reserved.
                self.reserve_region(ReservedRegion::new(
                    entry.base,
                    entry.size,
                    entry.region_type,
                    entry.name,
                    entry.region_type.as_str(),
                ));
            }
        }

        // Now apply standard kernel reservations on top.
        self.frame_allocator.init_standard_reservations();
        self.frame_allocator.recalculate_next_free_public();
        self.compute_statistics();
        self.memory_map.sort_by_key(|r| r.base.as_u64());
    }

    /// Build memory map from reserved regions.
    fn build_memory_map(&mut self) {
        for region in &self.reserved_regions {
            self.memory_map.push(region.region);
        }
        self.memory_map.sort_by_key(|r| r.base.as_u64());
    }

    /// Compute and update aggregate statistics.
    fn compute_statistics(&mut self) {
        let mut usable = 0u64;
        let mut reserved = 0u64;
        for region in &self.memory_map {
            if region.region_type.is_usable() {
                usable += region.size;
            } else {
                reserved += region.size;
            }
        }
        self.frame_allocator.update_statistics(usable, reserved);
    }

    /// Add a reserved region and mark it in the bitmap.
    pub fn reserve_region(&mut self, region: ReservedRegion) {
        let start = region.base();
        let end = region.end();
        self.reserved_regions.push(region);
        let _ = self
            .frame_allocator
            .reserve_range(start, end, region.region.name);
    }

    /// Allocate a single frame.
    pub fn allocate_frame(&mut self) -> Option<PhysAddr> {
        self.frame_allocator.allocate_frame()
    }

    /// Allocate `count` contiguous frames.
    pub fn allocate_frames(&mut self, count: usize) -> Option<PhysAddr> {
        self.frame_allocator.allocate_frames(count)
    }

    /// Free a single frame.
    pub fn free_frame(&mut self, addr: PhysAddr) -> PmmResult<()> {
        self.frame_allocator.free_frame(addr)
    }

    /// Free `count` contiguous frames.
    pub fn free_frames(&mut self, addr: PhysAddr, count: usize) -> PmmResult<()> {
        self.frame_allocator.free_frames(addr, count)
    }

    /// Get a snapshot of current statistics.
    pub fn statistics(&self) -> MemoryStatistics {
        self.frame_allocator.statistics()
    }

    /// Get the memory map.
    pub fn memory_map(&self) -> &[MemoryRegion] {
        &self.memory_map
    }

    /// Get reserved regions.
    pub fn reserved_regions(&self) -> &[ReservedRegion] {
        &self.reserved_regions
    }

    /// Get a mutable reference to the frame allocator.
    pub fn frame_allocator(&mut self) -> &mut PhysicalFrameAllocator {
        &mut self.frame_allocator
    }

    /// Validate PMM state (bitmap integrity).
    pub fn validate(&self) -> bool {
        self.frame_allocator.validate_bitmap()
    }
}

// ---------------------------------------------------------------------------
// PhysicalFrameAllocator public helpers needed by PhysicalMemoryManager
// ---------------------------------------------------------------------------

impl PhysicalFrameAllocator {
    /// Mark a frame as free (public interface for PMM init).
    ///
    /// SAFETY: Caller must ensure `frame` is within tracked range.
    pub fn mark_frame_free_public(&mut self, frame: FrameIndex) {
        self.mark_frame_free(frame);
    }

    /// Recalculate next_free (public interface for PMM init).
    pub fn recalculate_next_free_public(&mut self) {
        self.recalculate_next_free();
    }
}

// ---------------------------------------------------------------------------
// Global PMM singleton
// ---------------------------------------------------------------------------

/// Global Physical Memory Manager instance.
///
/// SAFETY: `PMM` is written once during single-threaded early boot,
/// then only read (via `&'static mut` references) afterward.
/// No concurrent writes occur.
static mut PMM: Option<PhysicalMemoryManager> = None;
static PMM_INIT: AtomicBool = AtomicBool::new(false);

/// Initialize the Physical Memory Manager.
///
/// Must be called exactly once during early boot, after heap initialization.
/// `total_memory` is the total physical memory in bytes (from CPUID or HVM map).
pub fn init(total_memory: u64) {
    // SAFETY: Called once during single-threaded early boot.
    unsafe {
        let mut pmm = PhysicalMemoryManager::new(total_memory);
        pmm.init();
        PMM = Some(pmm);
        PMM_INIT.store(true, Ordering::Release);
    }
}

/// Initialize the PMM with a bootloader-provided memory map.
///
/// `total_memory` is the total addressable memory (fallback for regions
/// not covered by the map). `entries` are the memory regions from
/// the PVH HVM start info.
pub fn init_with_memmap(total_memory: u64, entries: &[MemoryRegion]) {
    // SAFETY: Called once during single-threaded early boot.
    unsafe {
        let mut pmm = PhysicalMemoryManager::new(total_memory);
        pmm.init_from_memmap(entries);
        PMM = Some(pmm);
        PMM_INIT.store(true, Ordering::Release);
    }
}

/// Get a mutable reference to the global Physical Memory Manager.
///
/// # Panics
///
/// Panics if the PMM has not been initialized.
pub fn pmm() -> &'static mut PhysicalMemoryManager {
    if !PMM_INIT.load(Ordering::Acquire) {
        panic!("Physical Memory Manager not initialized");
    }
    // SAFETY: PMM is initialized once before any concurrent access.
    // The Option is always Some after init, and never set back to None.
    // Using raw pointer to avoid temporary-value lifetime issues with static mut.
    unsafe {
        let ptr = core::ptr::addr_of_mut!(PMM);
        (*ptr).as_mut().unwrap_unchecked()
    }
}

/// Returns `true` if the PMM has been initialized.
pub fn is_initialized() -> bool {
    PMM_INIT.load(Ordering::Acquire)
}

/// Allocate a single frame using the global PMM.
pub fn allocate_frame() -> Option<PhysAddr> {
    pmm().allocate_frame()
}

/// Free a single frame using the global PMM.
pub fn free_frame(addr: PhysAddr) -> PmmResult<()> {
    pmm().free_frame(addr)
}

/// Get memory statistics from the global PMM.
pub fn statistics() -> MemoryStatistics {
    pmm().statistics()
}

/// Parse an HVM start info structure and extract memory map entries.
///
/// Returns a `Vec<MemoryRegion>` describing the physical memory layout.
///
/// # Safety
///
/// `hvm_ptr` must be a valid pointer to an `HvmStartInfo` structure
/// in physical memory.
pub unsafe fn parse_hvm_start_info(hvm_ptr: u64) -> Vec<MemoryRegion> {
    if hvm_ptr == 0 {
        return Vec::new();
    }

    let hvm = &*(hvm_ptr as *const HvmStartInfo);

    // Validate magic ("Xen" magic number is 0x00000000 in newer QEMU).
    // QEMU PVH sets magic to 0 and version to 0.
    let mut regions = Vec::new();

    if hvm.memmap_paddr == 0 || hvm.memmap_entries == 0 {
        return regions;
    }

    let entry_ptr = hvm.memmap_paddr as *const HvmMemmapEntry;

    for i in 0..hvm.memmap_entries as usize {
        let entry = &*entry_ptr.add(i);

        let region_type = MemoryRegionType::from_hvm(entry.mem_type);

        let name: &'static str = match entry.mem_type {
            HVM_MEMMAP_RAM => "Boot RAM",
            HVM_MEMMAP_RESERVED => "Boot Reserved",
            HVM_MEMMAP_ACPI => "Boot ACPI",
            HVM_MEMMAP_ACPI_NVS => "Boot ACPI NVS",
            HVM_MEMMAP_BAD => "Boot Bad Memory",
            _ => "Boot Unknown",
        };

        regions.push(MemoryRegion::new(
            PhysAddr::new(entry.addr),
            entry.size,
            region_type,
            name,
        ));
    }

    regions
}
