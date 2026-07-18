use core::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArcadiaError {
    NotFound,
    PermissionDenied,
    InvalidPath,
    IoError,
    OutOfMemory,
    DeviceNotFound,
    UnsupportedOperation,
    BufferTooSmall,
    AlreadyExists,
    InvalidArgument,
    NotInitialized,
    Timeout,
    Unknown,
}

impl fmt::Display for ArcadiaError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ArcadiaError::NotFound => write!(f, "Not found"),
            ArcadiaError::PermissionDenied => write!(f, "Permission denied"),
            ArcadiaError::InvalidPath => write!(f, "Invalid path"),
            ArcadiaError::IoError => write!(f, "I/O error"),
            ArcadiaError::OutOfMemory => write!(f, "Out of memory"),
            ArcadiaError::DeviceNotFound => write!(f, "Device not found"),
            ArcadiaError::UnsupportedOperation => write!(f, "Unsupported operation"),
            ArcadiaError::BufferTooSmall => write!(f, "Buffer too small"),
            ArcadiaError::AlreadyExists => write!(f, "Already exists"),
            ArcadiaError::InvalidArgument => write!(f, "Invalid argument"),
            ArcadiaError::NotInitialized => write!(f, "Not initialized"),
            ArcadiaError::Timeout => write!(f, "Timeout"),
            ArcadiaError::Unknown => write!(f, "Unknown error"),
        }
    }
}

pub type Result<T> = core::result::Result<T, ArcadiaError>;
