//! Optional allocation counter for steady-state frame budgets (Sprint R).
//!
//! Enable with `--features alloc_counter`. Hot paths can call [`record`] to
//! count intentional heap traffic; paint should stay near zero in idle frames.
//! This is a cooperative counter (not a global allocator hook).

use std::cell::Cell;

thread_local! {
    static ALLOCS: Cell<u64> = const { Cell::new(0) };
}

/// Reset the counter (call at frame start in debug harnesses).
pub fn reset() {
    ALLOCS.with(|c| c.set(0));
}

/// Record `n` allocations (or approximate heap ops).
pub fn record(n: u64) {
    ALLOCS.with(|c| c.set(c.get().saturating_add(n)));
}

/// Current count since last [`reset`].
pub fn count() -> u64 {
    ALLOCS.with(|c| c.get())
}

/// Assert that steady-state work stayed under `max` recorded allocs.
pub fn assert_under(max: u64) {
    let n = count();
    assert!(
        n <= max,
        "alloc_counter: expected ≤{max} recorded allocs, got {n}"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counter_basic() {
        reset();
        record(2);
        record(3);
        assert_eq!(count(), 5);
        assert_under(5);
        reset();
        assert_eq!(count(), 0);
    }
}
