//! Stable public App-context APIs.
//!
//! All stateful resources are owned by one [`crate::app::AppContext`]. Handles validate their
//! origin before side effects and become unusable when that context is destroyed.

use crate::runtime::{self, ContextInner, ResourceKey, ResourceKind, ResourceRecord};
use std::marker::PhantomData;
use std::sync::Arc;

pub use crate::error::AppError;
pub use crate::runtime::{ContextId, LifecycleState};

/// Configuration used to create an [`App`].
#[derive(Clone, Debug, Default)]
pub struct AppConfig {
    _private: (),
}

/// Owner of an App-scoped UI runtime.
pub struct App {
    context: AppContext,
}

impl App {
    /// Creates an active context and records the current thread as its UI thread.
    pub fn new(_config: AppConfig) -> Result<Self, AppError> {
        Ok(Self {
            context: AppContext {
                inner: runtime::new_context(),
            },
        })
    }

    /// Returns the context owned by this application.
    pub fn context(&self) -> AppContext {
        self.context.clone()
    }

    /// Mounts an App-scoped declarative root in a native window and runs its UI loop.
    ///
    /// The builder is called for the initial mount and after native events so it can
    /// read current [`State`] values and produce the next [`crate::view::Node`] tree.
    pub fn run(
        self,
        window: crate::platform::Window,
        build: impl FnMut(&AppContext) -> Result<crate::view::Node, AppError> + 'static,
    ) -> Result<(), crate::platform::PlatformError> {
        crate::platform::RunLoop::new().run(self, window, build)
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.context.destroy();
    }
}

/// Cloneable reference to the App-owned runtime.
#[derive(Clone)]
pub struct AppContext {
    pub(crate) inner: Arc<ContextInner>,
}

impl AppContext {
    /// Returns this context's opaque, process-local identifier.
    pub fn id(&self) -> ContextId {
        self.inner.id
    }

    /// Reports the lifecycle state of this context.
    pub fn lifecycle(&self) -> LifecycleState {
        runtime::lifecycle(&self.inner)
    }

    /// Returns whether the caller is the UI thread recorded at [`App::new`].
    pub fn is_ui_thread(&self) -> bool {
        std::thread::current().id() == self.inner.ui_thread
    }

    /// Rejects direct UI work from a non-UI thread without mutating the context.
    pub fn check_ui_thread(&self) -> Result<(), AppError> {
        runtime::ensure_active_on_ui_thread(&self.inner)
    }

    /// Registers an App-owned state value.
    pub fn state<T>(&self, value: T) -> Result<State<T>, AppError>
    where
        T: Send + 'static,
    {
        let key = runtime::insert_resource(
            &self.inner,
            ResourceKind::State,
            ResourceRecord::State(Box::new(value)),
        )?;
        Ok(State {
            context: self.clone(),
            key,
            marker: PhantomData,
        })
    }

    /// Registers an App-owned event callback.
    pub fn action<E>(
        &self,
        callback: impl FnMut(&AppContext, E) + Send + 'static,
    ) -> Result<Action<E>, AppError>
    where
        E: Send + 'static,
    {
        let key = runtime::insert_resource(
            &self.inner,
            ResourceKind::Action,
            ResourceRecord::Action(Box::new(ActionCallback {
                callback: Some(Box::new(callback)),
            })),
        )?;
        Ok(Action {
            context: self.clone(),
            key,
            marker: PhantomData,
        })
    }

    /// Registers an App-owned data model.
    pub fn model<M>(&self, model: M) -> Result<ModelHandle<M>, AppError>
    where
        M: Model,
    {
        let key = runtime::insert_resource(
            &self.inner,
            ResourceKind::Model,
            ResourceRecord::Model(Box::new(model)),
        )?;
        Ok(ModelHandle {
            context: self.clone(),
            key,
            marker: PhantomData,
        })
    }

    /// Returns the cross-thread post scheduler for this context.
    pub fn ui_post(&self) -> crate::platform::UiPost {
        crate::platform::UiPost::new(self.clone())
    }

    /// Runs accepted posted work on the UI thread.
    pub fn run_posted(&self) -> Result<usize, AppError> {
        runtime::run_posts(&self.inner, self)
    }

    /// Starts shutdown, cancels queued UI work, and invalidates all resource handles.
    pub fn destroy(&self) {
        runtime::destroy_context(&self.inner);
    }

    pub(crate) fn same_context(&self, other: &AppContext) -> Result<(), AppError> {
        runtime::context_matches(&self.inner, &other.inner)
    }

    /// Validates that a borrowed Action is a live resource owned by this Context.
    /// The check is side-effect free and runs before an adapter publishes a route.
    pub(crate) fn validate_action<E>(&self, action: &Action<E>) -> Result<(), AppError> {
        action.context.same_context(self)?;
        runtime::validate_resource(&self.inner, action.key, ResourceKind::Action, true)
    }

    /// Releases one generated Action after strict owner, generation, slot, kind,
    /// lifecycle, and UI-thread validation. Borrowed Actions never use this path.
    pub(crate) fn release_action<E>(&self, action: &Action<E>) -> Result<(), AppError> {
        self.validate_action(action)?;
        runtime::remove_resource(&self.inner, action.key, ResourceKind::Action)?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn resource_count(&self) -> usize {
        self.inner
            .record
            .lock()
            .expect("context lock poisoned")
            .resources
            .len()
    }

    /// Returns the App-local State revision consumed by the native facade host.
    pub(crate) fn rebuild_revision(&self) -> u64 {
        runtime::rebuild_revision(&self.inner)
    }
}

/// App-owned state handle. It cannot be used after its context is destroyed.
pub struct State<T> {
    context: AppContext,
    key: ResourceKey,
    // UI-thread validation, rather than an ambient global, gates every mutation.
    marker: PhantomData<T>,
}

impl<T> Clone for State<T> {
    fn clone(&self) -> Self {
        Self {
            context: self.context.clone(),
            key: self.key,
            marker: PhantomData,
        }
    }
}

impl<T> State<T>
where
    T: Clone + Send + 'static,
{
    /// Reads the current state on its owning UI thread.
    pub fn get(&self) -> Result<T, AppError> {
        self.get_in(&self.context)
    }

    /// Reads state after verifying that `context` owns this handle.
    pub fn get_in(&self, context: &AppContext) -> Result<T, AppError> {
        self.context.same_context(context)?;
        runtime::validate_resource(&self.context.inner, self.key, ResourceKind::State, true)?;
        let record = self
            .context
            .inner
            .record
            .lock()
            .expect("context lock poisoned");
        match record.resources.get(&self.key.slot) {
            Some(ResourceRecord::State(value)) => value
                .downcast_ref::<T>()
                .cloned()
                .ok_or(AppError::StaleHandle),
            _ => Err(AppError::StaleHandle),
        }
    }

    /// Replaces state on its owning UI thread.
    pub fn set(&self, value: T) -> Result<(), AppError> {
        self.set_in(&self.context, value)
    }

    /// Replaces state after verifying that `context` owns this handle.
    pub fn set_in(&self, context: &AppContext, value: T) -> Result<(), AppError> {
        self.context.same_context(context)?;
        runtime::validate_resource(&self.context.inner, self.key, ResourceKind::State, true)?;
        let mut record = self
            .context
            .inner
            .record
            .lock()
            .expect("context lock poisoned");
        match record.resources.get_mut(&self.key.slot) {
            Some(ResourceRecord::State(stored)) => {
                let state = stored.downcast_mut::<T>().ok_or(AppError::StaleHandle)?;
                *state = value;
                record.rebuild_revision = record.rebuild_revision.wrapping_add(1);
                Ok(())
            }
            _ => Err(AppError::StaleHandle),
        }
    }
}

struct ActionCallback<E> {
    callback: Option<Box<dyn FnMut(&AppContext, E) + Send>>,
}

/// App-owned event callback handle.
pub struct Action<E> {
    context: AppContext,
    key: ResourceKey,
    marker: PhantomData<E>,
}

impl<E> std::fmt::Debug for Action<E> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Action")
            .field("context", &self.context.id())
            .finish_non_exhaustive()
    }
}

impl<E> Clone for Action<E> {
    fn clone(&self) -> Self {
        Self {
            context: self.context.clone(),
            key: self.key,
            marker: PhantomData,
        }
    }
}

impl<E> Action<E>
where
    E: Send + 'static,
{
    /// Invokes the callback only when `context` is the callback's active owner.
    pub fn invoke(&self, context: &AppContext, event: E) -> Result<(), AppError> {
        self.context.same_context(context)?;
        runtime::validate_resource(&self.context.inner, self.key, ResourceKind::Action, true)?;
        let mut callback = {
            let mut record = self
                .context
                .inner
                .record
                .lock()
                .expect("context lock poisoned");
            let Some(ResourceRecord::Action(stored)) = record.resources.get_mut(&self.key.slot)
            else {
                return Err(AppError::StaleHandle);
            };
            let action = stored
                .downcast_mut::<ActionCallback<E>>()
                .ok_or(AppError::StaleHandle)?;
            action.callback.take().ok_or(AppError::StaleHandle)?
        };
        callback(context, event);
        let mut record = self
            .context
            .inner
            .record
            .lock()
            .expect("context lock poisoned");
        if record.lifecycle == LifecycleState::Active && self.key.generation == record.generation {
            if let Some(ResourceRecord::Action(stored)) = record.resources.get_mut(&self.key.slot) {
                if let Some(action) = stored.downcast_mut::<ActionCallback<E>>() {
                    action.callback = Some(callback);
                }
            }
        }
        Ok(())
    }
}

/// Marker trait for data providers owned by an [`AppContext`].
pub trait Model: Send + 'static {}

/// Typed handle to an App-owned [`Model`].
pub struct ModelHandle<M> {
    context: AppContext,
    key: ResourceKey,
    marker: PhantomData<M>,
}

impl<M> Clone for ModelHandle<M> {
    fn clone(&self) -> Self {
        Self {
            context: self.context.clone(),
            key: self.key,
            marker: PhantomData,
        }
    }
}

impl<M> ModelHandle<M>
where
    M: Model,
{
    /// Reads the model after verifying the supplied owner context and UI thread.
    pub fn with<R>(&self, context: &AppContext, read: impl FnOnce(&M) -> R) -> Result<R, AppError> {
        self.context.same_context(context)?;
        runtime::validate_resource(&self.context.inner, self.key, ResourceKind::Model, true)?;
        let record = self
            .context
            .inner
            .record
            .lock()
            .expect("context lock poisoned");
        match record.resources.get(&self.key.slot) {
            Some(ResourceRecord::Model(model)) => model
                .downcast_ref::<M>()
                .map(read)
                .ok_or(AppError::StaleHandle),
            _ => Err(AppError::StaleHandle),
        }
    }

    /// Mutates the model after verifying the supplied owner context and UI thread.
    ///
    /// This is the only direct mutation path used by model adapters. Worker
    /// threads must submit the operation through [`AppContext::ui_post`].
    pub fn with_mut<R>(
        &self,
        context: &AppContext,
        update: impl FnOnce(&mut M) -> R,
    ) -> Result<R, AppError> {
        self.context.same_context(context)?;
        runtime::validate_resource(&self.context.inner, self.key, ResourceKind::Model, true)?;
        let mut model = {
            let mut record = self
                .context
                .inner
                .record
                .lock()
                .expect("context lock poisoned");
            let Some(stored) = record.resources.get_mut(&self.key.slot) else {
                return Err(AppError::StaleHandle);
            };
            match std::mem::replace(stored, ResourceRecord::ModelInUse) {
                ResourceRecord::Model(model) => model,
                other => {
                    *stored = other;
                    return Err(AppError::StaleHandle);
                }
            }
        };
        let typed = model.downcast_mut::<M>().ok_or(AppError::StaleHandle)?;
        let result = update(typed);
        let mut record = self
            .context
            .inner
            .record
            .lock()
            .expect("context lock poisoned");
        if record.lifecycle == LifecycleState::Active && self.key.generation == record.generation {
            if let Some(stored) = record.resources.get_mut(&self.key.slot) {
                if matches!(stored, ResourceRecord::ModelInUse) {
                    *stored = ResourceRecord::Model(model);
                }
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    struct TestModel;
    impl Model for TestModel {}

    struct DropProbe(Arc<AtomicUsize>);

    impl Drop for DropProbe {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    impl Model for DropProbe {}

    #[test]
    fn resources_are_context_scoped_and_destroyed_with_the_app() {
        let first = App::new(AppConfig::default()).unwrap();
        let second = App::new(AppConfig::default()).unwrap();
        let first_cx = first.context();
        let state = first_cx.state(1_u32).unwrap();
        let action = first_cx.action(|_, _: ()| {}).unwrap();
        let model = first_cx.model(TestModel).unwrap();

        assert_eq!(
            state.get_in(&second.context()),
            Err(AppError::ContextMismatch)
        );
        assert_eq!(
            action.invoke(&second.context(), ()),
            Err(AppError::ContextMismatch)
        );
        assert_eq!(
            model.with(&second.context(), |_| ()),
            Err(AppError::ContextMismatch)
        );

        first_cx.destroy();
        assert_eq!(state.get(), Err(AppError::ContextDestroyed));
        assert_eq!(
            action.invoke(&first_cx, ()),
            Err(AppError::ContextDestroyed)
        );
        assert_eq!(
            model.with(&first_cx, |_| ()),
            Err(AppError::ContextDestroyed)
        );
    }

    #[test]
    fn destroy_releases_state_action_and_model_arena_resources() {
        let app = App::new(AppConfig::default()).unwrap();
        let context = app.context();
        let dropped = Arc::new(AtomicUsize::new(0));

        let state_probe = DropProbe(dropped.clone());
        context.state(state_probe).unwrap();
        let action_probe = DropProbe(dropped.clone());
        context
            .action(move |_, _: ()| {
                let _ = &action_probe;
            })
            .unwrap();
        context.model(DropProbe(dropped.clone())).unwrap();

        context.destroy();
        assert_eq!(context.lifecycle(), LifecycleState::Destroyed);
        assert_eq!(dropped.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn actions_run_only_on_the_owning_ui_thread() {
        let app = App::new(AppConfig::default()).unwrap();
        let context = app.context();
        let calls = Arc::new(Mutex::new(0));
        let observed = calls.clone();
        let action = context
            .action(move |_, _: ()| *observed.lock().unwrap() += 1)
            .unwrap();
        action.invoke(&context, ()).unwrap();
        assert_eq!(*calls.lock().unwrap(), 1);
    }

    #[test]
    fn worker_thread_ui_operations_fail_without_mutating_state() {
        let app = App::new(AppConfig::default()).unwrap();
        let context = app.context();
        let state = context.state(1_u32).unwrap();
        let worker_state = state.clone();
        let worker_context = context.clone();

        let result = std::thread::spawn(move || {
            assert_eq!(worker_state.set(2), Err(AppError::ThreadAffinity));
            worker_context.state(3_u32)
        })
        .join()
        .unwrap();

        assert!(matches!(result, Err(AppError::ThreadAffinity)));
        assert_eq!(state.get(), Ok(1));
    }
}
