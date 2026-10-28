//! Animation tick helpers for widget-level effects (ripple, progress, lottie).

use crate::{
    Button, CircularProgress, LinearProgress, Lottie, MorphingShape, RichEdit, TextArea, TextField,
};
use synui_core::{WidgetId, WidgetStore};

/// Advance per-widget animations; returns ids that need a paint (Phase 30 scoped damage).
pub fn tick_animations(store: &mut WidgetStore, dt_ms: f32) -> Vec<WidgetId> {
    tick_animations_ex(store, dt_ms, dt_ms)
}

/// Like [`tick_animations`], but caret blink can use a separate (often uncapped) dt
/// so a single post-`WaitUntil` frame can cross the blink threshold.
pub fn tick_animations_ex(
    store: &mut WidgetStore,
    anim_dt_ms: f32,
    cursor_dt_ms: f32,
) -> Vec<WidgetId> {
    let mut dirty = Vec::new();
    for widget in store.widgets_mut() {
        let id = widget.id();
        let mut changed = false;
        if let Some(btn) = widget.as_any_mut().downcast_mut::<Button>() {
            changed = btn.advance_ripple(anim_dt_ms);
        } else if let Some(lp) = widget.as_any_mut().downcast_mut::<LinearProgress>() {
            changed = lp.advance_animation(anim_dt_ms);
        } else if let Some(cp) = widget.as_any_mut().downcast_mut::<CircularProgress>() {
            changed = cp.advance_animation(anim_dt_ms);
        } else if let Some(lottie) = widget.as_any_mut().downcast_mut::<Lottie>() {
            changed = lottie.advance(anim_dt_ms / 1000.0);
        } else if let Some(morph) = widget.as_any_mut().downcast_mut::<MorphingShape>() {
            changed = morph.advance(anim_dt_ms);
        } else if let Some(field) = widget.as_any_mut().downcast_mut::<TextField>() {
            changed = field.advance_cursor(cursor_dt_ms);
        } else if let Some(area) = widget.as_any_mut().downcast_mut::<TextArea>() {
            changed = area.advance_cursor(cursor_dt_ms);
        } else if let Some(rich) = widget.as_any_mut().downcast_mut::<RichEdit>() {
            changed = rich.advance_cursor(cursor_dt_ms);
        } else if let Some(snack) = widget.as_any_mut().downcast_mut::<crate::SnackBar>() {
            changed = snack.tick(anim_dt_ms);
        } else if let Some(item) = widget.as_any_mut().downcast_mut::<crate::ListItem>() {
            changed = item.advance_ripple(anim_dt_ms);
        }
        if changed {
            dirty.push(id);
        }
    }
    dirty
}

/// Soonest caret-blink wake (ms) among focused text editors, if any.
pub fn next_cursor_blink_wake_ms(store: &WidgetStore) -> Option<f32> {
    let mut best: Option<f32> = None;
    for widget in store.widgets() {
        let wake = if let Some(field) = widget.as_any().downcast_ref::<TextField>() {
            field.cursor_blink_wake_ms()
        } else if let Some(area) = widget.as_any().downcast_ref::<TextArea>() {
            area.cursor_blink_wake_ms()
        } else if let Some(rich) = widget.as_any().downcast_ref::<RichEdit>() {
            rich.cursor_blink_wake_ms()
        } else {
            None
        };
        if let Some(ms) = wake {
            best = Some(best.map_or(ms, |b| b.min(ms)));
        }
    }
    best
}

/// True when any animation still needs frames (non-empty dirty from last tick, or convenience).
pub fn tick_animations_needs_redraw(store: &mut WidgetStore, dt_ms: f32) -> bool {
    !tick_animations(store, dt_ms).is_empty()
}
