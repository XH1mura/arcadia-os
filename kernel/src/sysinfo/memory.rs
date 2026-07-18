//! Memory Detection Module
//!
//! Provides comprehensive memory information detection.
//! Detects total RAM, usable RAM, reserved memory regions, and memory map.

use core::arch::x86_64::__cpuid;
use x86_64::PhysAddr;

/// Memory region types for the memory map
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MemoryRegionType {
    /// Usable RAM
    Usable = 1,
    /// Reserved (not usable)
    Reserved = 2,
    /// ACPI tables
    Acpi = 3,
    /// ACPI Non-volatile storage
    AcpiNvs = 4,
    /// Bad memory
    Bad = 5,
    /// Bootloader reclaimable
    BootloaderReclaimable = 6,
    /// Kernel and modules
    Kernel = 7,
    /// Framebuffer
    Framebuffer = 8,
    /// Unknown type
    Unknown = 0,
}

impl MemoryRegionType {
    /// Convert from u32 to MemoryRegionType
    pub fn from_u32(t: u32) -> Self {
        match t {
            1 => MemoryRegionType::Usable,
            2 => MemoryRegionType::Reserved,
            3 => MemoryRegionType::Acpi,
            4 => MemoryRegionType::AcpiNvs,
            5 => MemoryRegionType::Bad,
            6 => MemoryRegionType::BootloaderReclaimable,
            7 => MemoryRegionType::Kernel,
            8 => MemoryRegionType::Framebuffer,
            _ => MemoryRegionType::Unknown,
        }
    }

    /// Get the type as a string
    pub fn as_str(&self) -> &'static str {
        match self {
            MemoryRegionType::Usable => "Usable RAM",
            MemoryRegionType::Reserved => "Reserved",
            MemoryRegionType::Acpi => "ACPI Tables",
            MemoryRegionType::AcpiNvs => "ACPI NVS",
            MemoryRegionType::Bad => "Bad Memory",
            MemoryRegionType::BootloaderReclaimable => "Bootloader Reclaimable",
            MemoryRegionType::Kernel => "Kernel",
            MemoryRegionType::Framebuffer => "Framebuffer",
            MemoryRegionType::Unknown => "Unknown",
        }
    }
}

/// A single memory region in the system memory map
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryRegion {
    /// Base physical address
    pub base: PhysAddr,
    /// Size in bytes
    pub size: u64,
    /// Memory region type
    pub region_type: MemoryRegionType,
    /// Whether this region is usable for the OS
    pub is_usable: bool,
}

impl MemoryRegion {
    /// Create a new memory region
    pub fn new(base: PhysAddr, size: u64, region_type: MemoryRegionType) -> Self {
        MemoryRegion {
            base,
            size,
            region_type,
            is_usable: matches!(
                region_type,
                MemoryRegionType::Usable | MemoryRegionType::BootloaderReclaimable
            ),
        }
    }

    /// Get the end address (base + size)
    pub fn end(&self) -> PhysAddr {
        PhysAddr::new(self.base.as_u64().checked_add(self.size).unwrap_or(0))
    }

    /// Format the region for display
    pub fn format(&self) -> alloc::string::String {
        alloc::format!(
            "0x{:016X} - 0x{:016X} ({} MiB) {}",
            self.base.as_u64(),
            self.end().as_u64(),
            self.size / (1024 * 1024),
            self.region_type.as_str()
        )
    }
}

/// Complete memory information structure
#[derive(Debug, Clone)]
pub struct MemoryInfo {
    /// Total physical memory in bytes
    pub total_bytes: u64,
    /// Total usable memory in bytes
    pub usable_bytes: u64,
    /// Total reserved memory in bytes
    pub reserved_bytes: u64,
    /// Memory map (all detected regions)
    pub memory_map: alloc::vec::Vec<MemoryRegion>,
    /// Firmware-provided memory regions
    pub firmware_regions: alloc::vec::Vec<MemoryRegion>,
    /// Usable memory regions
    pub usable_regions: alloc::vec::Vec<MemoryRegion>,
    /// Reserved memory regions
    pub reserved_regions: alloc::vec::Vec<MemoryRegion>,
}

impl MemoryInfo {
    /// Create a new MemoryInfo structure
    pub fn new() -> Self {
        let mut memory_map = alloc::vec::Vec::new();

        // For now, we'll use the e820 memory map detection
        // But we need to implement it properly

        // Initialize with basic information
        let total = detect_total_memory();

        // Create a basic memory map
        // This will be populated by the e820 detection later
        memory_map.push(MemoryRegion::new(
            PhysAddr::new(0),
            0x100000, // 1 MiB
            MemoryRegionType::Reserved,
        ));

        memory_map.push(MemoryRegion::new(
            PhysAddr::new(0x100000),
            total - 0x100000,
            MemoryRegionType::Usable,
        ));

        // Filter regions
        let usable_regions: alloc::vec::Vec<MemoryRegion> =
            memory_map.iter().filter(|r| r.is_usable).cloned().collect();

        let reserved_regions: alloc::vec::Vec<MemoryRegion> = memory_map
            .iter()
            .filter(|r| !r.is_usable)
            .cloned()
            .collect();

        let usable_bytes: u64 = usable_regions.iter().map(|r| r.size).sum();
        let reserved_bytes: u64 = reserved_regions.iter().map(|r| r.size).sum();

        MemoryInfo {
            total_bytes: total,
            usable_bytes,
            reserved_bytes,
            memory_map,
            firmware_regions: alloc::vec::Vec::new(),
            usable_regions,
            reserved_regions,
        }
    }

    /// Create with a custom memory map (for use with e820 or other detection methods)
    pub fn with_memory_map(memory_map: alloc::vec::Vec<MemoryRegion>) -> Self {
        let usable_regions: alloc::vec::Vec<MemoryRegion> =
            memory_map.iter().filter(|r| r.is_usable).cloned().collect();

        let reserved_regions: alloc::vec::Vec<MemoryRegion> = memory_map
            .iter()
            .filter(|r| !r.is_usable)
            .cloned()
            .collect();

        let total_bytes: u64 = memory_map.iter().map(|r| r.size).sum();
        let usable_bytes: u64 = usable_regions.iter().map(|r| r.size).sum();
        let reserved_bytes: u64 = reserved_regions.iter().map(|r| r.size).sum();

        MemoryInfo {
            total_bytes,
            usable_bytes,
            reserved_bytes,
            memory_map,
            firmware_regions: alloc::vec::Vec::new(),
            usable_regions,
            reserved_regions,
        }
    }

    /// Add a memory region
    pub fn add_region(&mut self, region: MemoryRegion) {
        self.memory_map.push(region);

        // Recalculate
        let usable_regions: alloc::vec::Vec<MemoryRegion> = self
            .memory_map
            .iter()
            .filter(|r| r.is_usable)
            .cloned()
            .collect();

        let reserved_regions: alloc::vec::Vec<MemoryRegion> = self
            .memory_map
            .iter()
            .filter(|r| !r.is_usable)
            .cloned()
            .collect();

        self.usable_regions = usable_regions;
        self.reserved_regions = reserved_regions;
        self.usable_bytes = self.usable_regions.iter().map(|r| r.size).sum();
        self.reserved_bytes = self.reserved_regions.iter().map(|r| r.size).sum();
        self.total_bytes = self.memory_map.iter().map(|r| r.size).sum();
    }

    /// Get total memory in MiB
    pub fn total_mib(&self) -> u64 {
        self.total_bytes / (1024 * 1024)
    }

    /// Get usable memory in MiB
    pub fn usable_mib(&self) -> u64 {
        self.usable_bytes / (1024 * 1024)
    }

    /// Get reserved memory in MiB
    pub fn reserved_mib(&self) -> u64 {
        self.reserved_bytes / (1024 * 1024)
    }

    /// Get a summary of memory information
    pub fn summary(&self) -> alloc::string::String {
        alloc::format!(
            "Total Memory: {} MiB\nUsable Memory: {} MiB\nReserved Memory: {} MiB\nMemory Regions: {}",
            self.total_mib(),
            self.usable_mib(),
            self.reserved_mib(),
            self.memory_map.len()
        )
    }

    /// Get detailed memory map as string
    pub fn memory_map_string(&self) -> alloc::string::String {
        let mut result = alloc::string::String::new();
        result.push_str("Memory Map:\n");
        for region in &self.memory_map {
            result.push_str(&alloc::format!("  {}\n", region.format()));
        }
        result
    }

    /// Get usable regions as string
    pub fn usable_regions_string(&self) -> alloc::string::String {
        let mut result = alloc::string::String::new();
        result.push_str("Usable Memory Regions:\n");
        for region in &self.usable_regions {
            result.push_str(&alloc::format!("  {}\n", region.format()));
        }
        result
    }

    /// Get reserved regions as string
    pub fn reserved_regions_string(&self) -> alloc::string::String {
        let mut result = alloc::string::String::new();
        result.push_str("Reserved Memory Regions:\n");
        for region in &self.reserved_regions {
            result.push_str(&alloc::format!("  {}\n", region.format()));
        }
        result
    }
}

/// Global memory information (initialized once at boot)
static mut MEMORY_INFO: Option<MemoryInfo> = None;

/// Initialize memory detection
/// Must be called once during boot
pub fn init(total_memory: u64) {
    unsafe {
        // For now, create basic memory info
        // In a real implementation, we would call e820 or similar
        MEMORY_INFO = Some(MemoryInfo::new_with_total(total_memory));
    }
}

/// Initialize memory detection with a custom memory map
pub fn init_with_memory_map(memory_map: alloc::vec::Vec<MemoryRegion>) {
    unsafe {
        MEMORY_INFO = Some(MemoryInfo::with_memory_map(memory_map));
    }
}

/// Get reference to global memory information
/// Panics if memory detection has not been initialized
#[allow(static_mut_refs)]
pub fn memory_info() -> &'static MemoryInfo {
    unsafe {
        MEMORY_INFO
            .as_ref()
            .expect("Memory detection not initialized")
    }
}

/// Check if memory detection has been initialized
#[allow(static_mut_refs)]
pub fn is_initialized() -> bool {
    unsafe { MEMORY_INFO.is_some() }
}

// -- Implementation --

impl MemoryInfo {
    /// Create memory info with just total memory (fallback method)
    fn new_with_total(total_bytes: u64) -> Self {
        let mut memory_map = alloc::vec::Vec::new();

        // Reserve standard regions
        // 0 - 1 MiB: BIOS, IVT, BDA, VGA, etc.
        memory_map.push(MemoryRegion::new(
            PhysAddr::new(0),
            0x100000,
            MemoryRegionType::Reserved,
        ));

        // 1 MiB - total: Usable RAM
        if total_bytes > 0x100000 {
            memory_map.push(MemoryRegion::new(
                PhysAddr::new(0x100000),
                total_bytes - 0x100000,
                MemoryRegionType::Usable,
            ));
        }

        MemoryInfo::with_memory_map(memory_map)
    }
}

/// Detect total memory using CPUID
fn detect_total_memory() -> u64 {
    // Try CPUID 0x80000008 for physical address size
    let result = __cpuid(0x80000008);
    let physical_bits = (result.eax & 0xFF) as u8;

    if physical_bits > 0 {
        // Calculate max physical address
        // This gives us the address bits, not the actual installed memory
        // We'll use this as a fallback
        (1u64 << physical_bits) - 1
    } else {
        // Default to 256 MiB for QEMU/basic testing
        // This will be overridden by the boot parameters
        256 * 1024 * 1024
    }
}

/// Get total memory in bytes
pub fn total_bytes() -> u64 {
    if is_initialized() {
        memory_info().total_bytes
    } else {
        0
    }
}

/// Get total memory in MiB
pub fn total_mib() -> u64 {
    if is_initialized() {
        memory_info().total_mib()
    } else {
        0
    }
}

/// Get usable memory in bytes
pub fn usable_bytes() -> u64 {
    if is_initialized() {
        memory_info().usable_bytes
    } else {
        0
    }
}

/// Get usable memory in MiB
pub fn usable_mib() -> u64 {
    if is_initialized() {
        memory_info().usable_mib()
    } else {
        0
    }
}

/// Get reserved memory in bytes
pub fn reserved_bytes() -> u64 {
    if is_initialized() {
        memory_info().reserved_bytes
    } else {
        0
    }
}

/// Get reserved memory in MiB
pub fn reserved_mib() -> u64 {
    if is_initialized() {
        memory_info().reserved_mib()
    } else {
        0
    }
}

/// Get reference to the memory map
pub fn memory_map() -> &'static alloc::vec::Vec<MemoryRegion> {
    if is_initialized() {
        &memory_info().memory_map
    } else {
        static EMPTY: alloc::vec::Vec<MemoryRegion> = alloc::vec::Vec::new();
        &EMPTY
    }
}

/// Get reference to usable regions
pub fn usable_regions() -> &'static alloc::vec::Vec<MemoryRegion> {
    if is_initialized() {
        &memory_info().usable_regions
    } else {
        static EMPTY: alloc::vec::Vec<MemoryRegion> = alloc::vec::Vec::new();
        &EMPTY
    }
}

/// Get reference to reserved regions
pub fn reserved_regions() -> &'static alloc::vec::Vec<MemoryRegion> {
    if is_initialized() {
        &memory_info().reserved_regions
    } else {
        static EMPTY: alloc::vec::Vec<MemoryRegion> = alloc::vec::Vec::new();
        &EMPTY
    }
}
