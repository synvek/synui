//! Focus management for keyboard navigation.

use crate::WidgetId;
use std::collections::HashMap;
use std::collections::HashSet;

/// Tracks focusable widgets and current focus.
#[derive(Debug, Default)]
pub struct FocusManager {
    focusable: HashSet<WidgetId>,
    focused: Option<WidgetId>,
    tab_order: Vec<WidgetId>,
    focus_trap: Option<Vec<WidgetId>>,
}

impl FocusManager {
    pub fn register(&mut self, id: WidgetId, focusable: bool) {
        if focusable {
            self.focusable.insert(id);
            if !self.tab_order.contains(&id) {
                self.tab_order.push(id);
            }
        } else {
            self.focusable.remove(&id);
            self.tab_order.retain(|&w| w != id);
            if self.focused == Some(id) {
                self.focused = None;
            }
        }
    }

    pub fn focused(&self) -> Option<WidgetId> {
        self.focused
    }

    pub fn focus(&mut self, id: WidgetId) -> bool {
        if self.focusable.contains(&id) {
            self.focused = Some(id);
            true
        } else {
            false
        }
    }

    pub fn clear_focus(&mut self) {
        self.focused = None;
    }

    /// Restrict Tab navigation to the given widget list (modal overlays).
    pub fn set_focus_trap(&mut self, trap: Option<Vec<WidgetId>>) {
        self.focus_trap = trap;
        if let Some(ref ids) = self.focus_trap {
            if let Some(&first) = ids.first() {
                if self.focusable.contains(&first) {
                    self.focused = Some(first);
                }
            }
        }
    }

    pub fn focus_trap(&self) -> Option<&[WidgetId]> {
        self.focus_trap.as_deref()
    }

    fn tab_cycle(&self, forward: bool) -> Option<WidgetId> {
        let order: &[WidgetId] = if let Some(trap) = self.focus_trap.as_ref() {
            if trap.is_empty() {
                return None;
            }
            trap.as_slice()
        } else if self.tab_order.is_empty() {
            return None;
        } else {
            self.tab_order.as_slice()
        };

        let next = match self.focused {
            None => {
                if forward {
                    order[0]
                } else {
                    *order.last()?
                }
            }
            Some(current) => {
                let idx = order.iter().position(|&id| id == current)?;
                let len = order.len();
                if forward {
                    order[(idx + 1) % len]
                } else {
                    order[(idx + len - 1) % len]
                }
            }
        };
        Some(next)
    }

    pub fn focus_next(&mut self) -> Option<WidgetId> {
        let next = self.tab_cycle(true)?;
        if self.focusable.contains(&next) {
            self.focused = Some(next);
            Some(next)
        } else {
            None
        }
    }

    pub fn focus_prev(&mut self) -> Option<WidgetId> {
        let prev = self.tab_cycle(false)?;
        if self.focusable.contains(&prev) {
            self.focused = Some(prev);
            Some(prev)
        } else {
            None
        }
    }

    pub fn remove(&mut self, id: WidgetId) {
        self.focusable.remove(&id);
        self.tab_order.retain(|&w| w != id);
        if self.focused == Some(id) {
            self.focused = None;
        }
        if let Some(ref mut trap) = self.focus_trap {
            trap.retain(|&w| w != id);
            if trap.is_empty() {
                self.focus_trap = None;
            }
        }
    }
}

/// Gesture recognizer — supports multi-pointer Tap/Drag and two-finger Pinch.
#[derive(Debug, Default)]
pub struct GestureRecognizer {
    active_pointers: HashMap<PointerId, crate::Point>,
    /// Initial distance between the first two pointers for pinch.
    pinch_start_dist: Option<f64>,
    threshold: f64,
}

use crate::event::{Gesture, PointerId};

impl GestureRecognizer {
    pub fn new() -> Self {
        Self {
            active_pointers: HashMap::new(),
            pinch_start_dist: None,
            threshold: 8.0,
        }
    }

    pub fn on_pointer_down(&mut self, id: PointerId, position: crate::Point) {
        self.active_pointers.insert(id, position);
        if self.active_pointers.len() >= 2 {
            self.pinch_start_dist = Some(self.pointer_distance());
        }
    }

    pub fn on_pointer_up(&mut self, id: PointerId, position: crate::Point) -> Option<Gesture> {
        let start = self.active_pointers.remove(&id)?;
        if self.active_pointers.len() < 2 {
            self.pinch_start_dist = None;
        }
        // Only emit Tap when no other pointers remain (single-finger).
        if !self.active_pointers.is_empty() {
            return None;
        }
        let dist = (position - start).hypot();
        if dist < self.threshold {
            Some(Gesture::Tap { position })
        } else {
            None
        }
    }

    pub fn on_pointer_move(&mut self, id: PointerId, position: crate::Point) -> Option<Gesture> {
        let start = *self.active_pointers.get(&id)?;
        self.active_pointers.insert(id, position);

        if self.active_pointers.len() >= 2 {
            let dist = self.pointer_distance();
            let start_dist = self.pinch_start_dist.unwrap_or(dist).max(1.0);
            let scale = (dist / start_dist) as f32;
            let center = self.pointer_center();
            return Some(Gesture::Pinch { scale, center });
        }

        let delta = position - start;
        if delta.hypot() >= self.threshold {
            Some(Gesture::Drag {
                start,
                current: position,
                delta,
            })
        } else {
            None
        }
    }

    pub fn on_pointer_cancel(&mut self, id: PointerId) {
        self.active_pointers.remove(&id);
        if self.active_pointers.len() < 2 {
            self.pinch_start_dist = None;
        }
    }

    pub fn on_scroll(&self, position: crate::Point, delta_x: f32, delta_y: f32) -> Gesture {
        Gesture::Scroll {
            position,
            delta_x,
            delta_y,
        }
    }

    fn pointer_distance(&self) -> f64 {
        let mut iter = self.active_pointers.values();
        let Some(a) = iter.next() else {
            return 0.0;
        };
        let Some(b) = iter.next() else {
            return 0.0;
        };
        (*a - *b).hypot()
    }

    fn pointer_center(&self) -> crate::Point {
        let mut iter = self.active_pointers.values();
        let a = iter.next().copied().unwrap_or(crate::Point::ZERO);
        let b = iter.next().copied().unwrap_or(a);
        crate::Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)
    }
}

#[cfg(test)]
mod pinch_tests {
    use super::*;
    use crate::Point;

    #[test]
    fn pinch_emits_scale() {
        let mut g = GestureRecognizer::new();
        g.on_pointer_down(0, Point::new(0.0, 0.0));
        g.on_pointer_down(1, Point::new(100.0, 0.0));
        let gesture = g.on_pointer_move(1, Point::new(200.0, 0.0));
        match gesture {
            Some(Gesture::Pinch { scale, .. }) => {
                assert!((scale - 2.0).abs() < 0.01);
            }
            other => panic!("expected Pinch, got {other:?}"),
        }
    }
}
