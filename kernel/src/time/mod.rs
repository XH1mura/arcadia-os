use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;
use x86_64::instructions::port::Port;

/// PIT oscillator frequency (Hz).
const PIT_FREQUENCY: u64 = 1_193_182;

/// Desired timer tick rate (Hz).
const TICK_RATE: u64 = 100;

/// PIT divisor for the desired tick rate.
const PIT_DIVISOR: u64 = PIT_FREQUENCY / TICK_RATE;

/// I/O port for PIT channel 0 data.
const PIT_CHANNEL0_PORT: u16 = 0x40;

/// I/O port for PIT command register.
const PIT_COMMAND_PORT: u16 = 0x43;

/// Total tick count since boot.
pub static TICK_COUNT: AtomicU64 = AtomicU64::new(0);

/// Milliseconds elapsed since boot (updated by sleep_ms).
static UPTIME_MS: AtomicU64 = AtomicU64::new(0);

/// Whether the PIT has been initialized.
static PIT_INITIALIZED: Mutex<bool> = Mutex::new(false);

/// Initialize the PIT channel 0 to fire at TICK_RATE Hz.
///
/// Configures PIT channel 0 in mode 2 (rate generator), lobyte/hibyte
/// access, binary mode. After this call, IRQ0 fires TICK_RATE times per second.
///
/// NOTE: Mode 3 (square wave) causes triple faults on QEMU's 8259A PIC.
/// Mode 2 is used instead — it generates a periodic low pulse on OUT0.
pub fn init() {
    let mut initialized = PIT_INITIALIZED.lock();
    if *initialized {
        return;
    }

    let divisor = PIT_DIVISOR as u16;
    let divisor_lo = (divisor & 0xFF) as u8;
    let divisor_hi = ((divisor >> 8) & 0xFF) as u8;

    // Command byte: channel 0, lobyte/hibyte, mode 2 (rate generator), binary
    // Bits: 00 | 11 | 010 | 0 | 0
    //       ch  access mode2  bcd
    let cmd: u8 = 0b00_11_010_0; // 0x34
    unsafe {
        Port::<u8>::new(PIT_COMMAND_PORT).write(cmd);
        Port::<u8>::new(PIT_CHANNEL0_PORT).write(divisor_lo);
        Port::<u8>::new(PIT_CHANNEL0_PORT).write(divisor_hi);
    }

    *initialized = true;
}

/// Called from the timer interrupt handler (IRQ 32) on each tick.
#[inline]
pub fn tick() {
    TICK_COUNT.fetch_add(1, Ordering::Relaxed);
}

/// Returns the total number of timer ticks since boot.
pub fn ticks() -> u64 {
    TICK_COUNT.load(Ordering::Acquire)
}

/// Returns elapsed time in milliseconds since boot.
///
/// This is a coarse estimate based on tick count multiplied by the tick period.
/// For accurate millisecond tracking, the sleep function updates `UPTIME_MS`.
pub fn uptime_ms() -> u64 {
    let tick = TICK_COUNT.load(Ordering::Acquire);
    tick * (1000 / TICK_RATE as u64)
}

/// Returns elapsed time in seconds since boot.
pub fn uptime_secs() -> u64 {
    uptime_ms() / 1000
}

/// Busy-wait for approximately `ms` milliseconds.
///
/// Uses PIT tick counting for timing. During the wait, interrupts remain
/// enabled so the timer IRQ continues to fire and update the tick counter.
/// This is a spin-wait — the CPU loops on `hlt` instructions to reduce
/// power consumption while waiting.
pub fn sleep_ms(ms: u64) {
    let target_ticks = (ms * TICK_RATE) / 1000;
    if target_ticks == 0 {
        return;
    }
    let start = TICK_COUNT.load(Ordering::Acquire);
    let end = start + target_ticks;
    while TICK_COUNT.load(Ordering::Acquire) < end {
        x86_64::instructions::hlt();
    }
    UPTIME_MS.fetch_add(ms, Ordering::Relaxed);
}

/// Returns the PIT tick rate (Hz).
pub fn tick_rate() -> u64 {
    TICK_RATE
}
