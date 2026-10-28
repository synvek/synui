//! Shared virtualization math for ListView / TreeView / DataTable (Phase 17).

/// Default overscan rows above/below the viewport.
pub const OVERSCAN: usize = 4;

/// Fixed-height or prefix-sum variable-height scroll geometry.
#[derive(Clone, Debug, Default)]
pub struct VirtualViewport {
    pub item_count: usize,
    pub item_extent: f64,
    pub scroll_y: f64,
    pub viewport_height: f64,
    height_cache: Vec<f64>,
    prefix: Vec<f64>,
}

impl VirtualViewport {
    pub fn new(item_count: usize, item_extent: f64) -> Self {
        Self {
            item_count,
            item_extent: item_extent.max(1.0),
            scroll_y: 0.0,
            viewport_height: 0.0,
            height_cache: Vec::new(),
            prefix: Vec::new(),
        }
    }

    pub fn clear_variable_heights(&mut self) {
        self.height_cache.clear();
        self.prefix.clear();
    }

    pub fn has_variable_heights(&self) -> bool {
        self.prefix.len() == self.item_count + 1 && self.item_count > 0
    }

    /// Rebuild prefix sums from an explicit height table.
    pub fn set_heights(&mut self, heights: Vec<f64>) {
        self.item_count = heights.len();
        self.height_cache = heights;
        self.prefix.clear();
        self.prefix.reserve(self.item_count + 1);
        self.prefix.push(0.0);
        for i in 0..self.item_count {
            let h = self.height_cache[i].max(1.0);
            self.height_cache[i] = h;
            let next = self.prefix[i] + h;
            self.prefix.push(next);
        }
    }

    /// Rebuild from a per-index extent function.
    pub fn rebuild_from_fn(&mut self, f: &dyn Fn(usize) -> f64) {
        let mut heights = Vec::with_capacity(self.item_count);
        for i in 0..self.item_count {
            heights.push(f(i).max(1.0));
        }
        self.set_heights(heights);
    }

    /// Ensure cache length matches `item_count` using `item_extent` fill for missing.
    pub fn ensure_measured_slots(&mut self) {
        if self.height_cache.len() > self.item_count {
            self.height_cache.truncate(self.item_count);
        }
        while self.height_cache.len() < self.item_count {
            self.height_cache.push(self.item_extent);
        }
        self.rebuild_prefix_from_cache();
    }

    pub fn set_measured_height(&mut self, index: usize, height: f64) {
        if index >= self.item_count {
            return;
        }
        self.ensure_measured_slots();
        self.height_cache[index] = height.max(1.0);
        self.rebuild_prefix_from_cache();
    }

    fn rebuild_prefix_from_cache(&mut self) {
        self.prefix.clear();
        self.prefix.reserve(self.height_cache.len() + 1);
        self.prefix.push(0.0);
        for (i, h) in self.height_cache.iter().enumerate() {
            let next = self.prefix[i] + h.max(1.0);
            self.prefix.push(next);
        }
    }

    pub fn row_extent(&self, index: usize) -> f64 {
        self.height_cache
            .get(index)
            .copied()
            .unwrap_or(self.item_extent)
    }

    pub fn offset_of(&self, index: usize) -> f64 {
        if self.has_variable_heights() {
            *self.prefix.get(index).unwrap_or(&self.content_height())
        } else {
            index as f64 * self.item_extent
        }
    }

    pub fn content_height(&self) -> f64 {
        // Only trust prefix when it matches item_count; a stale/partial prefix
        // under-reports height so scroll_y/max_scroll disagree and the thumb
        // leaves the track.
        if self.has_variable_heights() {
            self.prefix.last().copied().unwrap_or(0.0)
        } else {
            self.item_count as f64 * self.item_extent
        }
    }

    pub fn max_scroll(&self) -> f64 {
        (self.content_height() - self.viewport_height).max(0.0)
    }

    pub fn clamp_scroll(&mut self) {
        let max = self.max_scroll();
        self.scroll_y = self.scroll_y.clamp(0.0, max);
    }

    /// Inclusive start / exclusive end of visible (+ overscan) indices.
    pub fn visible_range(&self) -> (usize, usize) {
        if self.item_count == 0 {
            return (0, 0);
        }
        if self.has_variable_heights() {
            let mut lo = 0usize;
            let mut hi = self.item_count;
            while lo < hi {
                let mid = (lo + hi) / 2;
                if self.prefix[mid + 1] <= self.scroll_y {
                    lo = mid + 1;
                } else {
                    hi = mid;
                }
            }
            let first = lo.saturating_sub(OVERSCAN);
            let view_end = self.scroll_y + self.viewport_height;
            let mut last = first;
            while last < self.item_count && self.prefix[last] < view_end {
                last += 1;
            }
            last = (last + OVERSCAN).min(self.item_count);
            (first, last)
        } else {
            let first = (self.scroll_y / self.item_extent).floor() as usize;
            let first = first.saturating_sub(OVERSCAN);
            let visible = (self.viewport_height / self.item_extent).ceil() as usize + 1;
            let last = (first + visible + OVERSCAN * 2).min(self.item_count);
            (first, last)
        }
    }

    pub fn index_at_offset(&self, local_y: f64) -> Option<usize> {
        if self.item_count == 0 {
            return None;
        }
        let local = local_y.max(0.0);
        if self.has_variable_heights() {
            let mut lo = 0usize;
            let mut hi = self.item_count;
            while lo < hi {
                let mid = (lo + hi) / 2;
                if self.prefix[mid + 1] <= local {
                    lo = mid + 1;
                } else {
                    hi = mid;
                }
            }
            Some(lo.min(self.item_count.saturating_sub(1)))
        } else {
            let idx = (local / self.item_extent).floor() as usize;
            Some(idx.min(self.item_count.saturating_sub(1)))
        }
    }

    pub fn ensure_index_visible(&mut self, index: usize) {
        if index >= self.item_count {
            return;
        }
        let top = self.offset_of(index);
        let bottom = top + self.row_extent(index);
        if top < self.scroll_y {
            self.scroll_y = top;
        } else if bottom > self.scroll_y + self.viewport_height {
            self.scroll_y = bottom - self.viewport_height;
        }
        self.clamp_scroll();
    }
}

thread_local! {
    /// Pooled row layout engine — avoids per-row `LayoutEngine::new` on virt sync (Phase 24).
    static ROW_LAYOUT_ENGINE: std::cell::RefCell<synui_layout::LayoutEngine> =
        std::cell::RefCell::new(synui_layout::LayoutEngine::new());
}

/// Layout a detached widget subtree into `available`, then translate to `origin`.
pub fn layout_row_subtree(
    store: &mut synui_core::WidgetStore,
    text: &mut dyn synui_core::TextMeasurer,
    row_root: synui_core::WidgetId,
    origin: synui_core::Point,
    available: synui_core::Size,
) {
    ROW_LAYOUT_ENGINE.with(|engine| {
        engine
            .borrow_mut()
            .compute(store, text, row_root, available);
    });
    offset_subtree(store, row_root, origin.x, origin.y);
}

fn offset_subtree(store: &mut synui_core::WidgetStore, id: synui_core::WidgetId, dx: f64, dy: f64) {
    if let Some(w) = store.get_mut(id) {
        let b = w.bounds();
        w.set_bounds(synui_core::Rect::new(
            b.x0 + dx,
            b.y0 + dy,
            b.x1 + dx,
            b.y1 + dy,
        ));
    }
    let kids: Vec<_> = store.children_of(id).to_vec();
    for c in kids {
        offset_subtree(store, c, dx, dy);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_visible_bounded() {
        let mut vp = VirtualViewport::new(10_000, 32.0);
        vp.viewport_height = 320.0;
        let (a, b) = vp.visible_range();
        assert!(b - a <= 40);
        assert!(b - a >= 8);
    }

    #[test]
    fn variable_prefix() {
        let mut vp = VirtualViewport::new(5, 20.0);
        vp.rebuild_from_fn(&|i| if i % 2 == 0 { 20.0 } else { 40.0 });
        assert_eq!(vp.content_height(), 140.0);
        assert_eq!(vp.offset_of(2), 60.0);
    }

    #[test]
    fn content_height_ignores_stale_partial_prefix() {
        let mut vp = VirtualViewport::new(2, 32.0);
        vp.ensure_measured_slots();
        assert!(vp.has_variable_heights());
        // Grow item_count without rebuilding heights — old code used prefix.last()
        // and under-reported content height so scroll/thumb disagreed with the list.
        vp.item_count = 10_000;
        assert!(!vp.has_variable_heights());
        assert_eq!(vp.content_height(), 10_000.0 * 32.0);
        vp.viewport_height = 320.0;
        assert!(
            (vp.max_scroll() - (10_000.0 * 32.0 - 320.0)).abs() < 1.0,
            "max_scroll must use full fixed-extent height, got {}",
            vp.max_scroll()
        );
    }
}
