//! Stable platform-facing runtime services.

use crate::app::{App, AppContext, AppError};
use crate::runtime::{self, PostCallback, PostTokenInner};
use crate::view::{ElementError, ElementSpec, EngineAdapter, Mounted, Node};
use std::path::PathBuf;
use std::sync::Arc;
use synui_render::{IconId, ImageId, LottieId, RenderBackendKind};
use synui_winit::winit::event_loop::EventLoopProxy;
use synui_winit::{event_loop, AppDriver, Application, UserEvent};

/// Result state of a submitted UI post operation.
pub use crate::runtime::PostStatus as UiPostStatus;

/// Context-bound scheduler used to transfer work onto an App's UI thread.
#[derive(Clone)]
pub struct UiPost {
    context: AppContext,
}

impl UiPost {
    pub(crate) fn new(context: AppContext) -> Self {
        Self { context }
    }

    /// Submits work from any thread for execution by [`AppContext::run_posted`].
    ///
    /// A direct non-UI-thread write is rejected by the App context. A submitted
    /// operation is accepted only while the context is Active. During shutdown,
    /// queued operations are cancelled; an operation already committed to the UI
    /// thread completes before the context releases its resources.
    pub fn submit(
        &self,
        work: impl FnOnce(AppContext) + Send + 'static,
    ) -> Result<UiPostToken, AppError> {
        let inner = runtime::submit_post(&self.context.inner, Box::new(work) as PostCallback)?;
        Ok(UiPostToken { inner })
    }
}

/// Cancellation and terminal-status handle for a posted UI operation.
#[derive(Clone)]
pub struct UiPostToken {
    inner: Arc<PostTokenInner>,
}

impl UiPostToken {
    /// Returns the current status without blocking.
    pub fn status(&self) -> UiPostStatus {
        self.inner.status()
    }

    /// Cancels work that has not begun executing.
    pub fn cancel(&self) -> bool {
        self.inner.cancel()
    }
}

/// Native window configuration for the standard desktop host.
#[derive(Clone, Debug)]
pub struct Window {
    title: String,
    /// Optional second native window opened at resume (same App-owned root builder).
    second_window_title: Option<String>,
    /// Image cache preloads: `(id, path)` loaded before the event loop starts.
    images: Vec<(String, PathBuf)>,
    /// Icon cache preloads: `(id, path)` SVG files loaded before the event loop.
    icons: Vec<(String, PathBuf)>,
    /// Lottie JSON preloads: `(id, path)` loaded before the event loop starts.
    lotties: Vec<(String, PathBuf)>,
}

impl Window {
    /// Creates a native window with the supplied title.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            second_window_title: None,
            images: Vec::new(),
            icons: Vec::new(),
            lotties: Vec::new(),
        }
    }

    /// Returns the configured title without exposing a native window handle.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Replaces the title in an application-level window configuration.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Opens a second native window at resume that shares the same App-owned root builder
    /// and therefore observes the same `State` / `Action` graph.
    ///
    /// Tray / DesktopMenu remain host boundaries and are not configured here.
    pub fn with_second_window(mut self, title: impl Into<String>) -> Self {
        self.second_window_title = Some(title.into());
        self
    }

    /// Preloads a raster (PNG/JPEG/GIF/WebP) or SVG image into the host
    /// `ImageCache` before the first frame.
    ///
    /// SVG files keep vector data and re-render into `Image` bounds (sharp at
    /// any size); `currentColor` follows theme `on_surface`.
    pub fn with_image(mut self, id: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        self.images.push((id.into(), path.into()));
        self
    }

    /// Preloads a custom SVG into the host `IconCache` under `id`.
    ///
    /// Prefer curated names via [`crate::widgets::Icon::new`] when possible.
    /// Use this for app-specific glyphs; SVG should use `fill="currentColor"`
    /// so cascade / theme tint applies. Pair with `Icon::new(id)`.
    pub fn with_icon(mut self, id: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        self.icons.push((id.into(), path.into()));
        self
    }

    /// Preloads Lottie JSON into the host `LottieCache` before the first frame.
    ///
    /// Pair with [`crate::widgets::Lottie::from_asset`] using the same `id`.
    pub fn with_lottie(mut self, id: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        self.lotties.push((id.into(), path.into()));
        self
    }
}

impl Default for Window {
    fn default() -> Self {
        Self::new("SynUI")
    }
}

/// Failures while mounting or running a native SynUI application.
#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    /// App-context resource creation or callback execution failed.
    #[error(transparent)]
    App(#[from] AppError),
    /// The declarative root could not be mounted in the retained widget tree.
    #[error(transparent)]
    Element(#[from] ElementError),
    /// The native event loop could not start or run.
    #[error(transparent)]
    EventLoop(#[from] winit::error::EventLoopError),
    /// A configured window image preload failed.
    #[error("failed to load image {id} from {path}: {source}")]
    Image {
        id: String,
        path: String,
        #[source]
        source: synui_render::ImageError,
    },
    /// A configured window icon preload failed.
    #[error("failed to load icon {id} from {path}: {source}")]
    Icon {
        id: String,
        path: String,
        #[source]
        source: synui_render::IconError,
    },
    /// A configured window Lottie preload failed.
    #[error("failed to load lottie {id} from {path}: {source}")]
    Lottie {
        id: String,
        path: String,
        #[source]
        source: synui_render::LottieError,
    },
}

/// Native event-loop entry point for a mounted App-scoped declarative root.
#[derive(Debug, Default)]
pub struct RunLoop {
    _private: (),
}

impl RunLoop {
    /// Creates the standard native run loop.
    pub fn new() -> Self {
        Self::default()
    }

    /// Mounts `build` into `window` and blocks until the native window closes.
    ///
    /// The root builder receives the owning [`AppContext`] for every render pass.
    /// Native button presses invoke their typed App-owned [`crate::app::Action`]
    /// callbacks directly; no global action registry is involved.
    pub fn run(
        self,
        app: App,
        window: Window,
        mut build: impl FnMut(&AppContext) -> Result<Node, AppError> + 'static,
    ) -> Result<(), PlatformError> {
        let context = app.context();
        context.check_ui_thread()?;
        let mut root = build(&context)?;
        let rendered_revision = context.rebuild_revision();
        let adapter = EngineAdapter::with_context(app.context());
        adapter.validate_root(&mut root)?;
        let driver = FacadeDriver {
            context,
            build: Box::new(build),
            adapter,
            root: Some(root),
            mounted: None,
            previous: None,
            rendered_revision,
        };

        // `app` remains alive until `Application::run` returns, so the context
        // and every State/Action captured by the root builder remain App-owned.
        let mut application = Application::new(driver)
            .with_title(window.title)
            .with_render_backend(RenderBackendKind::Gpu);
        if let Some(title) = window.second_window_title {
            application = application.with_second_window(title);
        }
        for (id, path) in window.images {
            application
                .images_mut()
                .load_path(ImageId(id.clone()), &path)
                .map_err(|source| PlatformError::Image {
                    id,
                    path: path.display().to_string(),
                    source,
                })?;
        }
        for (id, path) in window.icons {
            application
                .icons_mut()
                .load_path(IconId(id.clone()), &path)
                .map_err(|source| PlatformError::Icon {
                    id,
                    path: path.display().to_string(),
                    source,
                })?;
        }
        for (id, path) in window.lotties {
            application
                .lotties_mut()
                .load_path(LottieId(id.clone()), &path)
                .map_err(|source| PlatformError::Lottie {
                    id,
                    path: path.display().to_string(),
                    source,
                })?;
        }
        application.run(event_loop())?;
        Ok(())
    }
}

type RootBuilder = Box<dyn FnMut(&AppContext) -> Result<Node, AppError>>;

/// Private bridge that keeps the v0.2 Node tree and App resources scoped to one
/// native host. The public entry point is [`RunLoop::run`].
struct FacadeDriver {
    context: AppContext,
    build: RootBuilder,
    adapter: EngineAdapter,
    root: Option<Node>,
    mounted: Option<Mounted>,
    previous: Option<ElementSpec>,
    // The mounted tree is replaced only after App-owned State changes. In
    // particular, a PointerDown repaint must preserve a retained button's
    // pressed state until its matching PointerUp is dispatched.
    rendered_revision: u64,
}

impl AppDriver for FacadeDriver {
    fn on_event_loop_ready(&mut self, proxy: EventLoopProxy<UserEvent>) {
        let waker = Arc::new(move || {
            let _ = proxy.send_event(UserEvent::UiPost);
        });
        runtime::set_post_waker(&self.context.inner, waker);
    }

    fn build_ui(&mut self, store: &mut synui_core::WidgetStore) -> synui_core::WidgetId {
        let root = self
            .root
            .as_mut()
            .expect("facade root is initialized before the run loop");
        let (spec, mounted) = self.adapter.with_host_store(store, |adapter| {
            adapter
                .mount_root(root)
                .expect("initial facade root was validated before mounting")
        });
        let widget = store
            .root()
            .expect("mounted facade root has a retained widget");
        self.previous = Some(spec);
        self.mounted = Some(mounted);
        widget
    }

    fn on_frame(&mut self, store: &mut synui_core::WidgetStore) {
        if let Err(error) = self.context.run_posted() {
            tracing::error!("synui: failed to run posted UI work: {error}");
        }
        if let Some(mounted) = self.mounted.as_mut() {
            let _ = self
                .adapter
                .with_host_store(store, |adapter| adapter.poll_input_changes(mounted));
        }
        let revision = self.context.rebuild_revision();
        if revision == self.rendered_revision {
            return;
        }
        let mut next = match (self.build)(&self.context) {
            Ok(next) => next,
            Err(error) => {
                tracing::error!("synui: failed to build updated root: {error}");
                return;
            }
        };
        let (Some(previous), Some(mounted)) = (self.previous.as_ref(), self.mounted.as_mut())
        else {
            return;
        };
        let spec = match self.adapter.with_host_store(store, |adapter| {
            adapter.update_root(&mut next, mounted, previous)
        }) {
            Ok(spec) => spec,
            Err(error) => {
                tracing::error!("synui: failed to update retained root: {error}");
                return;
            }
        };
        self.root = Some(next);
        self.previous = Some(spec);
        self.rendered_revision = revision;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, AppConfig};
    use crate::view::Element;
    use crate::widgets::{Button, TextInput};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use synui_core::{Event, Point, PointerButton, Rect};

    #[test]
    fn retained_button_press_invokes_app_action_and_rebuilds_from_updated_state() {
        let app = App::new(AppConfig::default()).unwrap();
        let context = app.context();
        let count = context.state(0_i32).unwrap();
        let increment = context
            .action({
                let count = count.clone();
                move |_, (): ()| count.set(count.get().unwrap() + 1).unwrap()
            })
            .unwrap();
        let button = Button::new("+1").on_press(increment);
        let initial_root = Node::container([
            Node::text(format!("Count: {}", count.get().unwrap())),
            button.node(),
        ]);
        let count_for_rebuild = count.clone();
        let rendered_revision = context.rebuild_revision();
        let mut driver = FacadeDriver {
            context: context.clone(),
            build: Box::new(move |_| {
                Ok(Node::container([
                    Node::text(format!("Count: {}", count_for_rebuild.get()?)),
                    button.node(),
                ]))
            }),
            adapter: EngineAdapter::with_context(context.clone()),
            root: Some(initial_root),
            mounted: None,
            previous: None,
            rendered_revision,
        };
        let mut store = synui_core::WidgetStore::new();
        let root_id = driver.build_ui(&mut store);
        let button_id = store.find_by_label("+1").unwrap();
        let bounds = Rect::new(0.0, 0.0, 100.0, 100.0);
        store.get_mut(root_id).unwrap().set_bounds(bounds);
        store.get_mut(button_id).unwrap().set_bounds(bounds);

        let position = Point::new(50.0, 20.0);
        assert!(
            store
                .dispatch_event(&Event::PointerDown {
                    id: 0,
                    position,
                    button: PointerButton::Primary,
                    shift: false,
                    ctrl: false,
                })
                .handled
        );
        driver.on_frame(&mut store);
        assert_eq!(store.find_by_label("+1"), Some(button_id));

        assert!(
            store
                .dispatch_event(&Event::PointerUp {
                    id: 0,
                    position,
                    button: PointerButton::Primary,
                })
                .handled
        );
        assert_eq!(count.get(), Ok(1));

        driver.on_frame(&mut store);
        let rebuilt = driver.previous.as_ref().unwrap();
        let Node::Builtin(count_label) = &rebuilt.children()[0] else {
            panic!("counter root should retain its text label");
        };
        assert_eq!(count_label.text_content(), Some("Count: 1"));
    }

    #[test]
    fn native_input_change_dispatches_action_before_the_same_frame_render() {
        let app = App::new(AppConfig::default()).unwrap();
        let context = app.context();
        let state = context.state("before".to_owned()).unwrap();
        let trace = Arc::new(Mutex::new(Vec::new()));
        let action = context
            .action({
                let state = state.clone();
                let trace = Arc::clone(&trace);
                move |_, value: String| {
                    trace.lock().unwrap().push("action");
                    state.set(value).unwrap();
                }
            })
            .unwrap();
        let initial_root = TextInput::new("value")
            .controlled(state.clone(), action.clone())
            .node();
        let rendered_revision = context.rebuild_revision();
        let mut driver = FacadeDriver {
            context: context.clone(),
            build: Box::new({
                let state = state.clone();
                let action = action.clone();
                let trace = Arc::clone(&trace);
                move |_| {
                    trace.lock().unwrap().push("render");
                    Ok(TextInput::new("value")
                        .controlled(state.clone(), action.clone())
                        .node())
                }
            }),
            adapter: EngineAdapter::with_context(context),
            root: Some(initial_root),
            mounted: None,
            previous: None,
            rendered_revision,
        };
        let mut store = synui_core::WidgetStore::new();
        let field_id = driver.build_ui(&mut store);
        store
            .get_mut(field_id)
            .unwrap()
            .as_any_mut()
            .downcast_mut::<synui_widgets::TextField>()
            .unwrap()
            .edit
            .text = "edited".to_owned();

        driver.on_frame(&mut store);

        assert_eq!(state.get().unwrap(), "edited");
        assert_eq!(&*trace.lock().unwrap(), &["action", "render"]);
        assert_eq!(
            store
                .get(field_id)
                .unwrap()
                .as_any()
                .downcast_ref::<synui_widgets::TextField>()
                .unwrap()
                .edit
                .text,
            "edited"
        );
    }

    #[test]
    fn worker_post_runs_only_when_drained_on_ui_thread() {
        let app = App::new(AppConfig::default()).unwrap();
        let context = app.context();
        let state = context.state(1_u32).unwrap();
        let posted_state = state.clone();
        let post = context.ui_post();
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let token = std::thread::spawn(move || {
            post.submit(move |cx| {
                assert!(cx.is_ui_thread());
                posted_state.set(2).unwrap();
                observed.fetch_add(1, Ordering::SeqCst);
            })
            .unwrap()
        })
        .join()
        .unwrap();

        assert_eq!(token.status(), UiPostStatus::Accepted);
        assert_eq!(context.run_posted().unwrap(), 1);
        assert_eq!(token.status(), UiPostStatus::Executed);
        assert_eq!(state.get(), Ok(2));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn cancellation_and_destroy_prevent_callback_execution() {
        let app = App::new(AppConfig::default()).unwrap();
        let context = app.context();
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let token = context
            .ui_post()
            .submit(move |_| {
                observed.fetch_add(1, Ordering::SeqCst);
            })
            .unwrap();
        assert!(token.cancel());
        assert_eq!(context.run_posted().unwrap(), 0);
        assert_eq!(token.status(), UiPostStatus::Cancelled);
        assert_eq!(calls.load(Ordering::SeqCst), 0);

        let token = context.ui_post().submit(|_| {}).unwrap();
        context.destroy();
        assert_eq!(token.status(), UiPostStatus::Cancelled);
        assert_eq!(context.run_posted(), Err(AppError::ContextDestroyed));
    }

    #[test]
    fn shutdown_cancels_queued_posts_and_defers_destruction_until_running_post_returns() {
        let app = App::new(AppConfig::default()).unwrap();
        let context = app.context();
        let state = context.state(1_u32).unwrap();
        let posted_state = state.clone();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (finish_tx, finish_rx) = std::sync::mpsc::channel();
        let running = context
            .ui_post()
            .submit(move |_| {
                started_tx.send(()).unwrap();
                finish_rx.recv().unwrap();
                assert_eq!(posted_state.set(2), Err(AppError::ContextDestroyed));
            })
            .unwrap();
        let queued = context
            .ui_post()
            .submit(|_| panic!("shutdown must cancel queued posts"))
            .unwrap();

        let shutting_down = context.clone();
        let shutdown = std::thread::spawn(move || {
            started_rx.recv().unwrap();
            shutting_down.destroy();
            assert_eq!(
                shutting_down.lifecycle(),
                crate::app::LifecycleState::Draining
            );
            assert!(matches!(
                shutting_down.ui_post().submit(|_| {}),
                Err(AppError::ContextDestroyed)
            ));
            finish_tx.send(()).unwrap();
        });

        assert_eq!(context.run_posted(), Ok(1));
        shutdown.join().unwrap();
        assert_eq!(running.status(), UiPostStatus::Executed);
        assert_eq!(queued.status(), UiPostStatus::Cancelled);
        assert_eq!(context.lifecycle(), crate::app::LifecycleState::Destroyed);
        assert_eq!(state.get(), Err(AppError::ContextDestroyed));
    }
}
