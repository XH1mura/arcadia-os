//! System Information Module
//!
//! Central module for all system information including CPU and memory detection.
//! This module provides a unified interface for accessing hardware information.

pub mod cpu;
pub mod memory;

/// Initialize the system information module
/// This should be called once during early boot
pub fn init(total_memory: u64) {
    cpu::init();
    memory::init(total_memory);
}

/// Get a complete system information summary
pub fn system_summary() -> alloc::string::String {
    alloc::format!(
        "System Information\n{}\n{}",
        cpu::cpu_info().summary(),
        memory::memory_info().summary()
    )
}

/// System Information structure containing all detected information
#[derive(Debug, Clone)]
pub struct SystemInfo {
    /// CPU Information
    pub cpu: cpu::CpuInfo,
    /// Memory Information
    pub memory: memory::MemoryInfo,
}

impl SystemInfo {
    /// Create a new SystemInfo structure
    pub fn new() -> Self {
        SystemInfo {
            cpu: cpu::CpuInfo::new(),
            memory: memory::MemoryInfo::new(),
        }
    }

    /// Get a complete summary
    pub fn summary(&self) -> alloc::string::String {
        alloc::format!(
            "System Information\n{}\n{}",
            self.cpu.summary(),
            self.memory.summary()
        )
    }
}

/// Global system information
static mut SYSTEM_INFO: Option<SystemInfo> = None;

/// Initialize system information with total memory
pub fn init_with_total_memory(total_memory: u64) {
    cpu::init();
    memory::init(total_memory);
    unsafe {
        SYSTEM_INFO = Some(SystemInfo::new());
    }
}

/// Get reference to global system information
#[allow(static_mut_refs)]
pub fn system_info() -> Option<&'static SystemInfo> {
    unsafe { SYSTEM_INFO.as_ref() }
}

/// Check if system information has been initialized
pub fn is_initialized() -> bool {
    cpu::is_initialized() && memory::is_initialized()
}
