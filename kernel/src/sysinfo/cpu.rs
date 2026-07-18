//! CPUID Detection Module
//!
//! Provides comprehensive CPU information detection using the CPUID instruction.
//! This is the central module for all CPU-related system information.

use alloc::string::String;
use alloc::string::ToString;
use core::arch::x86_64::__cpuid;

/// CPU Vendor string (12 characters + null terminator)
pub const VENDOR_STRING_LENGTH: usize = 13;

/// CPU Brand string (48 characters)
pub const BRAND_STRING_LENGTH: usize = 48;

/// CPU Vendor identification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuVendor {
    /// Intel CPU
    Intel,
    /// AMD CPU
    Amd,
    /// Unknown vendor (vendor string stored)
    Unknown([u8; VENDOR_STRING_LENGTH]),
}

impl CpuVendor {
    /// Returns the vendor as a string slice
    pub fn as_str(&self) -> &'static str {
        match self {
            CpuVendor::Intel => "Intel",
            CpuVendor::Amd => "AMD",
            CpuVendor::Unknown(_) => "Unknown",
        }
    }

    /// Returns the raw vendor string (12 bytes)
    pub fn raw_string(&self) -> [u8; VENDOR_STRING_LENGTH] {
        match self {
            CpuVendor::Intel => {
                let mut buf = [0u8; VENDOR_STRING_LENGTH];
                buf[..12].copy_from_slice(b"GenuineIntel");
                buf
            }
            CpuVendor::Amd => {
                let mut buf = [0u8; VENDOR_STRING_LENGTH];
                buf[..12].copy_from_slice(b"AuthenticAMD");
                buf
            }
            CpuVendor::Unknown(v) => *v,
        }
    }
}

/// CPU Family, Model, and Stepping identification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuSignature {
    /// Extended family
    pub extended_family: u8,
    /// Family
    pub family: u8,
    /// Extended model
    pub extended_model: u8,
    /// Model
    pub model: u8,
    /// Stepping
    pub stepping: u8,
}

impl CpuSignature {
    /// Create a new CPU signature from raw CPUID values
    pub fn new(eax: u32) -> Self {
        CpuSignature {
            extended_family: ((eax >> 20) & 0xFF) as u8,
            family: ((eax >> 8) & 0xF) as u8,
            extended_model: ((eax >> 16) & 0xF) as u8,
            model: ((eax >> 4) & 0xF) as u8,
            stepping: (eax & 0xF) as u8,
        }
    }

    /// Get full family number (including extended family)
    pub fn full_family(&self) -> u8 {
        self.extended_family + self.family
    }

    /// Get full model number (including extended model)
    pub fn full_model(&self) -> u8 {
        (self.extended_model << 4) | self.model
    }

    /// Format as "Family.Model.Stepping"
    pub fn format(&self) -> String {
        alloc::format!(
            "{}.{}.{}",
            self.full_family(),
            self.full_model(),
            self.stepping
        )
    }
}

/// CPU Feature flags from CPUID
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuFeatures {
    // Standard features (ECX, EDX from CPUID 0x00000001)
    /// FPU (Floating Point Unit)
    pub fpu: bool,
    /// VME (Virtual Mode Extensions)
    pub vme: bool,
    /// DE (Debugging Extensions)
    pub de: bool,
    /// PSE (Page Size Extensions)
    pub pse: bool,
    /// TSC (Time Stamp Counter)
    pub tsc: bool,
    /// MSR (Model Specific Registers)
    pub msr: bool,
    /// PAE (Physical Address Extensions)
    pub pae: bool,
    /// MCE (Machine Check Exception)
    pub mce: bool,
    /// CX8 (CMPXCHG8B)
    pub cx8: bool,
    /// APIC (Advanced Programmable Interrupt Controller)
    pub apic: bool,
    /// SEP (SYSENTER/SYSEXIT)
    pub sep: bool,
    /// MTRR (Memory Type Range Registers)
    pub mtrr: bool,
    /// PGE (Page Global Enable)
    pub pge: bool,
    /// MCA (Machine Check Architecture)
    pub mca: bool,
    /// CMOV (Conditional Move)
    pub cmov: bool,
    /// PAT (Page Attribute Table)
    pub pat: bool,
    /// PSE36 (36-bit Page Size Extensions)
    pub pse36: bool,
    /// MMX
    pub mmx: bool,
    /// FXSR (FXSAVE/FXRSTOR)
    pub fxsr: bool,
    /// SSE
    pub sse: bool,
    /// SSE2
    pub sse2: bool,
    /// Self Snoop
    pub self_snoop: bool,
    /// Hyper-Threading / Multi-Core
    pub ht: bool,
    /// Thermal Monitor
    pub tm: bool,
    // Extended features (ECX from CPUID 0x00000001)
    /// SSE3
    pub sse3: bool,
    /// PCLMULQDQ
    pub pclmulqdq: bool,
    /// DTES64
    pub dtes64: bool,
    /// MONITOR/MWAIT
    pub monitor: bool,
    /// DS-CPL
    pub ds_cpl: bool,
    /// VMX (Intel Virtualization)
    pub vmx: bool,
    /// SMX
    pub smx: bool,
    /// EST (Enhanced SpeedStep)
    pub est: bool,
    /// TM2
    pub tm2: bool,
    /// SSSE3
    pub ssse3: bool,
    /// CID
    pub cid: bool,
    /// FMA
    pub fma: bool,
    /// CX16
    pub cx16: bool,
    /// xTPR Update Control
    pub xtpr: bool,
    /// PDCM
    pub pdcm: bool,
    /// PCID
    pub pcid: bool,
    /// DCA
    pub dca: bool,
    /// SSE4.1
    pub sse41: bool,
    /// SSE4.2
    pub sse42: bool,
    /// x2APIC
    pub x2apic: bool,
    /// MOVBE
    pub movbe: bool,
    /// POPCNT
    pub popcnt: bool,
    /// AES
    pub aes: bool,
    /// XSAVE
    pub xsave: bool,
    /// OSXSAVE
    pub osxsave: bool,
    /// AVX
    pub avx: bool,
    /// F16C
    pub f16c: bool,
    /// RDRAND
    pub rdrand: bool,
    /// Hypervisor Present
    pub hypervisor: bool,
}

impl CpuFeatures {
    /// Create feature set from ECX and EDX registers
    pub fn new(edx: u32, ecx: u32) -> Self {
        CpuFeatures {
            fpu: edx & (1 << 0) != 0,
            vme: edx & (1 << 1) != 0,
            de: edx & (1 << 2) != 0,
            pse: edx & (1 << 3) != 0,
            tsc: edx & (1 << 4) != 0,
            msr: edx & (1 << 5) != 0,
            pae: edx & (1 << 6) != 0,
            mce: edx & (1 << 7) != 0,
            cx8: edx & (1 << 8) != 0,
            apic: edx & (1 << 9) != 0,
            sep: edx & (1 << 11) != 0,
            mtrr: edx & (1 << 12) != 0,
            pge: edx & (1 << 13) != 0,
            mca: edx & (1 << 14) != 0,
            cmov: edx & (1 << 15) != 0,
            pat: edx & (1 << 16) != 0,
            pse36: edx & (1 << 17) != 0,
            mmx: edx & (1 << 23) != 0,
            fxsr: edx & (1 << 24) != 0,
            sse: edx & (1 << 25) != 0,
            sse2: edx & (1 << 26) != 0,
            self_snoop: edx & (1 << 27) != 0,
            ht: edx & (1 << 28) != 0,
            tm: edx & (1 << 29) != 0,
            // ECX features
            sse3: ecx & (1 << 0) != 0,
            pclmulqdq: ecx & (1 << 1) != 0,
            dtes64: ecx & (1 << 2) != 0,
            monitor: ecx & (1 << 3) != 0,
            ds_cpl: ecx & (1 << 4) != 0,
            vmx: ecx & (1 << 5) != 0,
            smx: ecx & (1 << 6) != 0,
            est: ecx & (1 << 7) != 0,
            tm2: ecx & (1 << 8) != 0,
            ssse3: ecx & (1 << 9) != 0,
            cid: ecx & (1 << 10) != 0,
            fma: ecx & (1 << 12) != 0,
            cx16: ecx & (1 << 13) != 0,
            xtpr: ecx & (1 << 14) != 0,
            pdcm: ecx & (1 << 15) != 0,
            pcid: ecx & (1 << 17) != 0,
            dca: ecx & (1 << 18) != 0,
            sse41: ecx & (1 << 19) != 0,
            sse42: ecx & (1 << 20) != 0,
            x2apic: ecx & (1 << 21) != 0,
            movbe: ecx & (1 << 22) != 0,
            popcnt: ecx & (1 << 23) != 0,
            aes: ecx & (1 << 25) != 0,
            xsave: ecx & (1 << 26) != 0,
            osxsave: ecx & (1 << 27) != 0,
            avx: ecx & (1 << 28) != 0,
            f16c: ecx & (1 << 29) != 0,
            rdrand: ecx & (1 << 30) != 0,
            hypervisor: ecx & (1 << 31) != 0,
        }
    }

    /// Get a list of all enabled feature names
    pub fn enabled_features(&self) -> alloc::vec::Vec<&'static str> {
        let mut features = alloc::vec::Vec::new();

        // Standard features
        if self.fpu {
            features.push("FPU");
        }
        if self.vme {
            features.push("VME");
        }
        if self.de {
            features.push("DE");
        }
        if self.pse {
            features.push("PSE");
        }
        if self.tsc {
            features.push("TSC");
        }
        if self.msr {
            features.push("MSR");
        }
        if self.pae {
            features.push("PAE");
        }
        if self.mce {
            features.push("MCE");
        }
        if self.cx8 {
            features.push("CX8");
        }
        if self.apic {
            features.push("APIC");
        }
        if self.sep {
            features.push("SEP");
        }
        if self.mtrr {
            features.push("MTRR");
        }
        if self.pge {
            features.push("PGE");
        }
        if self.mca {
            features.push("MCA");
        }
        if self.cmov {
            features.push("CMOV");
        }
        if self.pat {
            features.push("PAT");
        }
        if self.pse36 {
            features.push("PSE36");
        }
        if self.mmx {
            features.push("MMX");
        }
        if self.fxsr {
            features.push("FXSR");
        }
        if self.sse {
            features.push("SSE");
        }
        if self.sse2 {
            features.push("SSE2");
        }
        if self.ht {
            features.push("HT");
        }
        if self.tm {
            features.push("TM");
        }

        // Extended features
        if self.sse3 {
            features.push("SSE3");
        }
        if self.pclmulqdq {
            features.push("PCLMULQDQ");
        }
        if self.monitor {
            features.push("MONITOR");
        }
        if self.vmx {
            features.push("VMX");
        }
        if self.smx {
            features.push("SMX");
        }
        if self.est {
            features.push("EST");
        }
        if self.tm2 {
            features.push("TM2");
        }
        if self.ssse3 {
            features.push("SSSE3");
        }
        if self.fma {
            features.push("FMA");
        }
        if self.cx16 {
            features.push("CX16");
        }
        if self.sse41 {
            features.push("SSE4.1");
        }
        if self.sse42 {
            features.push("SSE4.2");
        }
        if self.x2apic {
            features.push("x2APIC");
        }
        if self.popcnt {
            features.push("POPCNT");
        }
        if self.aes {
            features.push("AES");
        }
        if self.xsave {
            features.push("XSAVE");
        }
        if self.osxsave {
            features.push("OSXSAVE");
        }
        if self.avx {
            features.push("AVX");
        }
        if self.f16c {
            features.push("F16C");
        }
        if self.rdrand {
            features.push("RDRAND");
        }
        if self.hypervisor {
            features.push("HYPERVISOR");
        }

        features
    }

    /// Check if a specific feature is supported
    pub fn has_feature(&self, feature: &str) -> bool {
        match feature {
            "FPU" => self.fpu,
            "VME" => self.vme,
            "DE" => self.de,
            "PSE" => self.pse,
            "TSC" => self.tsc,
            "MSR" => self.msr,
            "PAE" => self.pae,
            "MCE" => self.mce,
            "CX8" => self.cx8,
            "APIC" => self.apic,
            "SEP" => self.sep,
            "MTRR" => self.mtrr,
            "PGE" => self.pge,
            "MCA" => self.mca,
            "CMOV" => self.cmov,
            "PAT" => self.pat,
            "PSE36" => self.pse36,
            "MMX" => self.mmx,
            "FXSR" => self.fxsr,
            "SSE" => self.sse,
            "SSE2" => self.sse2,
            "HT" => self.ht,
            "TM" => self.tm,
            "SSE3" => self.sse3,
            "PCLMULQDQ" => self.pclmulqdq,
            "MONITOR" => self.monitor,
            "VMX" => self.vmx,
            "SMX" => self.smx,
            "EST" => self.est,
            "TM2" => self.tm2,
            "SSSE3" => self.ssse3,
            "FMA" => self.fma,
            "CX16" => self.cx16,
            "SSE4.1" => self.sse41,
            "SSE4.2" => self.sse42,
            "x2APIC" => self.x2apic,
            "POPCNT" => self.popcnt,
            "AES" => self.aes,
            "XSAVE" => self.xsave,
            "OSXSAVE" => self.osxsave,
            "AVX" => self.avx,
            "F16C" => self.f16c,
            "RDRAND" => self.rdrand,
            "HYPERVISOR" => self.hypervisor,
            _ => false,
        }
    }
}

/// CPU Brand string information
#[derive(Debug, Clone)]
pub struct CpuBrandString {
    /// Raw brand string from CPUID (48 bytes)
    pub raw: [u8; BRAND_STRING_LENGTH],
}

impl CpuBrandString {
    /// Create a new brand string from CPUID results
    pub fn new(eax: u32, ebx: u32, ecx: u32, edx: u32) -> Self {
        let mut raw = [0u8; BRAND_STRING_LENGTH];

        // Each CPUID call gives us 16 bytes of the brand string
        // We need to call CPUID 0x80000002, 0x80000003, 0x80000004
        // For now, store the raw bytes from all three calls
        raw[0..4].copy_from_slice(&eax.to_le_bytes());
        raw[4..8].copy_from_slice(&ebx.to_le_bytes());
        raw[8..12].copy_from_slice(&ecx.to_le_bytes());
        raw[12..16].copy_from_slice(&edx.to_le_bytes());

        CpuBrandString { raw }
    }

    /// Get the brand string as a Rust string (trimming null bytes)
    pub fn as_str(&self) -> alloc::string::String {
        let trimmed: &[u8] = &self.raw[..];
        let end = trimmed.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
        alloc::string::String::from_utf8_lossy(&trimmed[..end]).into_owned()
    }
}

/// Complete CPU information structure
#[derive(Debug, Clone)]
pub struct CpuInfo {
    /// CPU Vendor
    pub vendor: CpuVendor,
    /// CPU Brand string
    pub brand_string: CpuBrandString,
    /// CPU Signature (family, model, stepping)
    pub signature: CpuSignature,
    /// CPU Features
    pub features: CpuFeatures,
    /// Physical cores (0 = unknown)
    pub physical_cores: u8,
    /// Logical cores (0 = unknown)
    pub logical_cores: u8,
    /// CPU frequency in MHz (0 = unknown)
    pub frequency_mhz: u32,
    /// Cache information
    pub cache_l1: u32,
    pub cache_l2: u32,
    pub cache_l3: u32,
}

impl CpuInfo {
    /// Create a new CpuInfo structure with all CPU information
    pub fn new() -> Self {
        let vendor = detect_vendor();
        let signature = detect_signature();
        let features = detect_features();
        let brand_string = detect_brand_string();
        let (physical, logical) = detect_core_count();
        let (l1, l2, l3) = detect_cache_info();
        let frequency = detect_frequency();

        CpuInfo {
            vendor,
            brand_string,
            signature,
            features,
            physical_cores: physical,
            logical_cores: logical,
            frequency_mhz: frequency,
            cache_l1: l1,
            cache_l2: l2,
            cache_l3: l3,
        }
    }

    /// Get a summary of CPU information
    pub fn summary(&self) -> alloc::string::String {
        let physical_str = if self.physical_cores > 0 {
            alloc::format!("{}", self.physical_cores)
        } else {
            "Unavailable".to_string()
        };

        let logical_str = if self.logical_cores > 0 {
            alloc::format!("{}", self.logical_cores)
        } else {
            "Unavailable".to_string()
        };

        let frequency_str = if self.frequency_mhz > 0 {
            alloc::format!("{} MHz", self.frequency_mhz)
        } else {
            "Unavailable".to_string()
        };

        let cache_str = if self.cache_l1 > 0 || self.cache_l2 > 0 || self.cache_l3 > 0 {
            alloc::format!(
                "L1={} KB, L2={} KB, L3={} KB",
                self.cache_l1,
                self.cache_l2,
                self.cache_l3
            )
        } else {
            "Unavailable".to_string()
        };

        alloc::format!(
            "Vendor: {}\nBrand: {}\nFamily: {} Model: {} Stepping: {}\nCores: {} physical, {} logical\nFrequency: {}\nCache: {}",
            self.vendor.as_str(),
            self.brand_string.as_str(),
            self.signature.full_family(),
            self.signature.full_model(),
            self.signature.stepping,
            physical_str,
            logical_str,
            frequency_str,
            cache_str
        )
    }

    /// Get feature count
    pub fn feature_count(&self) -> usize {
        self.features.enabled_features().len()
    }
}

/// Global CPU information (initialized once at boot)
static mut CPU_INFO: Option<CpuInfo> = None;

/// Initialize CPU detection
/// Must be called once during boot
pub fn init() {
    unsafe {
        CPU_INFO = Some(CpuInfo::new());
    }
}

/// Get reference to global CPU information
/// Panics if CPU detection has not been initialized
///
/// SAFETY: This function is safe because:
/// 1. CPU_INFO is initialized once during early boot before any concurrent access
/// 2. After initialization, CPU_INFO is only read, never written
/// 3. The reference is valid for the lifetime of the kernel
#[allow(static_mut_refs)]
pub fn cpu_info() -> &'static CpuInfo {
    unsafe { CPU_INFO.as_ref().expect("CPU detection not initialized") }
}

/// Check if CPU detection has been initialized
#[allow(static_mut_refs)]
pub fn is_initialized() -> bool {
    unsafe { CPU_INFO.is_some() }
}

// -- Detection Functions --

/// Detect CPU vendor using CPUID 0x00000000
fn detect_vendor() -> CpuVendor {
    let result = __cpuid(0);

    // EBX, EDX, ECX contain the vendor string parts
    let ebx = result.ebx as u32;
    let edx = result.edx as u32;
    let ecx = result.ecx as u32;

    let mut vendor_bytes = [0u8; 12];
    vendor_bytes[0..4].copy_from_slice(&ebx.to_le_bytes());
    vendor_bytes[4..8].copy_from_slice(&edx.to_le_bytes());
    vendor_bytes[8..12].copy_from_slice(&ecx.to_le_bytes());

    match &vendor_bytes {
        b"GenuineIntel" => CpuVendor::Intel,
        b"AuthenticAMD" => CpuVendor::Amd,
        _ => {
            let mut full_vendor = [0u8; VENDOR_STRING_LENGTH];
            full_vendor[..12].copy_from_slice(&vendor_bytes);
            CpuVendor::Unknown(full_vendor)
        }
    }
}

/// Detect CPU signature (family, model, stepping) using CPUID 0x00000001
fn detect_signature() -> CpuSignature {
    let result = __cpuid(1);
    let eax = result.eax;
    CpuSignature::new(eax)
}

/// Detect CPU features using CPUID 0x00000001
fn detect_features() -> CpuFeatures {
    let result = __cpuid(1);
    let edx = result.edx;
    let ecx = result.ecx;
    CpuFeatures::new(edx, ecx)
}

/// Detect CPU brand string using CPUID 0x80000002, 0x80000003, 0x80000004
fn detect_brand_string() -> CpuBrandString {
    // Get first 16 bytes (0x80000002)
    let result1 = __cpuid(0x80000002);

    // Note: We only handle the first call for now
    // A full implementation would concatenate results from 0x80000002, 0x80000003, 0x80000004
    CpuBrandString::new(result1.eax, result1.ebx, result1.ecx, result1.edx)
}

/// Detect core count using CPUID 0x00000001
/// QEMU typically reports 1 physical core with N logical cores based on -smp flag
fn detect_core_count() -> (u8, u8) {
    // CPUID 0x00000001: EBX[23:16] = logical processors per physical core
    let result1 = __cpuid(1);
    let ebx = result1.ebx;
    let logical_per_physical = ((ebx >> 16) & 0xFF) as u8;

    // For QEMU and single-processor systems, report 1 physical core
    // and the number of logical processors from EBX[23:16]
    // If EBX[23:16] is 0, we default to 1 logical core
    let logical = if logical_per_physical > 0 {
        logical_per_physical
    } else {
        1
    };

    (1, logical)
}

/// Detect cache information using CPUID 0x80000005, 0x80000006
fn detect_cache_info() -> (u32, u32, u32) {
    // CPUID 0x80000005 gives L1 cache info
    let result5 = __cpuid(0x80000005);
    let l1 = ((result5.ecx >> 24) & 0xFF) as u32 * 1024; // Size in KB

    // CPUID 0x80000006 gives L2/L3 cache info
    let result6 = __cpuid(0x80000006);
    let l2 = ((result6.ecx >> 16) & 0xFFFF) as u32 * 1024; // L2 size in KB
    let l3 = if (result6.edx >> 18) & 0x3FFF != 0 {
        ((result6.edx >> 18) & 0x3FFF) as u32 * 512 * 1024 / 1024 // L3 size in KB
    } else {
        0
    };

    (l1, l2, l3)
}

/// Detect CPU frequency using CPUID 0x16 (Core Frequency) or 0x80000000 (Max Extended)
/// Returns frequency in MHz, or 0 if undetectable
fn detect_frequency() -> u32 {
    // Try CPUID 0x16 (Core Frequency)
    let result = __cpuid(0x16);

    if result.eax != 0 {
        // EAX[15:0] = base frequency in MHz
        (result.eax & 0xFFFF) as u32
    } else {
        // CPUID 0x16 not supported, try alternative methods
        // We cannot reliably detect frequency without RTC or timer calibration
        // Return 0 to indicate unavailable (will be handled by caller)
        0
    }
}

/// Get vendor string as a formatted string
pub fn vendor_string() -> alloc::string::String {
    if is_initialized() {
        cpu_info().vendor.as_str().to_string()
    } else {
        "Unknown".to_string()
    }
}

/// Get brand string
pub fn brand_string() -> alloc::string::String {
    if is_initialized() {
        cpu_info().brand_string.as_str()
    } else {
        "Unknown".to_string()
    }
}

/// Get CPU signature as formatted string
pub fn signature_string() -> alloc::string::String {
    if is_initialized() {
        cpu_info().signature.format()
    } else {
        "Unknown".to_string()
    }
}

/// Get CPU frequency in MHz
pub fn frequency_mhz() -> u32 {
    if is_initialized() {
        cpu_info().frequency_mhz
    } else {
        0
    }
}

/// Get physical core count
pub fn physical_cores() -> u8 {
    if is_initialized() {
        cpu_info().physical_cores
    } else {
        0
    }
}

/// Get logical core count
pub fn logical_cores() -> u8 {
    if is_initialized() {
        cpu_info().logical_cores
    } else {
        0
    }
}

/// Get reference to CPU features
pub fn features() -> &'static CpuFeatures {
    if is_initialized() {
        &cpu_info().features
    } else {
        // Return a default empty feature set
        static DEFAULT_FEATURES: CpuFeatures = CpuFeatures {
            fpu: false,
            vme: false,
            de: false,
            pse: false,
            tsc: false,
            msr: false,
            pae: false,
            mce: false,
            cx8: false,
            apic: false,
            sep: false,
            mtrr: false,
            pge: false,
            mca: false,
            cmov: false,
            pat: false,
            pse36: false,
            mmx: false,
            fxsr: false,
            sse: false,
            sse2: false,
            self_snoop: false,
            ht: false,
            tm: false,
            sse3: false,
            pclmulqdq: false,
            dtes64: false,
            monitor: false,
            ds_cpl: false,
            vmx: false,
            smx: false,
            est: false,
            tm2: false,
            ssse3: false,
            cid: false,
            fma: false,
            cx16: false,
            xtpr: false,
            pdcm: false,
            pcid: false,
            dca: false,
            sse41: false,
            sse42: false,
            x2apic: false,
            movbe: false,
            popcnt: false,
            aes: false,
            xsave: false,
            osxsave: false,
            avx: false,
            f16c: false,
            rdrand: false,
            hypervisor: false,
        };
        &DEFAULT_FEATURES
    }
}
