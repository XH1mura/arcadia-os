#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
    Debug,
    Trace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogModule {
    Kernel,
    Memory,
    Drivers,
    Filesystem,
    Terminal,
    Flux,
    BuildSystem,
    Unknown,
}
