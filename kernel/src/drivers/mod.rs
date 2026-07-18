pub mod pci;
pub mod ata;

use core::fmt;

/// Status of a driver instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverStatus {
    Uninitialized,
    Initializing,
    Ready,
    Error,
    Disabled,
}

impl fmt::Display for DriverStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DriverStatus::Uninitialized => write!(f, "Uninitialized"),
            DriverStatus::Initializing => write!(f, "Initializing"),
            DriverStatus::Ready => write!(f, "Ready"),
            DriverStatus::Error => write!(f, "Error"),
            DriverStatus::Disabled => write!(f, "Disabled"),
        }
    }
}

/// Core driver trait that all hardware drivers must implement.
pub trait Driver: Send + Sync {
    fn name(&self) -> &str;
    fn init(&mut self) -> Result<(), DriverError>;
    fn status(&self) -> DriverStatus;
    fn reset(&mut self) -> Result<(), DriverError>;
    fn shutdown(&mut self);
    fn probe(&self) -> bool;
}

/// Standard error type for driver operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverError {
    DeviceNotFound,
    InitFailed,
    Timeout,
    IoError,
    NotReady,
    Unsupported,
    InvalidParam,
    BufferTooSmall,
}

impl fmt::Display for DriverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DriverError::DeviceNotFound => write!(f, "Device not found"),
            DriverError::InitFailed => write!(f, "Initialization failed"),
            DriverError::Timeout => write!(f, "Operation timed out"),
            DriverError::IoError => write!(f, "I/O error"),
            DriverError::NotReady => write!(f, "Device not ready"),
            DriverError::Unsupported => write!(f, "Unsupported operation"),
            DriverError::InvalidParam => write!(f, "Invalid parameter"),
            DriverError::BufferTooSmall => write!(f, "Buffer too small"),
        }
    }
}
