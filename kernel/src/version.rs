//! Single source of truth for Arcadia OS version information.
//!
//! All version reporting (boot screen, kernel version, ArcShell banner,
//! system information) must use the constants defined here.

/// Major version number.
pub const MAJOR: u32 = 0;

/// Minor version number.
pub const MINOR: u32 = 2;

/// Patch / build increment.
pub const PATCH: u32 = 0;

/// Release stage identifier.
pub const STAGE: &str = "Alpha";

/// Full version string, e.g. "0.2.0 Alpha".
pub const VERSION: &str = "0.2.0 Alpha";

/// Short version string, e.g. "v0.2 Alpha".
pub const VERSION_SHORT: &str = "v0.2 Alpha";

/// Human-readable OS name.
pub const OS_NAME: &str = "Arcadia OS";

/// Kernel binary name.
pub const KERNEL_NAME: &str = "arcadia-kernel";

/// Architecture string.
pub const ARCH: &str = "x86_64";

/// Formatted version line used by the boot screen and ArcShell banner.
pub const BANNER_VERSION: &str = "Arcadia OS v0.2 Alpha";

/// Complete, human-readable version description.
pub fn version_string() -> &'static str {
    VERSION
}
