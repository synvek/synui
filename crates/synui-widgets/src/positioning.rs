//! Post-layout sticky / fixed positioning (Phase 19 CSS subset).

use synui_core::{Position, Rect, Size, WidgetId, WidgetStore};

/// After layout + [`crate::apply_scroll_views`], correct sticky and fixed widgets.
pub fn apply_sticky_and_fixed(store: &mut WidgetStore) {
    let ids: Vec<WidgetId> = {
        let mut out = Vec::new();
        if let Some(root) = store.root() {
            collect(store, root, &mut out);
        }
        out
    };
    let viewport = Rect::new(0.0, 0.0, store.viewport().width, store.viewport().height);

    // Fixed first (viewport absolute), then sticky (depends on scrolled ancestors).
    for &id in &ids {
        let pos = store
            .get(id)
            .map(|w| w.layout_style().position)
            .unwrap_or(Position::Relative);
        if pos == Position::Fixed {
            apply_fixed(store, id, viewport);
        }
    }
    for &id in &ids {
        let pos = store
            .get(id)
            .map(|w| w.layout_style().position)
            .unwrap_or(Position::Relative);
        if pos == Position::Sticky {
            apply_sticky(store, id);
        }
    }
}

fn collect(store: &WidgetStore, id: WidgetId, out: &mut Vec<WidgetId>) {
    out.push(id);
    for &c in store.children_of(id) {
        collect(store, c, out);
    }
}

fn apply_fixed(store: &mut WidgetStore, id: WidgetId, viewport: Rect) {
    let (inset, size) = {
        let Some(w) = store.get(id) else {
            return;
        };
        let style = w.layout_style();
        let b = w.bounds();
        (
            style.inset,
            Size::new(b.width().max(1.0), b.height().max(1.0)),
        )
    };
    let top = inset[0].unwrap_or(0.0) as f64;
    let right = inset[1];
    let bottom = inset[2];
    let left = inset[3];

    let mut x0 = left.map(|v| v as f64).unwrap_or(viewport.x0);
    let mut y0 = top;
    if left.is_none() {
        if let Some(r) = right {
            x0 = viewport.x1 - r as f64 - size.width;
        }
    }
    if inset[0].is_none() {
        if let Some(b) = bottom {
            y0 = viewport.y1 - b as f64 - size.height;
        }
    }
    let x1 = x0 + size.width;
    let y1 = y0 + size.height;
    offset_to(store, id, Rect::new(x0, y0, x1, y1));
}

fn apply_sticky(store: &mut WidgetStore, id: WidgetId) {
    let (top_inset, bounds, parent_bounds, clip) = {
        let Some(w) = store.get(id) else {
            return;
        };
        let top = w.layout_style().inset[0].unwrap_or(0.0) as f64;
        let bounds = w.bounds();
        let parent_bounds = store
            .parent_of(id)
            .and_then(|p| store.get(p))
            .map(|p| p.bounds())
            .unwrap_or(bounds);
        let clip = nearest_clip_ancestor(store, id).unwrap_or(Rect::new(
            0.0,
            0.0,
            store.viewport().width,
            store.viewport().height,
        ));
        (top, bounds, parent_bounds, clip)
    };

    let h = bounds.height();
    let stick_at = clip.y0 + top_inset;
    let mut y = bounds.y0;
    if y < stick_at {
        y = stick_at;
    }
    // Stay within containing block (parent) so sticky releases at the end.
    let limit = parent_bounds.y1 - h;
    if y > limit {
        y = limit;
    }
    // Never push above the parent's top.
    y = y.max(parent_bounds.y0);

    if (y - bounds.y0).abs() > 0.01 {
        let dy = y - bounds.y0;
        offset_subtree(store, id, 0.0, dy);
    }
}

fn nearest_clip_ancestor(store: &WidgetStore, id: WidgetId) -> Option<Rect> {
    let mut cur = store.parent_of(id);
    while let Some(pid) = cur {
        if let Some(w) = store.get(pid) {
            if let Some(clip) = w.clip_bounds() {
                return Some(clip);
            }
        }
        cur = store.parent_of(pid);
    }
    None
}

fn offset_to(store: &mut WidgetStore, id: WidgetId, target: Rect) {
    let Some(w) = store.get(id) else {
        return;
    };
    let b = w.bounds();
    let dx = target.x0 - b.x0;
    let dy = target.y0 - b.y0;
    if dx.abs() < 0.01 && dy.abs() < 0.01 && (b.width() - target.width()).abs() < 0.01 {
        return;
    }
    // Resize root of fixed, then offset children relatively.
    if let Some(w) = store.get_mut(id) {
        w.set_bounds(target);
    }
    let kids: Vec<_> = store.children_of(id).to_vec();
    for c in kids {
        offset_subtree(store, c, dx, dy);
    }
}

fn offset_subtree(store: &mut WidgetStore, id: WidgetId, dx: f64, dy: f64) {
    if dx.abs() < 0.001 && dy.abs() < 0.001 {
        return;
    }
    if let Some(w) = store.get_mut(id) {
        let b = w.bounds();
        w.set_bounds(Rect::new(b.x0 + dx, b.y0 + dy, b.x1 + dx, b.y1 + dy));
    }
    let kids: Vec<_> = store.children_of(id).to_vec();
    for c in kids {
        offset_subtree(store, c, dx, dy);
    }
}

/// Sort children for paint (ascending z) or hit-test (call with reverse).
pub fn children_by_z(store: &WidgetStore, parent: WidgetId) -> Vec<WidgetId> {
    let mut kids: Vec<WidgetId> = store.children_of(parent).to_vec();
    kids.sort_by(|&a, &b| {
        let za = store
            .get(a)
            .and_then(|w| w.layout_style().z_index)
            .unwrap_or(0);
        let zb = store
            .get(b)
            .and_then(|w| w.layout_style().z_index)
            .unwrap_or(0);
        za.cmp(&zb).then_with(|| {
            // Stable document order for equal z.
            let ia = store.children_of(parent).iter().position(|&x| x == a);
            let ib = store.children_of(parent).iter().position(|&x| x == b);
            ia.cmp(&ib)
        })
    });
    kids
}

#[cfg(test)]
mod tests {
    use super::*;
    use synui_core::{Constraints, MeasureCtx, PaintCtx, Size, Widget};
    use synui_core::{Dimension, LayoutStyle, Position};
    use vello::Scene;

    struct BoxWidget {
        id: WidgetId,
        bounds: Rect,
        style: LayoutStyle,
        kids: Vec<WidgetId>,
    }

    impl BoxWidget {
        fn new(pos: Position) -> Self {
            let mut style = LayoutStyle::base();
            style.position = pos;
            style.width = Dimension::Length(100.0);
            style.height = Dimension::Length(40.0);
            if pos == Position::Sticky {
                style.inset[0] = Some(0.0);
            }
            Self {
                id: synui_core::alloc_widget_id(),
                bounds: Rect::ZERO,
                style,
                kids: Vec::new(),
            }
        }
    }

    impl Widget for BoxWidget {
        fn id(&self) -> WidgetId {
            self.id
        }
        fn children(&self) -> &[WidgetId] {
            &self.kids
        }
        fn layout_style(&self) -> &LayoutStyle {
            &self.style
        }
        fn bounds(&self) -> Rect {
            self.bounds
        }
        fn set_bounds(&mut self, bounds: Rect) {
            self.bounds = bounds;
        }
        fn measure(&mut self, _ctx: &mut MeasureCtx, _c: Constraints) -> Size {
            Size::new(100.0, 40.0)
        }
        fn paint(&self, _ctx: &mut PaintCtx, _scene: &mut Scene) {}
    }

    struct ClipParent {
        id: WidgetId,
        bounds: Rect,
        style: LayoutStyle,
        kids: Vec<WidgetId>,
    }

    impl ClipParent {
        fn new() -> Self {
            Self {
                id: synui_core::alloc_widget_id(),
                bounds: Rect::new(0.0, 0.0, 200.0, 200.0),
                style: LayoutStyle::base(),
                kids: Vec::new(),
            }
        }
    }

    impl Widget for ClipParent {
        fn id(&self) -> WidgetId {
            self.id
        }
        fn children(&self) -> &[WidgetId] {
            &self.kids
        }
        fn layout_style(&self) -> &LayoutStyle {
            &self.style
        }
        fn bounds(&self) -> Rect {
            self.bounds
        }
        fn set_bounds(&mut self, bounds: Rect) {
            self.bounds = bounds;
        }
        fn clip_bounds(&self) -> Option<Rect> {
            Some(self.bounds)
        }
        fn measure(&mut self, _ctx: &mut MeasureCtx, _c: Constraints) -> Size {
            Size::new(200.0, 200.0)
        }
        fn paint(&self, _ctx: &mut PaintCtx, _scene: &mut Scene) {}
    }

    #[test]
    fn sticky_pins_when_above_clip_top() {
        let mut store = WidgetStore::new();
        store.set_viewport(Size::new(400.0, 400.0));
        let mut parent = ClipParent::new();
        let mut sticky = BoxWidget::new(Position::Sticky);
        sticky.bounds = Rect::new(0.0, -20.0, 100.0, 20.0); // scrolled above clip
        let sid = store.insert(sticky);
        parent.kids = vec![sid];
        let pid = store.insert(parent);
        store.set_children(pid, vec![sid]);
        store.set_root(pid);

        apply_sticky_and_fixed(&mut store);
        let b = store.get(sid).unwrap().bounds();
        assert!((b.y0 - 0.0).abs() < 0.5, "sticky y0={}", b.y0);
    }

    #[test]
    fn fixed_uses_viewport_insets() {
        let mut store = WidgetStore::new();
        store.set_viewport(Size::new(400.0, 300.0));
        let mut fixed = BoxWidget::new(Position::Fixed);
        fixed.style.inset = [Some(10.0), None, None, Some(20.0)];
        fixed.bounds = Rect::new(0.0, 0.0, 100.0, 40.0);
        let id = store.insert(fixed);
        store.set_root(id);
        apply_sticky_and_fixed(&mut store);
        let b = store.get(id).unwrap().bounds();
        assert!((b.x0 - 20.0).abs() < 0.1);
        assert!((b.y0 - 10.0).abs() < 0.1);
    }

    #[test]
    fn children_sorted_by_z_index() {
        let mut store = WidgetStore::new();
        let mut a = BoxWidget::new(Position::Relative);
        a.style.z_index = Some(1);
        let mut b = BoxWidget::new(Position::Relative);
        b.style.z_index = Some(5);
        let mut c = BoxWidget::new(Position::Relative);
        c.style.z_index = Some(2);
        let aid = store.insert(a);
        let bid = store.insert(b);
        let cid = store.insert(c);
        let mut parent = BoxWidget::new(Position::Relative);
        parent.kids = vec![aid, bid, cid];
        let pid = store.insert(parent);
        store.set_children(pid, vec![aid, bid, cid]);
        store.set_root(pid);
        let sorted = children_by_z(&store, pid);
        assert_eq!(sorted, vec![aid, cid, bid]);
    }
}
