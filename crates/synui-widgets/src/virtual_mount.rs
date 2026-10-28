//! Unified virtual mount host (Phase 24): pool/recycle + nested scroll helpers.
//! Phase 26: virt host registry + dirty sync (no per-frame full-store ×4 walk).

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::Hash;

use synui_core::WidgetId;
use synui_core::WidgetStore;

/// Default pool caps (centralized).
pub const VSCROLL_POOL_CAP: usize = 48;
pub const LIST_POOL_CAP: usize = 64;
pub const TREE_POOL_CAP: usize = 64;
pub const TABLE_POOL_CAP: usize = 128;

/// Kind of virtualized host for dirty sync (Phase 26).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VirtHostKind {
    List,
    Tree,
    Table,
    Scroll,
}

thread_local! {
    static VIRT_HOSTS: RefCell<HashMap<WidgetId, VirtHostKind>> = RefCell::new(HashMap::new());
    static VIRT_DIRTY: RefCell<HashSet<WidgetId>> = RefCell::new(HashSet::new());
    static VIRT_LAST_STRUCTURE: Cell<u64> = Cell::new(u64::MAX);
    static VIRT_FORCE_ALL: Cell<bool> = Cell::new(true);
}

/// True when `id` is already in the virt host registry (tests / diagnostics).
pub fn virt_host_registered(id: WidgetId) -> bool {
    VIRT_HOSTS.with(|h| h.borrow().contains_key(&id))
}

/// Register a virtualized host widget.
pub fn register_virt_host(id: WidgetId, kind: VirtHostKind) {
    VIRT_HOSTS.with(|h| {
        h.borrow_mut().insert(id, kind);
    });
    mark_virt_host_dirty(id);
}

/// Mark one host for mount/recycle on the next sync.
pub fn mark_virt_host_dirty(id: WidgetId) {
    VIRT_DIRTY.with(|d| {
        d.borrow_mut().insert(id);
    });
}

/// Force a full rediscovery + sync of all virt hosts (structure changes).
pub fn mark_all_virt_dirty() {
    VIRT_FORCE_ALL.set(true);
}

/// Collect hosts that need mount sync this frame (Phase 26).
pub fn take_virt_sync_targets(store: &WidgetStore) -> Vec<(WidgetId, VirtHostKind)> {
    let ver = store.structure_version();
    let force = VIRT_FORCE_ALL.replace(false) || VIRT_LAST_STRUCTURE.get() != ver;
    VIRT_LAST_STRUCTURE.set(ver);
    if force {
        // Rediscover from store so hosts survive without explicit register.
        let ids = collect_all_widget_ids(store);
        VIRT_HOSTS.with(|h| {
            let mut map = h.borrow_mut();
            map.retain(|id, _| store.get(*id).is_some());
            for id in ids {
                if let Some(w) = store.get(id) {
                    if w.as_any()
                        .downcast_ref::<crate::list_view::ListView>()
                        .map(|lv| lv.uses_mount())
                        .unwrap_or(false)
                    {
                        map.insert(id, VirtHostKind::List);
                    } else if w
                        .as_any()
                        .downcast_ref::<crate::tree_view::TreeView>()
                        .map(|tv| tv.uses_mount())
                        .unwrap_or(false)
                    {
                        map.insert(id, VirtHostKind::Tree);
                    } else if w
                        .as_any()
                        .downcast_ref::<crate::data_table::DataTable>()
                        .map(|dt| dt.uses_mount())
                        .unwrap_or(false)
                    {
                        map.insert(id, VirtHostKind::Table);
                    } else if w
                        .as_any()
                        .downcast_ref::<crate::scroll::ScrollView>()
                        .map(|sv| sv.is_virtual() || sv.is_transparent_virt())
                        .unwrap_or(false)
                    {
                        map.insert(id, VirtHostKind::Scroll);
                    }
                }
            }
            map.iter().map(|(&i, &k)| (i, k)).collect()
        })
    } else {
        VIRT_DIRTY.with(|d| {
            let dirty = std::mem::take(&mut *d.borrow_mut());
            VIRT_HOSTS.with(|h| {
                let map = h.borrow();
                dirty
                    .into_iter()
                    .filter_map(|id| map.get(&id).copied().map(|k| (id, k)))
                    .filter(|(id, _)| store.get(*id).is_some())
                    .collect()
            })
        })
    }
}

/// Whether recycled widgets may be destroyed when the pool overflows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MountPoolPolicy {
    /// Slot-builder / List / Table: pool then `store.remove` on eviction.
    RecycleDestroy,
    /// Transparent virt: detach only; never destroy logical children.
    DetachOnly,
}

/// Shared mount window + recycle pool keyed by logical slot identity.
#[derive(Clone, Debug)]
pub struct VirtualMountHost<K: Eq + Hash + Clone> {
    pub mounted: Vec<(K, WidgetId)>,
    pub pool: HashMap<K, WidgetId>,
    pub pool_order: VecDeque<K>,
    pub pool_cap: usize,
}

impl<K: Eq + Hash + Clone> VirtualMountHost<K> {
    pub fn new(pool_cap: usize) -> Self {
        Self {
            mounted: Vec::new(),
            pool: HashMap::new(),
            pool_order: VecDeque::new(),
            pool_cap,
        }
    }

    pub fn mounted_count(&self) -> usize {
        self.mounted.len()
    }

    pub fn child_ids(&self) -> Vec<WidgetId> {
        self.mounted.iter().map(|(_, id)| *id).collect()
    }

    /// Reconcile the mount window to `desired` keys (order preserved).
    /// Returns widget ids that must be `store.remove`d by the caller.
    pub fn reconcile(
        &mut self,
        desired: &[K],
        policy: MountPoolPolicy,
        mut acquire: impl FnMut(&K) -> WidgetId,
    ) -> Vec<WidgetId> {
        reconcile_mount_window(
            &mut self.mounted,
            &mut self.pool,
            &mut self.pool_order,
            self.pool_cap,
            desired,
            policy,
            &mut acquire,
        )
    }
}

impl<K: Eq + Hash + Clone> Default for VirtualMountHost<K> {
    fn default() -> Self {
        Self::new(LIST_POOL_CAP)
    }
}

/// Reconcile mount window without owning a host struct (used by existing widgets).
/// Returns widget ids the caller must `store.remove`.
pub fn reconcile_mount_window<K: Eq + Hash + Clone>(
    mounted: &mut Vec<(K, WidgetId)>,
    pool: &mut HashMap<K, WidgetId>,
    pool_order: &mut VecDeque<K>,
    pool_cap: usize,
    desired: &[K],
    policy: MountPoolPolicy,
    acquire: &mut dyn FnMut(&K) -> WidgetId,
) -> Vec<WidgetId> {
    let desired_set: HashSet<K> = desired.iter().cloned().collect();
    let mut keep: Vec<(K, WidgetId)> = Vec::new();
    let mut to_pool: Vec<(K, WidgetId)> = Vec::new();
    for (key, wid) in mounted.drain(..) {
        if desired_set.contains(&key) {
            keep.push((key, wid));
        } else {
            to_pool.push((key, wid));
        }
    }

    let mut remove_ids: Vec<WidgetId> = Vec::new();
    match policy {
        MountPoolPolicy::DetachOnly => {
            let _ = to_pool;
        }
        MountPoolPolicy::RecycleDestroy => {
            for (key, wid) in to_pool {
                if let Some(old) = pool.insert(key.clone(), wid) {
                    remove_ids.push(old);
                }
                pool_order.retain(|k| k != &key);
                pool_order.push_back(key);
            }
            while pool.len() > pool_cap {
                if let Some(evict) = pool_order.pop_front() {
                    if let Some(wid) = pool.remove(&evict) {
                        remove_ids.push(wid);
                    }
                } else {
                    break;
                }
            }
        }
    }

    let keep_set: HashSet<K> = keep.iter().map(|(k, _)| k.clone()).collect();
    for key in desired {
        if keep_set.contains(key) {
            continue;
        }
        let wid = if matches!(policy, MountPoolPolicy::RecycleDestroy) {
            pool_order.retain(|k| k != key);
            if let Some(wid) = pool.remove(key) {
                wid
            } else {
                acquire(key)
            }
        } else {
            acquire(key)
        };
        keep.push((key.clone(), wid));
    }
    let order: HashMap<K, usize> = desired
        .iter()
        .enumerate()
        .map(|(i, k)| (k.clone(), i))
        .collect();
    keep.sort_by_key(|(k, _)| order.get(k).copied().unwrap_or(usize::MAX));
    *mounted = keep;
    remove_ids
}

/// DFS collect widget ids under `root` (inclusive).
pub fn collect_widget_ids(store: &WidgetStore, root: WidgetId) -> Vec<WidgetId> {
    let mut out = vec![root];
    for &c in store.children_of(root) {
        out.extend(collect_widget_ids(store, c));
    }
    out
}

/// Collect all widgets from the store root.
pub fn collect_all_widget_ids(store: &WidgetStore) -> Vec<WidgetId> {
    store
        .root()
        .into_iter()
        .flat_map(|r| collect_widget_ids(store, r))
        .collect()
}

/// Nested scroll chaining: true when viewport is at the edge for `delta_y`
/// (positive delta decreases scroll offset in SynUI scrollables).
pub fn scroll_at_edge(scroll_y: f64, max_scroll: f64, delta_y: f64) -> bool {
    let scrolling_toward_start = delta_y > 0.0;
    let scrolling_toward_end = delta_y < 0.0;
    (scrolling_toward_start && scroll_y <= 0.01)
        || (scrolling_toward_end && scroll_y >= (max_scroll - 0.01).max(0.0))
}

/// If focus was cleared because a recycled row was removed, re-focus `host`.
pub fn restore_host_focus_if_needed(
    store: &mut WidgetStore,
    host: WidgetId,
    host_wants_focus: bool,
) {
    if !host_wants_focus {
        return;
    }
    if store.focus_manager().focused().is_some() {
        return;
    }
    if store.get(host).map(|w| w.is_focusable()).unwrap_or(false) {
        store.focus_manager_mut().focus(host);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basic::Label;
    use synui_core::{Widget, WidgetStore};

    #[test]
    fn transparent_virt_registers_host() {
        use synui_core::Widget;
        let mut sv = crate::scroll::ScrollView::new();
        assert!(!virt_host_registered(sv.id()));
        sv.set_transparent_virt(true);
        assert!(virt_host_registered(sv.id()));
    }

    #[test]
    fn register_virt_host_at_create() {
        let lv = crate::list_view::ListView::new(10, 20.0);
        assert!(virt_host_registered(lv.id()));
        let tv = crate::tree_view::TreeView::new(vec![crate::tree_view::TreeNode::new("a", "A")]);
        assert!(virt_host_registered(tv.id()));
    }

    #[test]
    fn reconcile_bounds_mount_window() {
        let mut store = WidgetStore::new();
        let mut host = VirtualMountHost::<usize>::new(8);
        let desired: Vec<usize> = (0..10).collect();
        let remove = host.reconcile(&desired, MountPoolPolicy::RecycleDestroy, |&i| {
            store.insert(Label::new(format!("r{i}")))
        });
        assert!(remove.is_empty());
        assert_eq!(host.mounted_count(), 10);
        let desired2: Vec<usize> = (5..15).collect();
        let remove = host.reconcile(&desired2, MountPoolPolicy::RecycleDestroy, |&i| {
            store.insert(Label::new(format!("r{i}")))
        });
        for wid in remove {
            store.remove(wid);
        }
        assert_eq!(host.mounted_count(), 10);
        assert!(host.mounted.iter().all(|(i, _)| (5..15).contains(i)));
    }

    #[test]
    fn scroll_edge_chaining_detect() {
        assert!(scroll_at_edge(0.0, 100.0, 10.0));
        assert!(scroll_at_edge(100.0, 100.0, -10.0));
        assert!(!scroll_at_edge(50.0, 100.0, -10.0));
    }
}
