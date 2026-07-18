//! Virtual Memory Manager
//!
//! Production-quality virtual memory management for Arcadia OS.
//! Uses x86_64 4-level paging (PML4 → PDP → PD → PT) with the `OffsetPageTable`
//! mapper from the `x86_64` crate.
//!
//! ## Architecture
//!
//! The VMM wraps the x86_64 crate's `OffsetPageTable` with identity-mapped physical
//! memory access (`phys_offset = 0`). It takes over management of the page tables
//! established during boot by `boot64.asm`.
//!
//! ## Current Boot Page Table Layout
//!
//! `boot64.asm` creates 3 × 2 MiB large pages covering 0–6 MiB:
//!
//! | PML4[0] | PDP[0] | PD[0] = 0–2 MiB  (large page)
//! |         |        | PD[1] = 2–4 MiB  (large page, covers VGA)
//! |         |        | PD[2] = 4–6 MiB  (large page, covers heap)
//!
//! ## Public API
//!
//! | Method            | Description |
//! |-------------------|-------------|
//! | `map_page`        | Map a single 4 KiB page |
//! | `unmap_page`      | Unmap a single 4 KiB page |
//! | `remap_page`      | Change the physical backing of a virtual page |
//! | `translate`       | Virtual → physical address translation |
//! | `map_region`      | Map a contiguous range of 4 KiB pages |
//! | `unmap_region`    | Unmap a contiguous range of 4 KiB pages |
//! | `identity_map`    | Map phys → same virt (identity) |
//! | `flush_tlb`       | Full TLB flush (CR3 reload) |
//! | `flush_tlb_page`  | Single-page TLB invalidation (invlpg) |
//! | `validate`        | Walk page tables and verify consistency |
//! | `statistics`      | Return snapshot of VMM statistics |

extern crate alloc;

use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use x86_64::registers::control::Cr3;
use x86_64::structures::paging::mapper::{MapToError, TranslateResult, UnmapError};
use x86_64::structures::paging::{FrameAllocator, Mapper, OffsetPageTable, Page, PageSize, PageTable, PageTableFlags, PhysFrame, Size4KiB, Translate};
use x86_64::{PhysAddr, VirtAddr};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Size of a single 4 KiB page in bytes.
pub const PAGE_SIZE: u64 = 4096;

// ---------------------------------------------------------------------------
// VmmError
// ---------------------------------------------------------------------------

/// Error types for virtual memory operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmmError {
    /// VMM has not been initialized.
    NotInitialized,
    /// The page at the given virtual address is already mapped.
    AlreadyMapped,
    /// The page at the given virtual address is not mapped.
    NotMapped,
    /// No physical memory available for page table allocation.
    OutOfMemory,
    /// The address is not page-aligned or is out of the canonical address range.
    InvalidAddress,
    /// The underlying page table mapper returned an error.
    MapFailed,
    /// The underlying page table unmapper returned an error.
    UnmapFailed,
    /// Parent page table entry is a huge/large page; sub-page operation invalid.
    ParentEntryHugePage,
}

impl core::fmt::Display for VmmError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            VmmError::NotInitialized => write!(f, "VMM not initialized"),
            VmmError::AlreadyMapped => write!(f, "Page already mapped"),
            VmmError::NotMapped => write!(f, "Page not mapped"),
            VmmError::OutOfMemory => write!(f, "Out of memory for page tables"),
            VmmError::InvalidAddress => write!(f, "Invalid address (not page-aligned or out of range)"),
            VmmError::MapFailed => write!(f, "Page table mapping failed"),
            VmmError::UnmapFailed => write!(f, "Page table unmapping failed"),
            VmmError::ParentEntryHugePage => write!(f, "Parent entry is a huge page"),
        }
    }
}

/// Result type for virtual memory operations.
pub type VmmResult<T> = Result<T, VmmError>;

// ---------------------------------------------------------------------------
// VmmStatistics
// ---------------------------------------------------------------------------

/// Comprehensive statistics for the virtual memory manager.
#[derive(Debug, Clone, Copy)]
pub struct VmmStatistics {
    /// Total number of 4 KiB pages currently mapped.
    pub total_mapped_pages: usize,
    /// Total bytes currently mapped.
    pub total_mapped_bytes: u64,
    /// Number of map operations performed since init.
    pub map_count: u64,
    /// Number of unmap operations performed since init.
    pub unmap_count: u64,
    /// Number of remap operations performed since init.
    pub remap_count: u64,
    /// Number of TLB flush operations performed since init.
    pub tlb_flush_count: u64,
    /// Number of page table frames currently allocated.
    pub page_table_frames: usize,
}

impl VmmStatistics {
    /// Create a new zeroed statistics structure.
    const fn new() -> Self {
        VmmStatistics {
            total_mapped_pages: 0,
            total_mapped_bytes: 0,
            map_count: 0,
            unmap_count: 0,
            remap_count: 0,
            tlb_flush_count: 0,
            page_table_frames: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// MappedRegion
// ---------------------------------------------------------------------------

/// Describes a single virtual memory mapping tracked by the VMM.
#[derive(Debug, Clone, Copy)]
pub struct MappedRegion {
    /// Start of the virtual address range.
    pub virtual_addr: VirtAddr,
    /// Start of the physical address range.
    pub physical_addr: PhysAddr,
    /// Size of the mapping in bytes.
    pub size: u64,
    /// Page table flags for this mapping.
    pub flags: PageTableFlags,
    /// Human-readable description for debugging.
    pub description: &'static str,
}

// ---------------------------------------------------------------------------
// PmmFrameAllocator
// ---------------------------------------------------------------------------

/// Wraps the global Physical Memory Manager as a `FrameAllocator<Size4KiB>`
/// for use with the x86_64 crate's page table mapper.
///
/// # Safety
///
/// The x86_64 `FrameAllocator` trait is `unsafe` because the implementor must
/// guarantee that each frame is returned only once. The PMM guarantees this
/// through its bitmap.
pub struct PmmFrameAllocator;

unsafe impl FrameAllocator<Size4KiB> for PmmFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        crate::memory::physmem::allocate_frame().map(PhysFrame::containing_address)
    }
}

// ---------------------------------------------------------------------------
// VirtualMemoryManager
// ---------------------------------------------------------------------------

/// The core Virtual Memory Manager.
///
/// Owns an `OffsetPageTable` that manages the system page tables through
/// identity-mapped physical memory access. Tracks all mappings for
/// validation and statistics.
pub struct VirtualMemoryManager {
    /// The x86_64 page table mapper (identity-mapped, phys_offset = 0).
    page_table: OffsetPageTable<'static>,
    /// All tracked virtual memory mappings.
    mapped_regions: Vec<MappedRegion>,
    /// Runtime statistics.
    stats: VmmStatistics,
    /// Whether runtime validation is enabled.
    validation_enabled: bool,
}

impl VirtualMemoryManager {
    /// Initialize the VMM by taking over the existing page tables.
    ///
    /// Reads CR3 to locate the PML4, wraps it in an `OffsetPageTable` with
    /// identity mapping (physical address = virtual address), and discovers
    /// all existing mappings from `boot64.asm`.
    pub fn init() -> Self {
        // Read CR3 to get the PML4 physical frame.
        let (pml4_frame, _cr3_flags) = Cr3::read();
        let pml4_phys = pml4_frame.start_address();

        // Create OffsetPageTable with phys_offset = 0 (identity mapping).
        // SAFETY: The PML4 at pml4_phys is a valid level-4 page table
        // established by boot64.asm. Identity mapping is active, so
        // physical addresses are directly accessible. The reference is
        // transmuted to 'static because the page tables live at fixed
        // physical addresses for the entire program lifetime.
        let page_table = unsafe {
            let pml4_ptr = pml4_phys.as_u64() as *mut PageTable;
            let pml4_ref = &mut *pml4_ptr;
            let pml4_static: &'static mut PageTable = core::mem::transmute(pml4_ref);
            OffsetPageTable::new(pml4_static, VirtAddr::new(0))
        };

        let mut vmm = VirtualMemoryManager {
            page_table,
            mapped_regions: Vec::new(),
            stats: VmmStatistics::new(),
            validation_enabled: true,
        };

        // Discover and track all mappings established by boot64.asm.
        vmm.discover_existing_mappings();

        vmm
    }

    /// Walk the existing page tables and record all currently-mapped pages.
    ///
    /// This is called once during `init()` to populate `mapped_regions`
    /// with the identity mappings created by `boot64.asm`.
    fn discover_existing_mappings(&mut self) {
        let pml4 = self.page_table.level_4_table();

        for (p4_idx, p4_entry) in pml4.iter().enumerate() {
            if !p4_entry.flags().contains(PageTableFlags::PRESENT) {
                continue;
            }

            let pdp_phys = p4_entry.addr();
            // SAFETY: We are reading a valid page table entry that has PRESENT set.
            // Identity mapping is active, so we can access it directly.
            let pdp = unsafe { &*(pdp_phys.as_u64() as *const PageTable) };

            for (p3_idx, p3_entry) in pdp.iter().enumerate() {
                if !p3_entry.flags().contains(PageTableFlags::PRESENT) {
                    continue;
                }

                // 1 GiB large page
                if p3_entry.flags().contains(PageTableFlags::HUGE_PAGE) {
                    let virt = VirtAddr::new(
                        ((p4_idx as u64) << 39) | ((p3_idx as u64) << 30),
                    );
                    let phys = p3_entry.addr();
                    let flags = p3_entry.flags();
                    self.mapped_regions.push(MappedRegion {
                        virtual_addr: virt,
                        physical_addr: phys,
                        size: Size4KiB::SIZE * 512 * 512, // 1 GiB
                        flags,
                        description: "boot 1 GiB page",
                    });
                    self.stats.total_mapped_pages += 512 * 512;
                    continue;
                }

                let pd_phys = p3_entry.addr();
                let pd = unsafe { &*(pd_phys.as_u64() as *const PageTable) };

                for (p2_idx, p2_entry) in pd.iter().enumerate() {
                    if !p2_entry.flags().contains(PageTableFlags::PRESENT) {
                        continue;
                    }

                    // 2 MiB large page
                    if p2_entry.flags().contains(PageTableFlags::HUGE_PAGE) {
                        let virt = VirtAddr::new(
                            ((p4_idx as u64) << 39)
                                | ((p3_idx as u64) << 30)
                                | ((p2_idx as u64) << 21),
                        );
                        let phys = p2_entry.addr();
                        let flags = p2_entry.flags();
                        self.mapped_regions.push(MappedRegion {
                            virtual_addr: virt,
                            physical_addr: phys,
                            size: Size4KiB::SIZE * 512, // 2 MiB
                            flags,
                            description: "boot 2 MiB page",
                        });
                        self.stats.total_mapped_pages += 512;
                        continue;
                    }

                    // 4 KiB page table
                    let pt_phys = p2_entry.addr();
                    let pt = unsafe { &*(pt_phys.as_u64() as *const PageTable) };

                    for (p1_idx, p1_entry) in pt.iter().enumerate() {
                        if !p1_entry.flags().contains(PageTableFlags::PRESENT) {
                            continue;
                        }

                        let virt = VirtAddr::new(
                            ((p4_idx as u64) << 39)
                                | ((p3_idx as u64) << 30)
                                | ((p2_idx as u64) << 21)
                                | ((p1_idx as u64) << 12),
                        );
                        let phys = p1_entry.addr();
                        let flags = p1_entry.flags();
                        self.mapped_regions.push(MappedRegion {
                            virtual_addr: virt,
                            physical_addr: phys,
                            size: PAGE_SIZE,
                            flags,
                            description: "boot 4 KiB page",
                        });
                        self.stats.total_mapped_pages += 1;
                    }
                }
            }
        }

        self.stats.total_mapped_bytes =
            self.stats.total_mapped_pages as u64 * PAGE_SIZE;
    }

    // -- Core mapping operations ---------------------------------------------

    /// Map a single 4 KiB page.
    ///
    /// Maps virtual address `virt` to physical address `phys` with the given
    /// page table flags. Allocates intermediate page table frames from the PMM
    /// as needed.
    ///
    /// # Arguments
    ///
    /// * `virt` — Virtual address (must be 4 KiB page-aligned).
    /// * `phys` — Physical address (must be 4 KiB page-aligned).
    /// * `flags` — Page table flags (PRESENT is implied).
    ///
    /// # Errors
    ///
    /// Returns `VmmError::InvalidAddress` if addresses are not page-aligned.
    /// Returns `VmmError::AlreadyMapped` if the virtual page is already mapped.
    /// Returns `VmmError::OutOfMemory` if page table allocation fails.
    pub fn map_page(
        &mut self,
        virt: VirtAddr,
        phys: PhysAddr,
        flags: PageTableFlags,
    ) -> VmmResult<()> {
        if !virt.is_aligned(PAGE_SIZE) || !phys.is_aligned(PAGE_SIZE) {
            return Err(VmmError::InvalidAddress);
        }

        let page = Page::<Size4KiB>::containing_address(virt);
        let frame = PhysFrame::<Size4KiB>::containing_address(phys);
        let mut frame_allocator = PmmFrameAllocator;

        // SAFETY: We ensure that:
        // 1. The caller provides valid, page-aligned addresses.
        // 2. Each physical frame is only mapped to one virtual address
        //    (the caller must ensure no aliasing).
        // 3. The mapping is created with correct flags.
        let flush = unsafe {
            self.page_table
                .map_to(page, frame, flags, &mut frame_allocator)
        }
        .map_err(|e| match e {
            MapToError::FrameAllocationFailed => VmmError::OutOfMemory,
            MapToError::ParentEntryHugePage => VmmError::ParentEntryHugePage,
            MapToError::PageAlreadyMapped(_) => VmmError::AlreadyMapped,
        })?;

        flush.flush();

        self.mapped_regions.push(MappedRegion {
            virtual_addr: virt,
            physical_addr: phys,
            size: PAGE_SIZE,
            flags,
            description: "user mapped",
        });

        self.stats.total_mapped_pages += 1;
        self.stats.total_mapped_bytes += PAGE_SIZE;
        self.stats.map_count += 1;
        self.stats.tlb_flush_count += 1;

        Ok(())
    }

    /// Unmap a single 4 KiB page.
    ///
    /// Removes the mapping for virtual address `virt` and returns the
    /// physical address that was mapped.
    ///
    /// # Arguments
    ///
    /// * `virt` — Virtual address (must be 4 KiB page-aligned).
    ///
    /// # Returns
    ///
    /// The physical address that was previously mapped at `virt`.
    ///
    /// # Errors
    ///
    /// Returns `VmmError::NotMapped` if no mapping exists at `virt`.
    /// Returns `VmmError::ParentEntryHugePage` if the mapping uses a large page.
    pub fn unmap_page(&mut self, virt: VirtAddr) -> VmmResult<PhysAddr> {
        if !virt.is_aligned(PAGE_SIZE) {
            return Err(VmmError::InvalidAddress);
        }

        let page = Page::<Size4KiB>::containing_address(virt);

        let (frame, flush) = self.page_table.unmap(page).map_err(|e| match e {
            UnmapError::PageNotMapped => VmmError::NotMapped,
            UnmapError::ParentEntryHugePage => VmmError::ParentEntryHugePage,
            UnmapError::InvalidFrameAddress(_) => VmmError::UnmapFailed,
        })?;

        flush.flush();

        let phys = frame.start_address();

        // Remove from tracked regions.
        self.mapped_regions.retain(|r| {
            !(virt.as_u64() >= r.virtual_addr.as_u64()
                && virt.as_u64() < r.virtual_addr.as_u64() + r.size)
        });

        self.stats.total_mapped_pages = self.stats.total_mapped_pages.saturating_sub(1);
        self.stats.total_mapped_bytes = self
            .stats
            .total_mapped_bytes
            .saturating_sub(PAGE_SIZE);
        self.stats.unmap_count += 1;
        self.stats.tlb_flush_count += 1;

        Ok(phys)
    }

    /// Remap a virtual page to a new physical address.
    ///
    /// Unmaps the existing mapping at `virt`, then maps `virt` to `new_phys`
    /// with the given flags. Returns the old physical address.
    ///
    /// # Arguments
    ///
    /// * `virt` — Virtual address (must be 4 KiB page-aligned).
    /// * `new_phys` — New physical address (must be 4 KiB page-aligned).
    /// * `flags` — New page table flags.
    ///
    /// # Returns
    ///
    /// The old physical address that was previously mapped at `virt`.
    pub fn remap_page(
        &mut self,
        virt: VirtAddr,
        new_phys: PhysAddr,
        flags: PageTableFlags,
    ) -> VmmResult<PhysAddr> {
        let old_phys = self.unmap_page(virt)?;
        self.map_page(virt, new_phys, flags)?;
        self.stats.remap_count += 1;
        Ok(old_phys)
    }

    /// Translate a virtual address to its physical address.
    ///
    /// Walks the page tables to find the physical address mapped at `virt`.
    /// Works with all page sizes (4 KiB, 2 MiB, 1 GiB).
    ///
    /// # Arguments
    ///
    /// * `virt` — Virtual address to translate.
    ///
    /// # Returns
    ///
    /// `Some(phys)` if the address is mapped, `None` otherwise.
    pub fn translate(&self, virt: VirtAddr) -> Option<PhysAddr> {
        self.page_table.translate_addr(virt)
    }

    // -- Bulk mapping operations ---------------------------------------------

    /// Map a contiguous range of 4 KiB pages.
    ///
    /// Maps `count` pages starting at `virt` to physical addresses starting
    /// at `phys`, each with the given flags.
    ///
    /// # Arguments
    ///
    /// * `virt` — Start virtual address (must be 4 KiB page-aligned).
    /// * `phys` — Start physical address (must be 4 KiB page-aligned).
    /// * `count` — Number of 4 KiB pages to map.
    /// * `flags` — Page table flags for each page.
    pub fn map_region(
        &mut self,
        virt: VirtAddr,
        phys: PhysAddr,
        count: usize,
        flags: PageTableFlags,
    ) -> VmmResult<()> {
        if !virt.is_aligned(PAGE_SIZE) || !phys.is_aligned(PAGE_SIZE) {
            return Err(VmmError::InvalidAddress);
        }

        for i in 0..count {
            let page_virt = VirtAddr::new(virt.as_u64() + (i as u64) * PAGE_SIZE);
            let page_phys = PhysAddr::new(phys.as_u64() + (i as u64) * PAGE_SIZE);
            self.map_page(page_virt, page_phys, flags)?;
        }

        Ok(())
    }

    /// Unmap a contiguous range of 4 KiB pages.
    ///
    /// Unmaps `count` pages starting at `virt`.
    ///
    /// # Arguments
    ///
    /// * `virt` — Start virtual address (must be 4 KiB page-aligned).
    /// * `count` — Number of 4 KiB pages to unmap.
    pub fn unmap_region(&mut self, virt: VirtAddr, count: usize) -> VmmResult<()> {
        if !virt.is_aligned(PAGE_SIZE) {
            return Err(VmmError::InvalidAddress);
        }

        for i in 0..count {
            let page_virt = VirtAddr::new(virt.as_u64() + (i as u64) * PAGE_SIZE);
            let _ = self.unmap_page(page_virt);
        }

        Ok(())
    }

    /// Identity-map a range of physical memory.
    ///
    /// Maps each 4 KiB frame in the range `[phys, phys + count * PAGE_SIZE)`
    /// to the same virtual address (identity mapping).
    ///
    /// Pages that are already mapped are silently skipped.
    ///
    /// # Arguments
    ///
    /// * `phys` — Start physical address (must be 4 KiB page-aligned).
    /// * `count` — Number of 4 KiB pages to identity-map.
    /// * `flags` — Page table flags for each page.
    pub fn identity_map(
        &mut self,
        phys: PhysAddr,
        count: usize,
        flags: PageTableFlags,
    ) -> VmmResult<()> {
        if !phys.is_aligned(PAGE_SIZE) {
            return Err(VmmError::InvalidAddress);
        }

        for i in 0..count {
            let frame_phys = PhysAddr::new(phys.as_u64() + (i as u64) * PAGE_SIZE);
            let virt = VirtAddr::new(frame_phys.as_u64());

            // Skip if already mapped.
            if self.page_table.translate_addr(virt).is_some() {
                continue;
            }

            let frame = PhysFrame::<Size4KiB>::containing_address(frame_phys);
            let page = Page::<Size4KiB>::containing_address(virt);
            let mut frame_allocator = PmmFrameAllocator;

            // SAFETY: Identity mapping preserves physical = virtual, so no aliasing
            // issues. We skip already-mapped pages.
            match unsafe {
                self.page_table
                    .map_to(page, frame, flags, &mut frame_allocator)
            } {
                Ok(flush) => {
                    flush.flush();
                    self.mapped_regions.push(MappedRegion {
                        virtual_addr: virt,
                        physical_addr: frame_phys,
                        size: PAGE_SIZE,
                        flags,
                        description: "identity map",
                    });
                    self.stats.total_mapped_pages += 1;
                    self.stats.total_mapped_bytes += PAGE_SIZE;
                    self.stats.map_count += 1;
                    self.stats.tlb_flush_count += 1;
                }
                Err(MapToError::PageAlreadyMapped(_)) => {
                    // Already mapped — skip silently.
                }
                Err(MapToError::FrameAllocationFailed) => {
                    return Err(VmmError::OutOfMemory);
                }
                Err(MapToError::ParentEntryHugePage) => {
                    return Err(VmmError::ParentEntryHugePage);
                }
            }
        }

        Ok(())
    }

    // -- TLB management ------------------------------------------------------

    /// Flush the entire TLB by reloading CR3.
    ///
    /// This is the most expensive TLB invalidation. Use `flush_tlb_page`
    /// for single-page invalidations when possible.
    pub fn flush_tlb(&mut self) {
        x86_64::instructions::tlb::flush_all();
        self.stats.tlb_flush_count += 1;
    }

    /// Invalidate the TLB entry for a single virtual page.
    ///
    /// Uses the `invlpg` instruction for efficient single-page invalidation.
    ///
    /// # Arguments
    ///
    /// * `virt` — Virtual address of the page to invalidate.
    pub fn flush_tlb_page(&mut self, virt: VirtAddr) {
        x86_64::instructions::tlb::flush(virt);
        self.stats.tlb_flush_count += 1;
    }

    // -- Validation ----------------------------------------------------------

    /// Enable or disable runtime validation.
    pub fn set_validation(&mut self, enabled: bool) {
        self.validation_enabled = enabled;
    }

    /// Validate VMM consistency by walking the page tables.
    ///
    /// Walks all four levels of the page tables and counts mapped pages.
    /// Returns `true` if the walked count matches the tracked statistics.
    ///
    /// This is an O(n) operation where n is the total number of page table
    /// entries across all levels.
    pub fn validate(&self) -> bool {
        let walked_count = self.count_mapped_pages();
        walked_count == self.stats.total_mapped_pages
    }

    /// Walk the page tables and count all mapped pages (4 KiB + large pages).
    fn count_mapped_pages(&self) -> usize {
        let mut count = 0usize;
        let pml4 = self.page_table.level_4_table();

        for (_p4_idx, p4_entry) in pml4.iter().enumerate() {
            if !p4_entry.flags().contains(PageTableFlags::PRESENT) {
                continue;
            }

            let pdp = unsafe { &*(p4_entry.addr().as_u64() as *const PageTable) };

            for (_p3_idx, p3_entry) in pdp.iter().enumerate() {
                if !p3_entry.flags().contains(PageTableFlags::PRESENT) {
                    continue;
                }

                if p3_entry.flags().contains(PageTableFlags::HUGE_PAGE) {
                    count += 512 * 512; // 1 GiB = 512 × 512 × 4 KiB pages
                    continue;
                }

                let pd = unsafe { &*(p3_entry.addr().as_u64() as *const PageTable) };

                for (_p2_idx, p2_entry) in pd.iter().enumerate() {
                    if !p2_entry.flags().contains(PageTableFlags::PRESENT) {
                        continue;
                    }

                    if p2_entry.flags().contains(PageTableFlags::HUGE_PAGE) {
                        count += 512; // 2 MiB = 512 × 4 KiB pages
                        continue;
                    }

                    let pt = unsafe { &*(p2_entry.addr().as_u64() as *const PageTable) };

                    for (_p1_idx, p1_entry) in pt.iter().enumerate() {
                        if p1_entry.flags().contains(PageTableFlags::PRESENT) {
                            count += 1;
                        }
                    }
                }
            }
        }

        count
    }

    // -- Statistics ----------------------------------------------------------

    /// Get a snapshot of current VMM statistics.
    pub fn statistics(&self) -> VmmStatistics {
        self.stats
    }

    /// Get the total number of tracked mapped regions.
    pub fn region_count(&self) -> usize {
        self.mapped_regions.len()
    }

    /// Get a reference to all tracked mapped regions.
    pub fn mapped_regions(&self) -> &[MappedRegion] {
        &self.mapped_regions
    }

    // -- Query operations ----------------------------------------------------

    /// Check if a virtual address is currently mapped.
    pub fn is_mapped(&self, virt: VirtAddr) -> bool {
        self.page_table.translate_addr(virt).is_some()
    }

    /// Get the mapping info for a virtual address, if any.
    ///
    /// Searches the tracked `mapped_regions` for a region containing `virt`.
    pub fn get_mapping(&self, virt: VirtAddr) -> Option<&MappedRegion> {
        self.mapped_regions.iter().find(|r| {
            virt.as_u64() >= r.virtual_addr.as_u64()
                && virt.as_u64() < r.virtual_addr.as_u64() + r.size
        })
    }

    /// Get the page table flags for a virtual address, if mapped.
    pub fn get_flags(&self, virt: VirtAddr) -> Option<PageTableFlags> {
        match self.page_table.translate(virt) {
            TranslateResult::Mapped { flags, .. } => Some(flags),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Global VMM singleton
// ---------------------------------------------------------------------------

/// Global Virtual Memory Manager instance.
///
/// SAFETY: `VMM` is written once during single-threaded early boot,
/// then only read (via `&'static mut` references) afterward.
/// No concurrent writes occur.
static mut VMM: Option<VirtualMemoryManager> = None;
static VMM_INIT: AtomicBool = AtomicBool::new(false);

/// Initialize the Virtual Memory Manager.
///
/// Must be called exactly once during early boot, after PMM initialization.
/// Takes over the existing page tables and discovers all boot mappings.
pub fn init() {
    // SAFETY: Called once during single-threaded early boot.
    unsafe {
        let vmm = VirtualMemoryManager::init();
        VMM = Some(vmm);
        VMM_INIT.store(true, Ordering::Release);
    }
}

/// Get a mutable reference to the global Virtual Memory Manager.
///
/// # Panics
///
/// Panics if the VMM has not been initialized.
pub fn vmm() -> &'static mut VirtualMemoryManager {
    if !VMM_INIT.load(Ordering::Acquire) {
        panic!("Virtual Memory Manager not initialized");
    }
    // SAFETY: VMM is initialized once before any concurrent access.
    // The Option is always Some after init, and never set back to None.
    unsafe {
        let ptr = core::ptr::addr_of_mut!(VMM);
        (*ptr).as_mut().unwrap_unchecked()
    }
}

/// Returns `true` if the VMM has been initialized.
pub fn is_initialized() -> bool {
    VMM_INIT.load(Ordering::Acquire)
}

// ---------------------------------------------------------------------------
// Convenience free functions (global API)
// ---------------------------------------------------------------------------

/// Translate a virtual address using the global VMM.
pub fn translate(virt: VirtAddr) -> Option<PhysAddr> {
    vmm().translate(virt)
}

/// Get VMM statistics from the global VMM.
pub fn statistics() -> VmmStatistics {
    vmm().statistics()
}

/// Validate the global VMM consistency.
pub fn validate() -> bool {
    vmm().validate()
}
