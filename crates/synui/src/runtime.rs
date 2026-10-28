//! Private, App-scoped resource and post-queue runtime.

use crate::app::{AppContext, AppError};
use std::any::Any;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::ThreadId;

// This counter creates identities only. It is never a global resource table, so a
// destroyed context cannot be recovered through a later context with the same API.
static NEXT_CONTEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct ContextId(pub(crate) u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecycleState {
    Active,
    Draining,
    Destroyed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) enum ResourceKind {
    State,
    Action,
    Model,
}

/// The complete ownership identity carried by every runtime resource handle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ResourceKey {
    pub(crate) context_id: ContextId,
    pub(crate) generation: u64,
    pub(crate) slot: u64,
    pub(crate) kind: ResourceKind,
}

pub(crate) enum ResourceRecord {
    State(Box<dyn Any + Send>),
    Action(Box<dyn Any + Send>),
    Model(Box<dyn Any + Send>),
    ModelInUse,
}

/// The sole ownership root for the resources of one App context.
pub(crate) struct ContextRecord {
    pub(crate) id: ContextId,
    pub(crate) generation: u64,
    pub(crate) lifecycle: LifecycleState,
    pub(crate) ui_thread: ThreadId,
    pub(crate) next_slot: u64,
    pub(crate) resources: HashMap<u64, ResourceRecord>,
    // Store the allocated kind separately from the erased value so validation is
    // always against all four ResourceKey fields, not only a typed downcast.
    resource_kinds: HashMap<u64, ResourceKind>,
    scheduler: VecDeque<PostedWork>,
    // Optional host callback used to wake the native event loop after a post is queued.
    post_waker: Option<PostWaker>,
    // Work that has passed the final Active check and is executing on the UI
    // thread. Shutdown retains resources until this reaches zero.
    running_posts: usize,
    // Incremented by App-owned State writes so the facade host can rebuild only
    // after application data changes, not for input/paint-only redraws.
    pub(crate) rebuild_revision: u64,
}

pub(crate) struct ContextInner {
    pub(crate) id: ContextId,
    pub(crate) ui_thread: ThreadId,
    pub(crate) record: Mutex<ContextRecord>,
}

pub(crate) type PostCallback = Box<dyn FnOnce(AppContext) + Send + 'static>;
pub(crate) type PostWaker = Arc<dyn Fn() + Send + Sync + 'static>;

struct PostedWork {
    token: Arc<PostTokenInner>,
    callback: Option<PostCallback>,
}

const POST_ACCEPTED: u8 = 0;
const POST_RUNNING: u8 = 1;
const POST_EXECUTED: u8 = 2;
const POST_CANCELLED: u8 = 3;

/// Terminal or pending state of a posted UI operation.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum PostStatus {
    /// The operation is queued or has been committed for UI-thread execution.
    Accepted,
    /// The callback returned after running on the owning UI thread.
    Executed,
    /// Shutdown or the caller cancelled the operation before it began.
    Cancelled,
}

pub(crate) struct PostTokenInner {
    state: AtomicU8,
}

impl PostTokenInner {
    fn new() -> Self {
        Self {
            state: AtomicU8::new(POST_ACCEPTED),
        }
    }

    pub(crate) fn status(&self) -> PostStatus {
        match self.state.load(Ordering::Acquire) {
            POST_EXECUTED => PostStatus::Executed,
            POST_CANCELLED => PostStatus::Cancelled,
            // A running operation remains externally accepted until its callback
            // returns, preserving the stable accepted/executed/cancelled contract.
            POST_ACCEPTED | POST_RUNNING => PostStatus::Accepted,
            _ => unreachable!("post token state is always valid"),
        }
    }

    pub(crate) fn cancel(&self) -> bool {
        self.state
            .compare_exchange(
                POST_ACCEPTED,
                POST_CANCELLED,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    /// Atomically commits an accepted operation to execution. Once this succeeds,
    /// shutdown lets the callback finish before releasing context resources.
    fn begin_execution(&self) -> bool {
        self.state
            .compare_exchange(
                POST_ACCEPTED,
                POST_RUNNING,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    fn finish_execution(&self) {
        self.state.store(POST_EXECUTED, Ordering::Release);
    }
}

/// Keeps the context alive until a callback that passed the final Active check
/// exits, including when that callback unwinds.
struct PostExecutionGuard {
    inner: Arc<ContextInner>,
    token: Arc<PostTokenInner>,
}

impl Drop for PostExecutionGuard {
    fn drop(&mut self) {
        self.token.finish_execution();
        let mut record = self.inner.record.lock().expect("context lock poisoned");
        debug_assert!(record.running_posts > 0);
        record.running_posts -= 1;
        if record.lifecycle == LifecycleState::Draining && record.running_posts == 0 {
            finalize_destroyed(&mut record);
        }
    }
}

pub(crate) fn new_context() -> Arc<ContextInner> {
    let id = ContextId(NEXT_CONTEXT_ID.fetch_add(1, Ordering::Relaxed));
    let ui_thread = std::thread::current().id();
    Arc::new(ContextInner {
        id,
        ui_thread,
        record: Mutex::new(ContextRecord {
            id,
            generation: 1,
            lifecycle: LifecycleState::Active,
            ui_thread,
            next_slot: 0,
            resources: HashMap::new(),
            resource_kinds: HashMap::new(),
            scheduler: VecDeque::new(),
            post_waker: None,
            running_posts: 0,
            rebuild_revision: 0,
        }),
    })
}

/// Returns the revision incremented by successful App-owned State writes.
pub(crate) fn rebuild_revision(inner: &Arc<ContextInner>) -> u64 {
    inner
        .record
        .lock()
        .expect("context lock poisoned")
        .rebuild_revision
}

pub(crate) fn lifecycle(inner: &Arc<ContextInner>) -> LifecycleState {
    let record = inner.record.lock().expect("context lock poisoned");
    debug_assert_eq!(record.id, inner.id);
    record.lifecycle
}

pub(crate) fn ensure_active_on_ui_thread(inner: &Arc<ContextInner>) -> Result<(), AppError> {
    let record = inner.record.lock().expect("context lock poisoned");
    debug_assert_eq!(record.id, inner.id);
    if std::thread::current().id() != record.ui_thread {
        return Err(AppError::ThreadAffinity);
    }
    match record.lifecycle {
        LifecycleState::Active => Ok(()),
        LifecycleState::Draining | LifecycleState::Destroyed => Err(AppError::ContextDestroyed),
    }
}

pub(crate) fn ensure_active(inner: &Arc<ContextInner>) -> Result<(), AppError> {
    match lifecycle(inner) {
        LifecycleState::Active => Ok(()),
        LifecycleState::Draining | LifecycleState::Destroyed => Err(AppError::ContextDestroyed),
    }
}

pub(crate) fn context_matches(
    owner: &Arc<ContextInner>,
    supplied: &Arc<ContextInner>,
) -> Result<(), AppError> {
    if owner.id != supplied.id {
        return Err(AppError::ContextMismatch);
    }
    Ok(())
}

pub(crate) fn insert_resource(
    inner: &Arc<ContextInner>,
    kind: ResourceKind,
    resource: ResourceRecord,
) -> Result<ResourceKey, AppError> {
    ensure_active_on_ui_thread(inner)?;
    let mut record = inner.record.lock().expect("context lock poisoned");
    if record.lifecycle != LifecycleState::Active {
        return Err(AppError::ContextDestroyed);
    }

    let slot = record.next_slot;
    record.next_slot = record
        .next_slot
        .checked_add(1)
        .ok_or(AppError::StaleHandle)?;
    let generation = record.generation;
    record.resource_kinds.insert(slot, kind);
    record.resources.insert(slot, resource);
    Ok(ResourceKey {
        context_id: inner.id,
        generation,
        slot,
        kind,
    })
}

/// Validates context, generation, slot, and kind before any resource side effect.
pub(crate) fn validate_resource(
    inner: &Arc<ContextInner>,
    key: ResourceKey,
    expected: ResourceKind,
    require_ui_thread: bool,
) -> Result<(), AppError> {
    if key.context_id != inner.id {
        return Err(AppError::ContextMismatch);
    }
    if require_ui_thread && std::thread::current().id() != inner.ui_thread {
        return Err(AppError::ThreadAffinity);
    }

    let record = inner.record.lock().expect("context lock poisoned");
    match record.lifecycle {
        LifecycleState::Active => {}
        LifecycleState::Draining | LifecycleState::Destroyed => {
            return Err(AppError::ContextDestroyed)
        }
    }
    if key.generation != record.generation {
        return Err(AppError::StaleHandle);
    }
    match record.resource_kinds.get(&key.slot) {
        Some(kind)
            if *kind == key.kind
                && *kind == expected
                && record.resources.contains_key(&key.slot) =>
        {
            Ok(())
        }
        _ => Err(AppError::StaleHandle),
    }
}

/// Removes one resource only after validating every ownership field while holding
/// the resource-table lock. Validation and removal therefore form one operation.
pub(crate) fn remove_resource(
    inner: &Arc<ContextInner>,
    key: ResourceKey,
    expected: ResourceKind,
) -> Result<ResourceRecord, AppError> {
    if key.context_id != inner.id {
        return Err(AppError::ContextMismatch);
    }
    if std::thread::current().id() != inner.ui_thread {
        return Err(AppError::ThreadAffinity);
    }

    let mut record = inner.record.lock().expect("context lock poisoned");
    match record.lifecycle {
        LifecycleState::Active => {}
        LifecycleState::Draining | LifecycleState::Destroyed => {
            return Err(AppError::ContextDestroyed)
        }
    }
    if key.generation != record.generation {
        return Err(AppError::StaleHandle);
    }
    match record.resource_kinds.get(&key.slot) {
        Some(kind)
            if *kind == key.kind
                && *kind == expected
                && record.resources.contains_key(&key.slot) => {}
        _ => return Err(AppError::StaleHandle),
    }

    let removed_kind = record
        .resource_kinds
        .remove(&key.slot)
        .expect("validated resource kind must exist");
    debug_assert_eq!(removed_kind, expected);
    record
        .resources
        .remove(&key.slot)
        .ok_or(AppError::StaleHandle)
}

/// Completes destruction only after every callback that was committed to run has
/// returned. The caller holds `record`'s mutex.
fn finalize_destroyed(record: &mut ContextRecord) {
    debug_assert_eq!(record.lifecycle, LifecycleState::Draining);
    debug_assert_eq!(record.running_posts, 0);
    debug_assert!(record.scheduler.is_empty());
    record.resources.clear();
    record.resource_kinds.clear();
    record.generation = record.generation.wrapping_add(1);
    record.lifecycle = LifecycleState::Destroyed;
}

/// Starts shutdown. Queued work is cancelled; a callback that has already passed
/// the final Active check completes while resources remain retained. Its guard
/// performs the eventual Draining -> Destroyed transition.
pub(crate) fn destroy_context(inner: &Arc<ContextInner>) {
    let mut record = inner.record.lock().expect("context lock poisoned");
    match record.lifecycle {
        LifecycleState::Destroyed => return,
        LifecycleState::Active => {
            record.lifecycle = LifecycleState::Draining;
            for work in &record.scheduler {
                work.token.cancel();
            }
            record.scheduler.clear();
        }
        LifecycleState::Draining => {}
    }

    if record.running_posts == 0 {
        finalize_destroyed(&mut record);
    }
}

pub(crate) fn set_post_waker(inner: &Arc<ContextInner>, waker: PostWaker) {
    let mut record = inner.record.lock().expect("context lock poisoned");
    if record.lifecycle == LifecycleState::Active {
        record.post_waker = Some(waker);
    }
}

pub(crate) fn submit_post(
    inner: &Arc<ContextInner>,
    callback: PostCallback,
) -> Result<Arc<PostTokenInner>, AppError> {
    let (token, waker) = {
        let mut record = inner.record.lock().expect("context lock poisoned");
        if record.lifecycle != LifecycleState::Active {
            return Err(AppError::ContextDestroyed);
        }
        let token = Arc::new(PostTokenInner::new());
        record.scheduler.push_back(PostedWork {
            token: token.clone(),
            callback: Some(callback),
        });
        (token, record.post_waker.clone())
    };

    // Wake after releasing the runtime lock so the host callback can safely
    // submit or inspect application state without lock-order constraints.
    if let Some(waker) = waker {
        waker();
    }
    Ok(token)
}

/// Removes the next non-cancelled callback only while the context is still
/// Active. The state check and acceptance-to-running transition share the
/// context lock, so shutdown cannot slip between this final check and commitment
/// to execute the callback.
fn take_next_active_post(inner: &Arc<ContextInner>) -> Option<PostedWork> {
    let mut record = inner.record.lock().expect("context lock poisoned");
    if record.lifecycle != LifecycleState::Active {
        return None;
    }

    while let Some(work) = record.scheduler.pop_front() {
        if work.token.begin_execution() {
            record.running_posts += 1;
            return Some(work);
        }
    }
    None
}

pub(crate) fn run_posts(
    inner: &Arc<ContextInner>,
    context: &AppContext,
) -> Result<usize, AppError> {
    ensure_active_on_ui_thread(inner)?;
    let mut executed = 0;
    while let Some(mut work) = take_next_active_post(inner) {
        let callback = work
            .callback
            .take()
            .expect("queued posts always have a callback");
        let _execution = PostExecutionGuard {
            inner: inner.clone(),
            token: work.token,
        };
        callback(context.clone());
        executed += 1;
    }
    Ok(executed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_key_validation_includes_context_generation_slot_and_kind() {
        let context = new_context();
        let key = insert_resource(
            &context,
            ResourceKind::State,
            ResourceRecord::State(Box::new(7_u8)),
        )
        .unwrap();

        assert_eq!(
            validate_resource(&context, key, ResourceKind::State, true),
            Ok(())
        );
        let wrong_kind = ResourceKey {
            kind: ResourceKind::Action,
            ..key
        };
        assert_eq!(
            validate_resource(&context, wrong_kind, ResourceKind::Action, true),
            Err(AppError::StaleHandle)
        );

        let other = new_context();
        assert_eq!(
            validate_resource(&other, key, ResourceKind::State, true),
            Err(AppError::ContextMismatch)
        );

        destroy_context(&context);
        assert_eq!(
            validate_resource(&context, key, ResourceKind::State, true),
            Err(AppError::ContextDestroyed)
        );
    }

    #[test]
    fn strict_remove_validates_before_mutation_and_rejects_reuse() {
        let context = new_context();
        let key = insert_resource(
            &context,
            ResourceKind::Action,
            ResourceRecord::Action(Box::new(7_u8)),
        )
        .unwrap();
        let wrong_kind = ResourceKey {
            kind: ResourceKind::State,
            ..key
        };

        assert!(matches!(
            remove_resource(&context, wrong_kind, ResourceKind::State),
            Err(AppError::StaleHandle)
        ));
        assert!(context
            .record
            .lock()
            .unwrap()
            .resources
            .contains_key(&key.slot));
        assert!(matches!(
            remove_resource(&context, key, ResourceKind::Action),
            Ok(ResourceRecord::Action(_))
        ));
        assert_eq!(
            remove_resource(&context, key, ResourceKind::Action).err(),
            Some(AppError::StaleHandle)
        );
    }

    #[test]
    fn context_record_keeps_its_identity_and_ui_thread() {
        let context = new_context();
        let record = context.record.lock().unwrap();
        assert_eq!(record.id, context.id);
        assert_eq!(record.ui_thread, context.ui_thread);
        assert_eq!(record.lifecycle, LifecycleState::Active);
    }
}
