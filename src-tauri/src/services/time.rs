use std::sync::OnceLock;
use std::time::Instant;

#[cfg_attr(target_os = "macos", allow(dead_code))]
static MONOTONIC_START: OnceLock<Instant> = OnceLock::new();

/// Returns a monotonic timestamp in nanoseconds relative to process start.
/// Using a shared origin lets different threads compare times without
/// depending on platform-specific reference epochs.
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub fn monotonic_now_ns() -> u64 {
    let start = MONOTONIC_START.get_or_init(Instant::now);
    let elapsed = Instant::now().duration_since(*start);
    elapsed.as_nanos().min(u128::from(u64::MAX)) as u64
}

#[cfg(target_os = "macos")]
mod mac_host_time {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Once;

    #[repr(C)]
    #[derive(Default, Copy, Clone)]
    struct MachTimebaseInfo {
        numer: u32,
        denom: u32,
    }

    extern "C" {
        fn mach_absolute_time() -> u64;
        fn mach_timebase_info(info: *mut MachTimebaseInfo) -> i32;
    }

    static INIT: Once = Once::new();
    static NUMER: AtomicU64 = AtomicU64::new(0);
    static DENOM: AtomicU64 = AtomicU64::new(0);

    fn ensure_timebase() {
        INIT.call_once(|| {
            let mut info = MachTimebaseInfo::default();
            unsafe {
                mach_timebase_info(&mut info);
            }
            NUMER.store(info.numer as u64, Ordering::Relaxed);
            DENOM.store(info.denom.max(1) as u64, Ordering::Relaxed);
        });
    }

    pub fn host_time_now_ns() -> u64 {
        ensure_timebase();
        let numer = NUMER.load(Ordering::Relaxed);
        let denom = DENOM.load(Ordering::Relaxed);
        let raw = unsafe { mach_absolute_time() };
        let nanos = (raw as u128)
            .saturating_mul(numer as u128)
            .checked_div(denom as u128)
            .unwrap_or(0);
        nanos.min(u128::from(u64::MAX)) as u64
    }
}

#[cfg(target_os = "macos")]
pub use mac_host_time::host_time_now_ns;

#[cfg(target_os = "macos")]
use screencapturekit::CMTime;

#[cfg(target_os = "macos")]
/// Converts a CoreMedia CMTime into nanoseconds, clamping on overflow.
pub fn cm_time_to_ns(time: CMTime) -> u64 {
    if time.timescale == 0 {
        return 0;
    }

    let numerator = time.value as i128;
    let denominator = time.timescale as i128;
    if denominator == 0 {
        return 0;
    }

    let nanos = numerator
        .saturating_mul(1_000_000_000i128)
        .checked_div(denominator)
        .unwrap_or(0);

    if nanos < 0 {
        0
    } else {
        nanos.min(i128::from(u64::MAX)) as u64
    }
}
