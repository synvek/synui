//! Layout dirty tracking for incremental relayout.

use std::collections::{HashMap, HashSet};
use synui_core::{Size, WidgetId};

/// Tracks which widgets need relayout; propagates dirty flags up the tree.
///
/// Structure changes (insert/remove/reorder) bump `structure_version` and clear
/// size caches. Size-only dirtiness uses `dirty` + upward propagation.
#[derive(Debug, Default)]
pub struct LayoutCache {
    dirty: HashSet<WidgetId>,
    structure_version: u64,
    /// Cached leaf measure results; skipped when the leaf is not dirty.
    leaf_sizes: HashMap<WidgetId, Size>,
    last_available: Option<Size>,
}

impl LayoutCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn mark_dirty(&mut self, id: WidgetId) {
        self.dirty.insert(id);
        self.leaf_sizes.remove(&id);
    }

    pub fn mark_structure_changed(&mut self) {
        self.structure_version = self.structure_version.wrapping_add(1);
        self.dirty.clear();
        self.leaf_sizes.clear();
        self.last_available = None;
    }

    pub fn is_dirty(&self, id: WidgetId) -> bool {
        self.dirty.contains(&id)
    }

    pub fn any_dirty(&self) -> bool {
        !self.dirty.is_empty()
    }

    pub fn clear(&mut self) {
        self.dirty.clear();
    }

    pub fn structure_version(&self) -> u64 {
        self.structure_version
    }

    pub fn cached_leaf_size(&self, id: WidgetId) -> Option<Size> {
        self.leaf_sizes.get(&id).copied()
    }

    pub fn store_leaf_size(&mut self, id: WidgetId, size: Size) {
        self.leaf_sizes.insert(id, size);
    }

    /// Drop cached measures without bumping structure (prop-only changes).
    pub fn invalidate_leaf_sizes(&mut self) {
        self.leaf_sizes.clear();
    }

    pub fn set_last_available(&mut self, size: Size) {
        self.last_available = Some(size);
    }

    pub fn last_available(&self) -> Option<Size> {
        self.last_available
    }

    /// Propagate dirty flag from widget to ancestors.
    pub fn propagate_dirty(
        &mut self,
        id: WidgetId,
        parent_of: impl Fn(WidgetId) -> Option<WidgetId>,
    ) {
        let mut current = Some(id);
        while let Some(widget_id) = current {
            self.dirty.insert(widget_id);
            self.leaf_sizes.remove(&widget_id);
            current = parent_of(widget_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn propagate_marks_ancestors() {
        let mut cache = LayoutCache::new();
        // Use alloc-free synthetic ids via transmute-safe constructor if available.
        let child = WidgetId(3);
        let parent = WidgetId(2);
        let root = WidgetId(1);
        cache.propagate_dirty(child, |id| {
            if id == child {
                Some(parent)
            } else if id == parent {
                Some(root)
            } else {
                None
            }
        });
        assert!(cache.is_dirty(child));
        assert!(cache.is_dirty(parent));
        assert!(cache.is_dirty(root));
    }
}
