//! Bounded workload calibration against the Kernel's monotonic nanosecond clock.
use kernel_client::call::room;

pub const BURST_MS: usize = 1;
const CHUNK: usize = 4_096;
const MAX_CHUNKS: usize = 4_096;

pub fn spin(n: usize) {
    let mut acc = 11usize;
    for _ in 0..n {
        acc = acc.wrapping_mul(3).wrapping_add(1);
        core::hint::spin_loop();
    }
    core::hint::black_box(acc);
}

pub fn spin_iters(iters: usize) {
    spin(iters);
}

/// A measured sample, rather than a guessed timer interrupt period.
#[derive(Clone, Copy, Debug)]
pub struct Calibration {
    pub iters_per_ms: usize,
    pub elapsed_ns: u64,
}

pub fn calibrate() -> Option<Calibration> {
    let begin = room::clock();
    let mut iterations = 0usize;
    let mut elapsed = 0u64;
    for _ in 0..MAX_CHUNKS {
        spin(CHUNK);
        iterations += CHUNK;
        elapsed = room::clock().checked_sub(begin)?;
        if elapsed >= 20_000_000 {
            break;
        }
    }
    if elapsed == 0 {
        return None;
    }
    Some(Calibration {
        iters_per_ms: usize::try_from((iterations as u128 * 1_000_000) / elapsed as u128)
            .ok()?
            .max(1),
        elapsed_ns: elapsed,
    })
}
