//! Cooperative alloc counter harness for steady-state paint budgets.

#![cfg(feature = "alloc_counter")]

use synui_core::alloc_counter;

#[test]
fn steady_state_paint_budget_is_zero_without_records() {
    alloc_counter::reset();
    // No intentional heap traffic recorded on a quiet frame.
    alloc_counter::assert_under(0);
}

#[test]
fn recorded_allocs_are_counted() {
    alloc_counter::reset();
    alloc_counter::record(3);
    assert_eq!(alloc_counter::count(), 3);
    alloc_counter::assert_under(3);
}
