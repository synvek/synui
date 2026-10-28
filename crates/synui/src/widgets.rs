//! Stable built-in widget descriptors for the curated facade.

use crate::app::{Action, AppContext, AppError, State};
use crate::view::{Element, EventContext};
use std::sync::{Arc, Mutex};

/// Stable identifier for one choice in a [`Select`].
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ChoiceId(usize);

impl ChoiceId {
    /// Creates an identifier from its zero-based choice position.
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    /// Returns the zero-based choice position.
    pub const fn index(self) -> usize {
        self.0
    }
}

impl From<usize> for ChoiceId {
    fn from(value: usize) -> Self {
        Self::new(value)
    }
}

/// Typed value carried by the unified input runtime.
#[derive(Clone, Debug, PartialEq)]
pub enum InputValue {
    Text(String),
    Bool(bool),
    Number(f64),
    Choice(ChoiceId),
}

/// The four input families supported by the stable facade.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum InputKind {
    Text,
    Bool,
    Number,
    Choice,
}

/// Homogeneous controlled/uncontrolled configuration shared by every input.
#[derive(Clone)]
pub enum InputConfig<T> {
    /// The displayed value is read from App-owned state on every render.
    Controlled {
        value: State<T>,
        on_change: Action<T>,
    },
    /// The native widget owns its value after the initial mount.
    Uncontrolled {
        initial: T,
        on_change: Option<Action<T>>,
    },
}

impl<T> InputConfig<T> {
    /// Creates a controlled configuration.
    pub fn controlled(value: State<T>, on_change: Action<T>) -> Self {
        Self::Controlled { value, on_change }
    }

    /// Creates an uncontrolled configuration.
    pub fn uncontrolled(initial: T, on_change: Option<Action<T>>) -> Self {
        Self::Uncontrolled { initial, on_change }
    }
}

/// Defined failures produced while validating a typed input before store access.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum InputConfigError {
    #[error("{kind:?} input does not support value {value:?}")]
    UnsupportedValue { kind: InputKind, value: InputValue },
    #[error("choice {choice:?} is not present in the Select choices")]
    InvalidChoice { choice: ChoiceId },
    #[error("numeric value {value} is outside the inclusive range {min}..={max}")]
    OutOfRange { value: f64, min: f64, max: f64 },
    #[error(transparent)]
    App(#[from] AppError),
}

/// Converts a closure accepting either [`EventContext`] or [`AppContext`] into
/// the default Button click path.
///
/// This trait is implemented by the framework for ordinary Rust closures and is
/// not intended to be implemented directly.
#[doc(hidden)]
pub trait ButtonClickHandler<Context>: Send + 'static {
    #[doc(hidden)]
    fn call(&mut self, context: EventContext);
}

impl<F> ButtonClickHandler<EventContext> for F
where
    F: FnMut(EventContext) + Send + 'static,
{
    fn call(&mut self, context: EventContext) {
        self(context);
    }
}

impl<F> ButtonClickHandler<AppContext> for F
where
    F: FnMut(AppContext) + Send + 'static,
{
    fn call(&mut self, context: EventContext) {
        self(context.app().clone());
    }
}

type ButtonCallbackFn = Box<dyn FnMut(EventContext) + Send + 'static>;

/// Cloneable, facade-private ownership for a non-Clone Rust closure.
#[derive(Clone)]
pub(crate) struct ButtonCallback {
    callback: Arc<Mutex<Option<ButtonCallbackFn>>>,
}

impl std::fmt::Debug for ButtonCallback {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ButtonCallback(..)")
    }
}

impl ButtonCallback {
    fn new(callback: impl FnMut(EventContext) + Send + 'static) -> Self {
        Self {
            callback: Arc::new(Mutex::new(Some(Box::new(callback)))),
        }
    }

    pub(crate) fn invoke(&self, context: EventContext) {
        let mut callback = self
            .callback
            .lock()
            .expect("button callback lock poisoned")
            .take()
            .expect("button callback cannot be invoked recursively");
        callback(context);
        let mut stored = self.callback.lock().expect("button callback lock poisoned");
        debug_assert!(stored.is_none());
        *stored = Some(callback);
    }
}

#[derive(Clone, Debug)]
enum ButtonHandler {
    Click(ButtonCallback),
    Press(Action<()>),
}

/// A button whose handler is owned by its App Context.
pub struct Button {
    label: String,
    handler: Option<ButtonHandler>,
    tooltip: Option<String>,
}
impl Button {
    /// Creates a labelled button without a handler.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            handler: None,
            tooltip: None,
        }
    }
    /// Attaches an ordinary Rust closure as the default click handler.
    ///
    /// The closure may accept either [`EventContext`] or [`AppContext`] by value.
    /// It is registered as an Action owned by the mounted Button's App Context.
    pub fn on_click<Handler, Context>(mut self, mut handler: Handler) -> Self
    where
        Handler: ButtonClickHandler<Context>,
    {
        self.handler = Some(ButtonHandler::Click(ButtonCallback::new(move |context| {
            handler.call(context);
        })));
        self
    }
    /// Attaches an existing App-owned press Action. The Button borrows this
    /// Action and never releases it during update or unmount.
    pub fn on_press(mut self, handler: Action<()>) -> Self {
        self.handler = Some(ButtonHandler::Press(handler));
        self
    }
    /// Invokes the configured handler directly through the supplied context.
    pub fn press(&self, context: &AppContext) -> Result<(), AppError> {
        match &self.handler {
            Some(ButtonHandler::Click(callback)) => {
                callback.invoke(EventContext::new(context.clone()));
                Ok(())
            }
            Some(ButtonHandler::Press(handler)) => handler.invoke(context, ()),
            None => Ok(()),
        }
    }
    /// Returns the visible label.
    pub fn label(&self) -> &str {
        &self.label
    }
    /// Hover tip text (short delay; non-interactive overlay).
    pub fn tooltip(mut self, text: impl Into<String>) -> Self {
        self.tooltip = Some(text.into());
        self
    }
}
impl Element for Button {
    fn describe(&self, _cx: &mut crate::view::BuildCx) -> crate::view::ElementSpec {
        let mut accessibility =
            crate::view::Accessibility::new().role(crate::view::AccessibilityRole::Button);
        if self.handler.is_some() {
            accessibility = accessibility.action(crate::view::AccessibilityAction::Click);
        }
        let mut spec = crate::view::ElementSpec::custom("button", [])
            .property("label", self.label.clone())
            .accessibility(accessibility)
            .on(crate::view::Event::Press, 0);
        if let Some(tip) = &self.tooltip {
            spec = spec.property("tooltip", tip.clone());
        }
        match &self.handler {
            Some(ButtonHandler::Click(callback)) => spec.on_click(callback.clone()),
            Some(ButtonHandler::Press(handler)) => spec.on_press(handler.clone()),
            None => spec,
        }
    }
}

/// Text input using the unified controlled/uncontrolled data flow.
#[derive(Clone)]
pub struct TextInput {
    placeholder: String,
    password: bool,
    config: InputConfig<String>,
}
impl TextInput {
    /// Creates an uncontrolled empty text field, preserving the existing constructor contract.
    pub fn new(placeholder: impl Into<String>) -> Self {
        Self {
            placeholder: placeholder.into(),
            password: false,
            config: InputConfig::uncontrolled(String::new(), None),
        }
    }
    /// Masks glyphs (native `TextField::password`); value remains the real string.
    pub fn password(mut self, password: bool) -> Self {
        self.password = password;
        self
    }
    /// Uses a controlled State/Action pair.
    pub fn controlled(mut self, value: State<String>, on_change: Action<String>) -> Self {
        self.config = InputConfig::controlled(value, on_change);
        self
    }
    /// Uses a native-owned initial value and optional typed change Action.
    pub fn uncontrolled(
        mut self,
        initial: impl Into<String>,
        on_change: Option<Action<String>>,
    ) -> Self {
        self.config = InputConfig::uncontrolled(initial.into(), on_change);
        self
    }
    /// Replaces the complete homogeneous input configuration.
    pub fn with_config(mut self, config: InputConfig<String>) -> Self {
        self.config = config;
        self
    }
}
impl Element for TextInput {
    fn describe(&self, _cx: &mut crate::view::BuildCx) -> crate::view::ElementSpec {
        let accessibility = crate::view::Accessibility::new()
            .role(crate::view::AccessibilityRole::TextInput)
            .action(crate::view::AccessibilityAction::Focus)
            .action(crate::view::AccessibilityAction::SetValue);
        let mut spec = crate::view::ElementSpec::custom("text-input", [])
            .accessibility(accessibility)
            .on(crate::view::Event::Change, 0)
            .with_text_input(self.placeholder.clone(), &self.config);
        if self.password {
            spec = spec.property("password", "true");
        }
        spec
    }
}

/// Multiline text area using the unified controlled/uncontrolled data flow.
///
/// Native [`synui_widgets::TextArea`] has no placeholder glyph; the `label`
/// passed to [`TextArea::new`] is accessibility name only.
#[derive(Clone)]
pub struct TextArea {
    label: String,
    config: InputConfig<String>,
    background: Option<String>,
    border: bool,
}
impl TextArea {
    /// Creates an uncontrolled empty multiline field; `label` is a11y name only.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            config: InputConfig::uncontrolled(String::new(), None),
            background: None,
            border: true,
        }
    }
    /// Uses a controlled State/Action pair.
    pub fn controlled(mut self, value: State<String>, on_change: Action<String>) -> Self {
        self.config = InputConfig::controlled(value, on_change);
        self
    }
    /// Uses a native-owned initial value and optional typed change Action.
    pub fn uncontrolled(
        mut self,
        initial: impl Into<String>,
        on_change: Option<Action<String>>,
    ) -> Self {
        self.config = InputConfig::uncontrolled(initial.into(), on_change);
        self
    }
    /// Replaces the complete homogeneous input configuration.
    pub fn with_config(mut self, config: InputConfig<String>) -> Self {
        self.config = config;
        self
    }
    /// Field fill as `#RRGGBB` / `#RRGGBBAA`, or `transparent` to skip the fill.
    pub fn background(mut self, color: impl Into<String>) -> Self {
        self.background = Some(color.into());
        self
    }
    /// When false, no field outline is drawn.
    pub fn border(mut self, show: bool) -> Self {
        self.border = show;
        self
    }
}
impl Element for TextArea {
    fn describe(&self, _cx: &mut crate::view::BuildCx) -> crate::view::ElementSpec {
        let accessibility = crate::view::Accessibility::new()
            .role(crate::view::AccessibilityRole::TextInput)
            .name(self.label.clone())
            .action(crate::view::AccessibilityAction::Focus)
            .action(crate::view::AccessibilityAction::SetValue);
        let mut spec = crate::view::ElementSpec::custom("text-area", [])
            .accessibility(accessibility)
            .on(crate::view::Event::Change, 0)
            .with_text_input(String::new(), &self.config)
            .property("border", if self.border { "true" } else { "false" });
        if let Some(color) = &self.background {
            spec = spec.property("background", color.clone());
        }
        spec
    }
}

/// Boolean input using the unified controlled/uncontrolled data flow.
#[derive(Clone)]
pub struct Checkbox {
    label: String,
    config: InputConfig<bool>,
}
impl Checkbox {
    /// Creates an uncontrolled checkbox with `checked` as its mount-only initial value.
    pub fn new(checked: bool) -> Self {
        Self {
            label: String::new(),
            config: InputConfig::uncontrolled(checked, None),
        }
    }
    /// Optional label painted beside the box (left-aligned with the control).
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }
    pub fn controlled(mut self, value: State<bool>, on_change: Action<bool>) -> Self {
        self.config = InputConfig::controlled(value, on_change);
        self
    }
    pub fn uncontrolled(mut self, initial: bool, on_change: Option<Action<bool>>) -> Self {
        self.config = InputConfig::uncontrolled(initial, on_change);
        self
    }
    pub fn with_config(mut self, config: InputConfig<bool>) -> Self {
        self.config = config;
        self
    }
}
impl Element for Checkbox {
    fn describe(&self, _cx: &mut crate::view::BuildCx) -> crate::view::ElementSpec {
        let checked = match &self.config {
            InputConfig::Controlled { value, .. } => value.get().ok(),
            InputConfig::Uncontrolled { initial, .. } => Some(*initial),
        };
        let mut accessibility = crate::view::Accessibility::new()
            .role(crate::view::AccessibilityRole::Checkbox)
            .action(crate::view::AccessibilityAction::Click)
            .action(crate::view::AccessibilityAction::Focus);
        if !self.label.is_empty() {
            accessibility = accessibility.name(self.label.clone());
        }
        if let Some(checked) = checked {
            accessibility = accessibility.state(crate::view::AccessibilityState::Checked(checked));
        }
        let mut spec = crate::view::ElementSpec::custom("checkbox", [])
            .accessibility(accessibility)
            .on(crate::view::Event::Change, 0)
            .with_bool_input(&self.config);
        if !self.label.is_empty() {
            spec = spec.property("label", self.label.clone());
        }
        spec
    }
}

/// Toggle switch using the unified controlled/uncontrolled data flow.
///
/// Distinct from [`Checkbox`]: mounts native `Switch` (`kind = "switch"`), not a checkbox host.
#[derive(Clone)]
pub struct Switch {
    config: InputConfig<bool>,
}
impl Switch {
    /// Creates an uncontrolled switch with `on` as its mount-only initial value.
    pub fn new(on: bool) -> Self {
        Self {
            config: InputConfig::uncontrolled(on, None),
        }
    }
    pub fn controlled(mut self, value: State<bool>, on_change: Action<bool>) -> Self {
        self.config = InputConfig::controlled(value, on_change);
        self
    }
    pub fn uncontrolled(mut self, initial: bool, on_change: Option<Action<bool>>) -> Self {
        self.config = InputConfig::uncontrolled(initial, on_change);
        self
    }
    pub fn with_config(mut self, config: InputConfig<bool>) -> Self {
        self.config = config;
        self
    }
}
impl Element for Switch {
    fn describe(&self, _cx: &mut crate::view::BuildCx) -> crate::view::ElementSpec {
        let on = match &self.config {
            InputConfig::Controlled { value, .. } => value.get().ok(),
            InputConfig::Uncontrolled { initial, .. } => Some(*initial),
        };
        let mut accessibility = crate::view::Accessibility::new()
            .role(crate::view::AccessibilityRole::Switch)
            .action(crate::view::AccessibilityAction::Click)
            .action(crate::view::AccessibilityAction::Focus);
        if let Some(on) = on {
            accessibility = accessibility.state(crate::view::AccessibilityState::Checked(on));
        }
        crate::view::ElementSpec::custom("switch", [])
            .accessibility(accessibility)
            .on(crate::view::Event::Change, 0)
            .with_bool_input(&self.config)
    }
}

/// Radio selection using the unified controlled/uncontrolled data flow.
///
/// Facade a11y uses Checkbox role until a dedicated Radio role exists;
/// native host exposes checkbox-equivalent checked metadata (not a Switch).
#[derive(Clone)]
pub struct Radio {
    label: String,
    config: InputConfig<bool>,
}
impl Radio {
    /// Creates an uncontrolled radio with `selected` as its mount-only initial value.
    pub fn new(label: impl Into<String>, selected: bool) -> Self {
        Self {
            label: label.into(),
            config: InputConfig::uncontrolled(selected, None),
        }
    }
    pub fn controlled(mut self, value: State<bool>, on_change: Action<bool>) -> Self {
        self.config = InputConfig::controlled(value, on_change);
        self
    }
    pub fn uncontrolled(mut self, initial: bool, on_change: Option<Action<bool>>) -> Self {
        self.config = InputConfig::uncontrolled(initial, on_change);
        self
    }
    pub fn with_config(mut self, config: InputConfig<bool>) -> Self {
        self.config = config;
        self
    }
}
impl Element for Radio {
    fn describe(&self, _cx: &mut crate::view::BuildCx) -> crate::view::ElementSpec {
        let selected = match &self.config {
            InputConfig::Controlled { value, .. } => value.get().ok(),
            InputConfig::Uncontrolled { initial, .. } => Some(*initial),
        };
        let mut accessibility = crate::view::Accessibility::new()
            .role(crate::view::AccessibilityRole::Checkbox)
            .name(self.label.clone())
            .action(crate::view::AccessibilityAction::Click)
            .action(crate::view::AccessibilityAction::Focus);
        if let Some(selected) = selected {
            accessibility = accessibility.state(crate::view::AccessibilityState::Checked(selected));
        }
        crate::view::ElementSpec::custom("radio", [])
            .property("label", self.label.clone())
            .accessibility(accessibility)
            .on(crate::view::Event::Change, 0)
            .with_bool_input(&self.config)
    }
}

/// Linear progress indicator (display-only; no InputConfig).
#[derive(Clone)]
pub struct LinearProgress {
    value: f32,
    indeterminate: bool,
}
impl LinearProgress {
    /// Creates a determinate progress bar; `value` is clamped to `0.0..=1.0` at mount.
    pub fn new(value: f32) -> Self {
        Self {
            value,
            indeterminate: false,
        }
    }
    pub fn indeterminate(mut self, indeterminate: bool) -> Self {
        self.indeterminate = indeterminate;
        self
    }
    pub fn value(mut self, value: f32) -> Self {
        self.value = value;
        self
    }
}
impl Element for LinearProgress {
    fn describe(&self, _cx: &mut crate::view::BuildCx) -> crate::view::ElementSpec {
        let pct = (self.value.clamp(0.0, 1.0) * 100.0).round();
        crate::view::ElementSpec::custom("linear-progress", [])
            .property("value", self.value.to_string())
            .property(
                "indeterminate",
                if self.indeterminate { "true" } else { "false" },
            )
            .accessibility(
                crate::view::Accessibility::new()
                    .role(crate::view::AccessibilityRole::ProgressBar)
                    .value(format!("{pct}%")),
            )
    }
}

/// Circular progress indicator (display-only; no InputConfig).
///
/// Native paint draws a track plus a determinate arc from `value`, or a short
/// rotating arc when `indeterminate` is true.
#[derive(Clone)]
pub struct CircularProgress {
    value: f32,
    indeterminate: bool,
}
impl CircularProgress {
    /// Creates a progress indicator; defaults to indeterminate visual host behavior.
    pub fn new(value: f32) -> Self {
        Self {
            value,
            indeterminate: true,
        }
    }
    pub fn indeterminate(mut self, indeterminate: bool) -> Self {
        self.indeterminate = indeterminate;
        self
    }
    pub fn value(mut self, value: f32) -> Self {
        self.value = value;
        self
    }
}
impl Element for CircularProgress {
    fn describe(&self, _cx: &mut crate::view::BuildCx) -> crate::view::ElementSpec {
        let pct = (self.value.clamp(0.0, 1.0) * 100.0).round();
        crate::view::ElementSpec::custom("circular-progress", [])
            .property("value", self.value.to_string())
            .property(
                "indeterminate",
                if self.indeterminate { "true" } else { "false" },
            )
            .accessibility(
                crate::view::Accessibility::new()
                    .role(crate::view::AccessibilityRole::ProgressBar)
                    .value(format!("{pct}%")),
            )
    }
}

/// Elevated surface container for composed children.
pub struct Card {
    elevation: u8,
    children: Vec<crate::view::Node>,
}
impl Card {
    pub fn new(children: impl IntoIterator<Item = crate::view::Node>) -> Self {
        Self {
            elevation: 1,
            children: children.into_iter().collect(),
        }
    }
    pub fn elevation(mut self, elevation: u8) -> Self {
        self.elevation = elevation;
        self
    }
}
impl Element for Card {
    fn describe(&self, _cx: &mut crate::view::BuildCx) -> crate::view::ElementSpec {
        crate::view::ElementSpec::custom("card", self.children.clone())
            .property("elevation", self.elevation.to_string())
            .accessibility(
                crate::view::Accessibility::new()
                    .role(crate::view::AccessibilityRole::Generic)
                    .name("Card"),
            )
    }
}

/// Single-line / two-line list row with optional trailing children and press Action.
pub struct ListItem {
    title: String,
    subtitle: Option<String>,
    trailing: Vec<crate::view::Node>,
    on_press: Option<Action<()>>,
    background: Option<String>,
}
impl ListItem {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            subtitle: None,
            trailing: Vec::new(),
            on_press: None,
            background: None,
        }
    }
    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }
    pub fn trailing(mut self, children: impl IntoIterator<Item = crate::view::Node>) -> Self {
        self.trailing = children.into_iter().collect();
        self
    }
    pub fn on_press(mut self, handler: Action<()>) -> Self {
        self.on_press = Some(handler);
        self
    }
    /// Row fill as `#RRGGBB` or `#RRGGBBAA`.
    pub fn background(mut self, color: impl Into<String>) -> Self {
        self.background = Some(color.into());
        self
    }
}
impl Element for ListItem {
    fn describe(&self, _cx: &mut crate::view::BuildCx) -> crate::view::ElementSpec {
        let mut accessibility = crate::view::Accessibility::new()
            .role(crate::view::AccessibilityRole::ListItem)
            .name(self.title.clone());
        if self.on_press.is_some() {
            accessibility = accessibility.action(crate::view::AccessibilityAction::Click);
        }
        let mut spec = crate::view::ElementSpec::custom("list-item", self.trailing.clone())
            .property("title", self.title.clone())
            .accessibility(accessibility)
            .on(crate::view::Event::Press, 0);
        if let Some(subtitle) = &self.subtitle {
            spec = spec.property("subtitle", subtitle.clone());
        }
        if let Some(color) = &self.background {
            spec = spec.property("background", color.clone());
        }
        if let Some(handler) = self.on_press.clone() {
            spec = spec.on_press(handler);
        }
        spec
    }
}

/// Numeric input mapped to the native Slider.
#[derive(Clone)]
pub struct NumberInput {
    config: InputConfig<f64>,
    min: f64,
    max: f64,
}
impl NumberInput {
    /// Creates an uncontrolled number input with the default Slider range `0..=100`.
    pub fn new(value: f64) -> Self {
        Self {
            config: InputConfig::uncontrolled(value, None),
            min: 0.0,
            max: 100.0,
        }
    }
    pub fn controlled(mut self, value: State<f64>, on_change: Action<f64>) -> Self {
        self.config = InputConfig::controlled(value, on_change);
        self
    }
    pub fn uncontrolled(mut self, initial: f64, on_change: Option<Action<f64>>) -> Self {
        self.config = InputConfig::uncontrolled(initial, on_change);
        self
    }
    pub fn with_config(mut self, config: InputConfig<f64>) -> Self {
        self.config = config;
        self
    }
    /// Sets the inclusive native Slider range.
    pub fn range(mut self, min: f64, max: f64) -> Self {
        self.min = min;
        self.max = max;
        self
    }
}
impl Element for NumberInput {
    fn describe(&self, _cx: &mut crate::view::BuildCx) -> crate::view::ElementSpec {
        let mut accessibility = crate::view::Accessibility::new()
            .role(crate::view::AccessibilityRole::Slider)
            .action(crate::view::AccessibilityAction::Focus)
            .action(crate::view::AccessibilityAction::SetValue);
        let value = match &self.config {
            InputConfig::Controlled { value, .. } => value.get().ok(),
            InputConfig::Uncontrolled { initial, .. } => Some(*initial),
        };
        if let Some(value) = value {
            accessibility = accessibility.value(value.to_string());
        }
        crate::view::ElementSpec::custom("number-input", [])
            .accessibility(accessibility)
            .on(crate::view::Event::Change, 0)
            .with_number_input(&self.config, self.min, self.max)
    }
}

/// Choice input mapped to the native Dropdown.
#[derive(Clone)]
pub struct Select {
    options: Vec<String>,
    config: InputConfig<ChoiceId>,
}
impl Select {
    /// Creates an uncontrolled Select; `selected` is its zero-based mount-only initial choice.
    pub fn new(options: impl IntoIterator<Item = impl Into<String>>, selected: usize) -> Self {
        Self {
            options: options.into_iter().map(Into::into).collect(),
            config: InputConfig::uncontrolled(ChoiceId::new(selected), None),
        }
    }
    pub fn controlled(mut self, value: State<ChoiceId>, on_change: Action<ChoiceId>) -> Self {
        self.config = InputConfig::controlled(value, on_change);
        self
    }
    pub fn uncontrolled(mut self, initial: ChoiceId, on_change: Option<Action<ChoiceId>>) -> Self {
        self.config = InputConfig::uncontrolled(initial, on_change);
        self
    }
    pub fn with_config(mut self, config: InputConfig<ChoiceId>) -> Self {
        self.config = config;
        self
    }
}
impl Element for Select {
    fn describe(&self, _cx: &mut crate::view::BuildCx) -> crate::view::ElementSpec {
        let mut accessibility = crate::view::Accessibility::new()
            .role(crate::view::AccessibilityRole::List)
            .action(crate::view::AccessibilityAction::Click)
            .action(crate::view::AccessibilityAction::Focus);
        let selected = match &self.config {
            InputConfig::Controlled { value, .. } => value.get().ok(),
            InputConfig::Uncontrolled { initial, .. } => Some(*initial),
        };
        if let Some(selected) = selected {
            accessibility = accessibility.value(selected.index().to_string());
        }
        crate::view::ElementSpec::custom("select", [])
            .accessibility(accessibility)
            .on(crate::view::Event::Change, 0)
            .with_choice_input(self.options.clone(), &self.config)
    }
}

/// Column metadata for a facade [`DataTable`].
#[derive(Clone, Debug, PartialEq)]
pub struct TableColumn {
    pub id: String,
    pub title: String,
    pub width: f64,
}

impl TableColumn {
    pub fn new(id: impl Into<String>, title: impl Into<String>, width: f64) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            width: width.max(40.0),
        }
    }
}

type TableCellFn = Arc<dyn Fn(usize, usize) -> String + Send + Sync>;

/// Cloneable cell text factory for virtual table rows.
#[derive(Clone)]
pub(crate) struct TableCellFactory {
    factory: TableCellFn,
}

impl std::fmt::Debug for TableCellFactory {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("TableCellFactory(..)")
    }
}

impl TableCellFactory {
    pub(crate) fn new(factory: impl Fn(usize, usize) -> String + Send + Sync + 'static) -> Self {
        Self {
            factory: Arc::new(factory),
        }
    }

    pub(crate) fn cell(&self, row: usize, column: usize) -> String {
        (self.factory)(row, column)
    }
}

/// Runtime virtualized data table (Phase 44). Mounts a native DataTable host.
pub struct DataTable {
    columns: Vec<TableColumn>,
    row_count: usize,
    freeze_leading: usize,
    filter_query: String,
    cell: TableCellFactory,
    mount_cells: bool,
    /// Merge spans as `(data_row, data_col, row_span, col_span)`.
    spans: Vec<(usize, usize, usize, usize)>,
    mounted_row_index: Option<usize>,
    mounted_column_index: Option<usize>,
    on_select: Option<Action<crate::model::SelectionChange>>,
    on_sort: Option<Action<Vec<crate::model::SortDescriptor>>>,
    on_edit: Option<Action<(usize, usize, String)>>,
}

impl Default for DataTable {
    fn default() -> Self {
        Self::new()
    }
}

impl DataTable {
    /// Creates an empty table; call [`Self::columns`] and [`Self::rows`] before mount.
    pub fn new() -> Self {
        Self {
            columns: Vec::new(),
            row_count: 0,
            freeze_leading: 0,
            filter_query: String::new(),
            cell: TableCellFactory::new(|row, col| format!("{row},{col}")),
            mount_cells: true,
            spans: Vec::new(),
            mounted_row_index: None,
            mounted_column_index: None,
            on_select: None,
            on_sort: None,
            on_edit: None,
        }
    }

    /// Declares logical dimensions with synthetic columns (a11y / smoke helpers).
    pub fn logical_dimensions(mut self, row_count: usize, column_count: usize) -> Self {
        self.row_count = row_count;
        self.columns = (0..column_count)
            .map(|index| TableColumn::new(format!("c{index}"), format!("Col {index}"), 80.0))
            .collect();
        self
    }

    /// Declares the logical coordinate represented by this mounted table window.
    pub fn mounted_position(mut self, row_index: usize, column_index: usize) -> Self {
        self.mounted_row_index = Some(row_index);
        self.mounted_column_index = Some(column_index);
        self
    }

    pub fn columns(mut self, columns: impl IntoIterator<Item = TableColumn>) -> Self {
        self.columns = columns.into_iter().collect();
        self
    }

    pub fn rows(mut self, row_count: usize) -> Self {
        self.row_count = row_count;
        self
    }

    pub fn freeze_leading(mut self, count: usize) -> Self {
        self.freeze_leading = count;
        self
    }

    pub fn filter(mut self, query: impl Into<String>) -> Self {
        self.filter_query = query.into();
        self
    }

    pub fn cell(
        mut self,
        factory: impl Fn(usize, usize) -> String + Send + Sync + 'static,
    ) -> Self {
        self.cell = TableCellFactory::new(factory);
        self
    }

    /// When true (default), viewport cells mount as Label widgets for virt bounds.
    pub fn mount_cells(mut self, enabled: bool) -> Self {
        self.mount_cells = enabled;
        self
    }

    pub fn on_select(mut self, handler: Action<crate::model::SelectionChange>) -> Self {
        self.on_select = Some(handler);
        self
    }

    pub fn on_sort(mut self, handler: Action<Vec<crate::model::SortDescriptor>>) -> Self {
        self.on_sort = Some(handler);
        self
    }

    pub fn on_edit(mut self, handler: Action<(usize, usize, String)>) -> Self {
        self.on_edit = Some(handler);
        self
    }

    /// Declares a merge span in data coordinates `(row, col, row_span, col_span)`.
    pub fn span(mut self, row: usize, col: usize, row_span: usize, col_span: usize) -> Self {
        self.spans.push((row, col, row_span, col_span));
        self
    }

    /// Replaces merge spans (data coordinates).
    pub fn spans(mut self, spans: impl IntoIterator<Item = (usize, usize, usize, usize)>) -> Self {
        self.spans = spans.into_iter().collect();
        self
    }
}

impl Element for DataTable {
    fn describe(&self, _cx: &mut crate::view::BuildCx) -> crate::view::ElementSpec {
        let column_count = self.columns.len();
        let mut accessibility = crate::view::Accessibility::new()
            .role(crate::view::AccessibilityRole::Grid)
            .action(crate::view::AccessibilityAction::Focus)
            .row_count(self.row_count)
            .column_count(column_count);
        if let Some(value) = self.mounted_row_index {
            accessibility = accessibility.row_index(value);
        }
        if let Some(value) = self.mounted_column_index {
            accessibility = accessibility.column_index(value);
        }
        let encoded_columns = self
            .columns
            .iter()
            .map(|column| format!("{}|{}|{}", column.id, column.title, column.width))
            .collect::<Vec<_>>()
            .join(";");
        let encoded_spans = self
            .spans
            .iter()
            .map(|(r, c, rs, cs)| format!("{r}|{c}|{rs}|{cs}"))
            .collect::<Vec<_>>()
            .join(";");
        let mut spec = crate::view::ElementSpec::custom("data-table", [])
            .property("row-count", self.row_count.to_string())
            .property("freeze-leading", self.freeze_leading.to_string())
            .property("filter-query", self.filter_query.clone())
            .property("columns", encoded_columns)
            .property("spans", encoded_spans)
            .property(
                "mount-cells",
                if self.mount_cells { "true" } else { "false" },
            )
            .with_table_cell(self.cell.clone())
            .on(crate::view::Event::Change, 0)
            .accessibility(accessibility);
        if let Some(handler) = self.on_select.clone() {
            spec = spec.on_table_select(handler);
        }
        if let Some(handler) = self.on_sort.clone() {
            spec = spec.on_table_sort(handler);
        }
        if let Some(handler) = self.on_edit.clone() {
            spec = spec.on_table_edit(handler);
        }
        spec
    }
}

/// Clipped vertical scroll viewport. Children are full-mounted; sticky/fixed
/// positioning uses [`Styled`] CSS metadata (Phase 43 Wave A).
pub struct Scroll {
    children: Vec<crate::view::Node>,
    scroll_chaining: bool,
    background: Option<String>,
}

impl Scroll {
    /// Creates a scroll viewport with the given child nodes.
    pub fn new(children: impl IntoIterator<Item = crate::view::Node>) -> Self {
        Self {
            children: children.into_iter().collect(),
            scroll_chaining: true,
            background: None,
        }
    }

    /// When true (default), wheel events at a scroll edge chain to ancestors.
    pub fn scroll_chaining(mut self, enabled: bool) -> Self {
        self.scroll_chaining = enabled;
        self
    }

    /// Viewport fill as `#RRGGBB` or `#RRGGBBAA` (optional leading `#`).
    pub fn background(mut self, color: impl Into<String>) -> Self {
        self.background = Some(color.into());
        self
    }
}

impl Element for Scroll {
    fn describe(&self, _cx: &mut crate::view::BuildCx) -> crate::view::ElementSpec {
        let mut spec = crate::view::ElementSpec::custom("scroll", self.children.clone())
            .property(
                "scroll-chaining",
                if self.scroll_chaining {
                    "true"
                } else {
                    "false"
                },
            )
            .accessibility(
                crate::view::Accessibility::new()
                    .role(crate::view::AccessibilityRole::Generic)
                    .name("Scroll"),
            );
        if let Some(color) = &self.background {
            spec = spec.property("background", color.clone());
        }
        spec
    }
}

/// Style host for sticky/fixed/z-index child metadata (CSS subset via StyleInput).
pub struct Styled {
    css: String,
    children: Vec<crate::view::Node>,
    on_press: Option<Action<()>>,
    tooltip: Option<String>,
}

impl Styled {
    /// Wraps children in a styled box parsed from non-panicking CSS text.
    pub fn new(
        css: impl Into<String>,
        children: impl IntoIterator<Item = crate::view::Node>,
    ) -> Self {
        Self {
            css: css.into(),
            children: children.into_iter().collect(),
            on_press: None,
            tooltip: None,
        }
    }

    /// Makes the styled host itself pressable (children still paint, but do not steal the hit).
    pub fn on_press(mut self, handler: Action<()>) -> Self {
        self.on_press = Some(handler);
        self
    }

    /// Hover tip text (short delay; non-interactive overlay).
    pub fn tooltip(mut self, text: impl Into<String>) -> Self {
        self.tooltip = Some(text.into());
        self
    }

    /// Column flex host: children are direct flex items so `gap` / `align-items` apply.
    ///
    /// Prefixed with `flex-direction:column` when the caller CSS omits it.
    pub fn column(
        css: impl Into<String>,
        children: impl IntoIterator<Item = crate::view::Node>,
    ) -> Self {
        Self::new(Self::with_flex_direction(css.into(), "column"), children)
    }

    /// Row flex host: children are direct flex items so `gap` / `justify-content` apply.
    ///
    /// Prefixed with `flex-direction:row` when the caller CSS omits it.
    pub fn row(
        css: impl Into<String>,
        children: impl IntoIterator<Item = crate::view::Node>,
    ) -> Self {
        Self::new(Self::with_flex_direction(css.into(), "row"), children)
    }

    fn with_flex_direction(css: String, direction: &str) -> String {
        let trimmed = css.trim();
        if trimmed.is_empty() {
            return format!("flex-direction:{direction}");
        }
        if trimmed
            .split(';')
            .map(str::trim)
            .any(|decl| decl.starts_with("flex-direction"))
        {
            css
        } else {
            format!("flex-direction:{direction};{css}")
        }
    }

    /// Convenience for a single sticky top bar (vertical sticky MVP).
    pub fn sticky_top(top: f32, child: crate::view::Node) -> Self {
        Self::new(
            format!(
                "position:sticky;top:{top};z-index:5;padding:12;background:surface-container;width:fill"
            ),
            [child],
        )
    }
}

impl Element for Styled {
    fn describe(&self, _cx: &mut crate::view::BuildCx) -> crate::view::ElementSpec {
        let mut accessibility =
            crate::view::Accessibility::new().role(crate::view::AccessibilityRole::Generic);
        if self.on_press.is_some() {
            accessibility = crate::view::Accessibility::new()
                .role(crate::view::AccessibilityRole::Button)
                .action(crate::view::AccessibilityAction::Click);
        }
        if let Some(tip) = &self.tooltip {
            accessibility = accessibility.name(tip.clone());
        }
        let mut spec = crate::view::ElementSpec::custom("styled", self.children.clone())
            .property("css", self.css.clone())
            .accessibility(accessibility);
        if let Some(tip) = &self.tooltip {
            spec = spec.property("tooltip", tip.clone());
        }
        if let Some(handler) = &self.on_press {
            spec = spec
                .on(crate::view::Event::Press, 0)
                .on_press(handler.clone());
        }
        spec
    }
}

type ItemLabelFn = Arc<dyn Fn(usize) -> String + Send + Sync>;

/// Cloneable label factory for virtual list rows.
#[derive(Clone)]
pub(crate) struct ItemLabelFactory {
    factory: ItemLabelFn,
}

impl std::fmt::Debug for ItemLabelFactory {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ItemLabelFactory(..)")
    }
}

impl ItemLabelFactory {
    pub(crate) fn new(factory: impl Fn(usize) -> String + Send + Sync + 'static) -> Self {
        Self {
            factory: Arc::new(factory),
        }
    }

    pub(crate) fn label(&self, index: usize) -> String {
        (self.factory)(index)
    }
}

/// Virtualized list: fixed row height MVP with optional measured heights.
pub struct VirtualList {
    row_count: usize,
    item_extent: f64,
    measure_heights: bool,
    item_label: ItemLabelFactory,
    on_select: Option<Action<usize>>,
    on_activate: Option<Action<usize>>,
    on_context: Option<Action<(usize, f64, f64)>>,
}

impl VirtualList {
    /// Creates a virtual list with `row_count` logical rows and default 32px extent.
    pub fn new(row_count: usize) -> Self {
        Self {
            row_count,
            item_extent: 32.0,
            measure_heights: false,
            item_label: ItemLabelFactory::new(|index| format!("Item {index}")),
            on_select: None,
            on_activate: None,
            on_context: None,
        }
    }

    /// Sets the fixed row height in logical pixels.
    pub fn item_extent(mut self, extent: f64) -> Self {
        self.item_extent = extent.max(1.0);
        self
    }

    /// Enables measure-driven variable row heights (optional; not required for Wave A).
    pub fn measure_heights(mut self, enabled: bool) -> Self {
        self.measure_heights = enabled;
        self
    }

    /// Supplies row label text for the mounted viewport window.
    pub fn item(mut self, factory: impl Fn(usize) -> String + Send + Sync + 'static) -> Self {
        self.item_label = ItemLabelFactory::new(factory);
        self
    }

    /// Invoked when the primary selection changes (click / arrow keys).
    pub fn on_select(mut self, handler: Action<usize>) -> Self {
        self.on_select = Some(handler);
        self
    }

    /// Invoked when a row is activated (Enter / double-activate path).
    pub fn on_activate(mut self, handler: Action<usize>) -> Self {
        self.on_activate = Some(handler);
        self
    }

    /// Invoked on secondary-click with `(row_index, x, y)`. The host also opens the
    /// first mounted [`ContextMenu`] at that point when present.
    pub fn on_context(mut self, handler: Action<(usize, f64, f64)>) -> Self {
        self.on_context = Some(handler);
        self
    }
}

impl Element for VirtualList {
    fn describe(&self, _cx: &mut crate::view::BuildCx) -> crate::view::ElementSpec {
        let accessibility = crate::view::Accessibility::new()
            .role(crate::view::AccessibilityRole::List)
            .action(crate::view::AccessibilityAction::Focus)
            .row_count(self.row_count);
        let mut spec = crate::view::ElementSpec::custom("virtual-list", [])
            .property("row-count", self.row_count.to_string())
            .property("item-extent", self.item_extent.to_string())
            .property(
                "measure-heights",
                if self.measure_heights {
                    "true"
                } else {
                    "false"
                },
            )
            .with_item_label(self.item_label.clone())
            .on(crate::view::Event::Change, 0)
            .accessibility(accessibility);
        if let Some(handler) = self.on_select.clone() {
            spec = spec.on_list_select(handler);
        }
        if let Some(handler) = self.on_activate.clone() {
            spec = spec.on_list_activate(handler);
        }
        if let Some(handler) = self.on_context.clone() {
            spec = spec.on_list_context(handler);
        }
        spec
    }
}

/// Tree node description for [`VirtualTree`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeNode {
    pub id: String,
    pub label: String,
    pub children: Vec<TreeNode>,
}

impl TreeNode {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            children: Vec::new(),
        }
    }

    pub fn with_children(mut self, children: impl IntoIterator<Item = TreeNode>) -> Self {
        self.children = children.into_iter().collect();
        self
    }
}

/// Virtualized tree Element (Phase 44).
pub struct VirtualTree {
    roots: Vec<TreeNode>,
    on_select: Option<Action<String>>,
    on_activate: Option<Action<String>>,
    on_expand: Option<Action<(String, bool)>>,
}

impl VirtualTree {
    pub fn new(roots: impl IntoIterator<Item = TreeNode>) -> Self {
        Self {
            roots: roots.into_iter().collect(),
            on_select: None,
            on_activate: None,
            on_expand: None,
        }
    }

    pub fn on_select(mut self, handler: Action<String>) -> Self {
        self.on_select = Some(handler);
        self
    }

    pub fn on_activate(mut self, handler: Action<String>) -> Self {
        self.on_activate = Some(handler);
        self
    }

    pub fn on_expand(mut self, handler: Action<(String, bool)>) -> Self {
        self.on_expand = Some(handler);
        self
    }
}

impl Element for VirtualTree {
    fn describe(&self, _cx: &mut crate::view::BuildCx) -> crate::view::ElementSpec {
        let flat_count = count_tree_nodes(&self.roots);
        let accessibility = crate::view::Accessibility::new()
            .role(crate::view::AccessibilityRole::List)
            .action(crate::view::AccessibilityAction::Focus)
            .row_count(flat_count);
        let mut spec = crate::view::ElementSpec::custom("virtual-tree", [])
            .with_tree_roots(self.roots.clone())
            .on(crate::view::Event::Change, 0)
            .accessibility(accessibility);
        if let Some(handler) = self.on_select.clone() {
            spec = spec.on_tree_select(handler);
        }
        if let Some(handler) = self.on_activate.clone() {
            spec = spec.on_tree_activate(handler);
        }
        if let Some(handler) = self.on_expand.clone() {
            spec = spec.on_tree_expand(handler);
        }
        spec
    }
}

fn count_tree_nodes(nodes: &[TreeNode]) -> usize {
    nodes
        .iter()
        .map(|node| 1 + count_tree_nodes(&node.children))
        .sum()
}

/// Split orientation for [`SplitPane`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

/// Fixed two-pane splitter (Phase 44; not a floating dock).
pub struct SplitPane {
    direction: SplitDirection,
    ratio: f32,
    primary: crate::view::Node,
    secondary: crate::view::Node,
    on_ratio: Option<Action<f64>>,
    divider_color: Option<String>,
    divider_css: Option<String>,
}

impl SplitPane {
    pub fn new(
        direction: SplitDirection,
        primary: crate::view::Node,
        secondary: crate::view::Node,
    ) -> Self {
        Self {
            direction,
            ratio: 0.35,
            primary,
            secondary,
            on_ratio: None,
            divider_color: None,
            divider_css: None,
        }
    }

    pub fn horizontal(primary: crate::view::Node, secondary: crate::view::Node) -> Self {
        Self::new(SplitDirection::Horizontal, primary, secondary)
    }

    pub fn vertical(primary: crate::view::Node, secondary: crate::view::Node) -> Self {
        Self::new(SplitDirection::Vertical, primary, secondary)
    }

    pub fn ratio(mut self, ratio: f32) -> Self {
        self.ratio = ratio.clamp(0.05, 0.95);
        self
    }

    pub fn on_ratio(mut self, handler: Action<f64>) -> Self {
        self.on_ratio = Some(handler);
        self
    }

    /// Divider fill as `#RRGGBB` or `#RRGGBBAA` (optional leading `#`).
    pub fn divider_color(mut self, color: impl Into<String>) -> Self {
        self.divider_color = Some(color.into());
        self
    }

    /// CSS-subset for the divider strip (e.g. `border-right:1px solid #888888`).
    ///
    /// With only per-side borders (no `background`), the default thick fill is omitted.
    pub fn divider_css(mut self, css: impl Into<String>) -> Self {
        self.divider_css = Some(css.into());
        self
    }
}

impl Element for SplitPane {
    fn describe(&self, _cx: &mut crate::view::BuildCx) -> crate::view::ElementSpec {
        let direction = match self.direction {
            SplitDirection::Horizontal => "horizontal",
            SplitDirection::Vertical => "vertical",
        };
        let mut spec = crate::view::ElementSpec::custom(
            "split-pane",
            [self.primary.clone(), self.secondary.clone()],
        )
        .property("direction", direction)
        .property("ratio", self.ratio.to_string())
        .accessibility(
            crate::view::Accessibility::new()
                .role(crate::view::AccessibilityRole::Generic)
                .name("Split"),
        );
        if let Some(color) = &self.divider_color {
            spec = spec.property("divider-color", color.clone());
        }
        if let Some(css) = &self.divider_css {
            spec = spec.property("divider-css", css.clone());
        }
        if let Some(handler) = self.on_ratio.clone() {
            spec = spec.on_split_ratio(handler);
        }
        spec
    }
}

/// Segmented tab bar (Phase 44). Pane content is application-composed.
pub struct TabBar {
    labels: Vec<String>,
    selected: usize,
    on_change: Option<Action<usize>>,
    background: Option<String>,
}

impl TabBar {
    pub fn new(labels: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            labels: labels.into_iter().map(Into::into).collect(),
            selected: 0,
            on_change: None,
            background: None,
        }
    }

    pub fn selected(mut self, index: usize) -> Self {
        self.selected = index;
        self
    }

    pub fn on_change(mut self, handler: Action<usize>) -> Self {
        self.on_change = Some(handler);
        self
    }

    /// Bar fill as `#RRGGBB` or `#RRGGBBAA`.
    pub fn background(mut self, color: impl Into<String>) -> Self {
        self.background = Some(color.into());
        self
    }
}

impl Element for TabBar {
    fn describe(&self, _cx: &mut crate::view::BuildCx) -> crate::view::ElementSpec {
        let labels = self.labels.join("|");
        let mut accessibility = crate::view::Accessibility::new()
            .role(crate::view::AccessibilityRole::Tab)
            .action(crate::view::AccessibilityAction::Focus);
        if let Some(label) = self.labels.get(self.selected) {
            accessibility = accessibility.value(label.clone());
        }
        let mut spec = crate::view::ElementSpec::custom("tab-bar", [])
            .property("labels", labels)
            .property("selected", self.selected.to_string())
            .on(crate::view::Event::Change, 0)
            .accessibility(accessibility);
        if let Some(color) = &self.background {
            spec = spec.property("background", color.clone());
        }
        if let Some(handler) = self.on_change.clone() {
            spec = spec.on_tab_change(handler);
        }
        spec
    }
}

/// Rich text / block document editor host (Phase 45 Wave C).
pub struct RichDocument {
    document: synui_core::BlockDoc,
    on_change: Option<Action<synui_core::BlockDoc>>,
}

impl RichDocument {
    pub fn new(document: synui_core::BlockDoc) -> Self {
        Self {
            document,
            on_change: None,
        }
    }

    pub fn on_change(mut self, handler: Action<synui_core::BlockDoc>) -> Self {
        self.on_change = Some(handler);
        self
    }
}

impl Element for RichDocument {
    fn describe(&self, _cx: &mut crate::view::BuildCx) -> crate::view::ElementSpec {
        let mut spec = crate::view::ElementSpec::custom("rich-document", [])
            .with_rich_doc(self.document.clone())
            .accessibility(
                crate::view::Accessibility::new()
                    .role(crate::view::AccessibilityRole::TextInput)
                    .name("Rich document")
                    .value(self.document.plain_text())
                    .action(crate::view::AccessibilityAction::Focus)
                    .action(crate::view::AccessibilityAction::SetValue),
            );
        if let Some(handler) = self.on_change.clone() {
            spec = spec.on_rich_change(handler);
        }
        spec
    }
}

/// Top application bar (Phase 45).
pub struct AppBar {
    title: String,
    trailing: Vec<crate::view::Node>,
    background: Option<String>,
}

impl AppBar {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            trailing: Vec::new(),
            background: None,
        }
    }

    pub fn trailing(mut self, children: impl IntoIterator<Item = crate::view::Node>) -> Self {
        self.trailing = children.into_iter().collect();
        self
    }

    /// Bar fill as `#RRGGBB` or `#RRGGBBAA`.
    pub fn background(mut self, color: impl Into<String>) -> Self {
        self.background = Some(color.into());
        self
    }
}

impl Element for AppBar {
    fn describe(&self, _cx: &mut crate::view::BuildCx) -> crate::view::ElementSpec {
        let mut spec = crate::view::ElementSpec::custom("app-bar", self.trailing.clone())
            .property("title", self.title.clone())
            .accessibility(
                crate::view::Accessibility::new()
                    .role(crate::view::AccessibilityRole::Generic)
                    .name(self.title.clone()),
            );
        if let Some(color) = &self.background {
            spec = spec.property("background", color.clone());
        }
        spec
    }
}

/// Side navigation drawer (Phase 45).
pub struct NavigationDrawer {
    open: bool,
    children: Vec<crate::view::Node>,
    on_open_change: Option<Action<bool>>,
    background: Option<String>,
}

impl NavigationDrawer {
    pub fn new(children: impl IntoIterator<Item = crate::view::Node>) -> Self {
        Self {
            open: false,
            children: children.into_iter().collect(),
            on_open_change: None,
            background: None,
        }
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub fn on_open_change(mut self, handler: Action<bool>) -> Self {
        self.on_open_change = Some(handler);
        self
    }

    /// Drawer rail fill as `#RRGGBB` or `#RRGGBBAA`.
    pub fn background(mut self, color: impl Into<String>) -> Self {
        self.background = Some(color.into());
        self
    }
}

impl Element for NavigationDrawer {
    fn describe(&self, _cx: &mut crate::view::BuildCx) -> crate::view::ElementSpec {
        let mut spec = crate::view::ElementSpec::custom("navigation-drawer", self.children.clone())
            .property("open", if self.open { "true" } else { "false" })
            .accessibility(
                crate::view::Accessibility::new()
                    .role(crate::view::AccessibilityRole::Generic)
                    .name("Navigation drawer"),
            );
        if let Some(color) = &self.background {
            spec = spec.property("background", color.clone());
        }
        if let Some(handler) = self.on_open_change.clone() {
            spec = spec.on_drawer_change(handler);
        }
        spec
    }
}

/// Bottom navigation bar (Phase 45).
pub struct BottomNavigation {
    labels: Vec<String>,
    selected: usize,
    on_change: Option<Action<usize>>,
    background: Option<String>,
}

impl BottomNavigation {
    pub fn new(labels: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            labels: labels.into_iter().map(Into::into).collect(),
            selected: 0,
            on_change: None,
            background: None,
        }
    }

    pub fn selected(mut self, index: usize) -> Self {
        self.selected = index;
        self
    }

    pub fn on_change(mut self, handler: Action<usize>) -> Self {
        self.on_change = Some(handler);
        self
    }

    /// Bar fill as `#RRGGBB` or `#RRGGBBAA`.
    pub fn background(mut self, color: impl Into<String>) -> Self {
        self.background = Some(color.into());
        self
    }
}

impl Element for BottomNavigation {
    fn describe(&self, _cx: &mut crate::view::BuildCx) -> crate::view::ElementSpec {
        let labels = self.labels.join("|");
        let mut spec = crate::view::ElementSpec::custom("bottom-navigation", [])
            .property("labels", labels)
            .property("selected", self.selected.to_string())
            .on(crate::view::Event::Change, 0)
            .accessibility(
                crate::view::Accessibility::new()
                    .role(crate::view::AccessibilityRole::Tab)
                    .name("Bottom navigation"),
            );
        if let Some(color) = &self.background {
            spec = spec.property("background", color.clone());
        }
        if let Some(handler) = self.on_change.clone() {
            spec = spec.on_tab_change(handler);
        }
        spec
    }
}

/// Modal dialog (Phase 45).
pub struct Dialog {
    title: String,
    open: bool,
    children: Vec<crate::view::Node>,
    on_dismiss: Option<Action<()>>,
}

impl Dialog {
    pub fn new(
        title: impl Into<String>,
        children: impl IntoIterator<Item = crate::view::Node>,
    ) -> Self {
        Self {
            title: title.into(),
            open: false,
            children: children.into_iter().collect(),
            on_dismiss: None,
        }
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub fn on_dismiss(mut self, handler: Action<()>) -> Self {
        self.on_dismiss = Some(handler);
        self
    }
}

impl Element for Dialog {
    fn describe(&self, _cx: &mut crate::view::BuildCx) -> crate::view::ElementSpec {
        let mut spec = crate::view::ElementSpec::custom("dialog", self.children.clone())
            .property("title", self.title.clone())
            .property("open", if self.open { "true" } else { "false" })
            .accessibility(
                crate::view::Accessibility::new()
                    .role(crate::view::AccessibilityRole::Generic)
                    .name(self.title.clone()),
            );
        if let Some(handler) = self.on_dismiss.clone() {
            spec = spec.on_overlay_dismiss(handler);
        }
        spec
    }
}

/// Bottom sheet overlay (Phase 45).
pub struct BottomSheet {
    open: bool,
    children: Vec<crate::view::Node>,
    on_dismiss: Option<Action<()>>,
}

impl BottomSheet {
    pub fn new(children: impl IntoIterator<Item = crate::view::Node>) -> Self {
        Self {
            open: false,
            children: children.into_iter().collect(),
            on_dismiss: None,
        }
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub fn on_dismiss(mut self, handler: Action<()>) -> Self {
        self.on_dismiss = Some(handler);
        self
    }
}

impl Element for BottomSheet {
    fn describe(&self, _cx: &mut crate::view::BuildCx) -> crate::view::ElementSpec {
        let mut spec = crate::view::ElementSpec::custom("bottom-sheet", self.children.clone())
            .property("open", if self.open { "true" } else { "false" })
            .accessibility(
                crate::view::Accessibility::new()
                    .role(crate::view::AccessibilityRole::Generic)
                    .name("Bottom sheet"),
            );
        if let Some(handler) = self.on_dismiss.clone() {
            spec = spec.on_overlay_dismiss(handler);
        }
        spec
    }
}

/// Lottie animation host. Prefer [`Self::from_asset`] with [`crate::platform::Window::with_lottie`].
#[derive(Clone)]
pub struct Lottie {
    json: Option<String>,
    asset_id: Option<String>,
    width: f64,
    height: f64,
    playing: bool,
}

impl Lottie {
    /// Creates a Lottie host from Bodymovin JSON text (validated when mounted).
    ///
    /// Alias of [`Self::from_json`].
    pub fn new(json: impl Into<String>) -> Self {
        Self::from_json(json)
    }

    /// Creates a Lottie host from Bodymovin JSON text (validated when mounted).
    pub fn from_json(json: impl Into<String>) -> Self {
        Self {
            json: Some(json.into()),
            asset_id: None,
            width: 160.0,
            height: 160.0,
            playing: true,
        }
    }

    /// Creates a Lottie host that resolves JSON from a host-preloaded asset id.
    ///
    /// Requires [`crate::platform::Window::with_lottie`] with the same `id` before run.
    /// A missing id paints nothing (no panic).
    pub fn from_asset(id: impl Into<String>) -> Self {
        Self {
            json: None,
            asset_id: Some(id.into()),
            width: 160.0,
            height: 160.0,
            playing: true,
        }
    }

    pub fn size(mut self, width: f64, height: f64) -> Self {
        self.width = width.max(1.0);
        self.height = height.max(1.0);
        self
    }

    pub fn playing(mut self, playing: bool) -> Self {
        self.playing = playing;
        self
    }
}

impl Element for Lottie {
    fn describe(&self, _cx: &mut crate::view::BuildCx) -> crate::view::ElementSpec {
        let mut spec = crate::view::ElementSpec::custom("lottie", [])
            .property("width", self.width.to_string())
            .property("height", self.height.to_string())
            .property("playing", if self.playing { "true" } else { "false" })
            .accessibility(
                crate::view::Accessibility::new()
                    .role(crate::view::AccessibilityRole::Generic)
                    .name("Animation"),
            );
        // Asset id and inline JSON are mutually exclusive.
        if let Some(id) = &self.asset_id {
            spec = spec.property("id", id.clone());
        } else if let Some(json) = &self.json {
            spec = spec.property("json", json.clone());
        }
        spec
    }
}

/// Morphing MD3 shape host.
#[derive(Clone)]
pub struct MorphingShape {
    size: f64,
    playing: bool,
    speed: f32,
}

impl MorphingShape {
    /// Creates a morphing MD3 shape animation (playing by default).
    pub fn new() -> Self {
        Self {
            size: 120.0,
            playing: true,
            speed: 0.45,
        }
    }

    pub fn size(mut self, size: f64) -> Self {
        self.size = size.max(1.0);
        self
    }

    pub fn playing(mut self, playing: bool) -> Self {
        self.playing = playing;
        self
    }

    pub fn speed(mut self, speed: f32) -> Self {
        self.speed = speed.max(0.0);
        self
    }
}

impl Default for MorphingShape {
    fn default() -> Self {
        Self::new()
    }
}

impl Element for MorphingShape {
    fn describe(&self, _cx: &mut crate::view::BuildCx) -> crate::view::ElementSpec {
        crate::view::ElementSpec::custom("morphing-shape", [])
            .property("size", self.size.to_string())
            .property("playing", if self.playing { "true" } else { "false" })
            .property("speed", self.speed.to_string())
            .accessibility(
                crate::view::Accessibility::new()
                    .role(crate::view::AccessibilityRole::Generic)
                    .name("Shape animation"),
            )
    }
}

/// Transient snack message (Phase 45).
pub struct SnackBar {
    message: String,
    visible: bool,
    on_dismiss: Option<Action<()>>,
}

impl SnackBar {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            visible: false,
            on_dismiss: None,
        }
    }

    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    pub fn on_dismiss(mut self, handler: Action<()>) -> Self {
        self.on_dismiss = Some(handler);
        self
    }
}

impl Element for SnackBar {
    fn describe(&self, _cx: &mut crate::view::BuildCx) -> crate::view::ElementSpec {
        let mut spec = crate::view::ElementSpec::custom("snack-bar", [])
            .property("message", self.message.clone())
            .property("visible", if self.visible { "true" } else { "false" })
            .accessibility(
                crate::view::Accessibility::new()
                    .role(crate::view::AccessibilityRole::Generic)
                    .name(self.message.clone()),
            );
        if let Some(handler) = self.on_dismiss.clone() {
            spec = spec.on_overlay_dismiss(handler);
        }
        spec
    }
}

/// Named icon glyph (Phase 45).
///
/// Use a short alias (`"home"`) or canonical id (`"material-symbols:home"`).
/// Curated names load automatically; custom SVGs via [`crate::platform::Window::with_icon`].
/// Tint follows style cascade `color` (host `text_color`); see book `icons.md`.
pub struct Icon {
    name: String,
    size: f32,
    tooltip: Option<String>,
}

impl Icon {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            size: 24.0,
            tooltip: None,
        }
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// Hover tip text (short delay; non-interactive overlay).
    pub fn tooltip(mut self, text: impl Into<String>) -> Self {
        self.tooltip = Some(text.into());
        self
    }
}

impl Element for Icon {
    fn describe(&self, _cx: &mut crate::view::BuildCx) -> crate::view::ElementSpec {
        let mut spec = crate::view::ElementSpec::custom("icon", [])
            .property("name", self.name.clone())
            .property("size", self.size.to_string())
            .accessibility(
                crate::view::Accessibility::new()
                    .role(crate::view::AccessibilityRole::Generic)
                    .name(self.name.clone()),
            );
        if let Some(tip) = &self.tooltip {
            spec = spec.property("tooltip", tip.clone());
        }
        spec
    }
}

/// Text label Element (Phase 48). Prefer this when you need truncation metadata or an
/// explicit a11y name; [`crate::view::Node::text`] remains available for plain text.
pub struct Label {
    text: String,
    truncate: bool,
    /// Fill parent width and wrap lines (dialog / form body copy).
    wrap: bool,
    max_lines: Option<u32>,
    a11y_name: Option<String>,
}

impl Label {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            truncate: false,
            wrap: false,
            max_lines: None,
            a11y_name: None,
        }
    }

    /// Enables paint-width ellipsis when the label measures under a width budget.
    /// Multi-line line-budget clipping still applies in the facade adapter.
    pub fn truncate(mut self, truncate: bool) -> Self {
        self.truncate = truncate;
        self
    }

    /// Fills the parent width and wraps text (use for dialog body / long copy).
    pub fn wrap(mut self, wrap: bool) -> Self {
        self.wrap = wrap;
        self
    }

    /// Caps displayed lines when [`Self::truncate`] is enabled.
    pub fn max_lines(mut self, max_lines: u32) -> Self {
        self.max_lines = Some(max_lines);
        self
    }

    /// Overrides the accessibility name (defaults to the display text).
    pub fn a11y_name(mut self, name: impl Into<String>) -> Self {
        self.a11y_name = Some(name.into());
        self
    }
}

impl Element for Label {
    fn describe(&self, _cx: &mut crate::view::BuildCx) -> crate::view::ElementSpec {
        let a11y_name = self.a11y_name.clone().unwrap_or_else(|| self.text.clone());
        let mut spec = crate::view::ElementSpec::custom("label", [])
            .property("text", self.text.clone())
            .property("truncate", if self.truncate { "true" } else { "false" })
            .property("wrap", if self.wrap { "true" } else { "false" })
            .accessibility(
                crate::view::Accessibility::new()
                    .role(crate::view::AccessibilityRole::Label)
                    .name(a11y_name),
            );
        if let Some(max_lines) = self.max_lines {
            spec = spec.property("max_lines", max_lines.to_string());
        }
        spec
    }
}

/// Raster image by cache id (Phase 45). Preload via [`crate::platform::Window::with_image`].
///
/// `.svg` assets are stored as vectors and painted crisp at the widget size;
/// SVG `currentColor` is tinted with theme `on_surface` (same as [`Icon`]).
pub struct Image {
    id: String,
    width: f32,
    height: f32,
}

impl Image {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            width: 120.0,
            height: 120.0,
        }
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.width = width;
        self.height = height;
        self
    }
}

impl Element for Image {
    fn describe(&self, _cx: &mut crate::view::BuildCx) -> crate::view::ElementSpec {
        crate::view::ElementSpec::custom("image", [])
            .property("id", self.id.clone())
            .property("width", self.width.to_string())
            .property("height", self.height.to_string())
            .accessibility(
                crate::view::Accessibility::new()
                    .role(crate::view::AccessibilityRole::Generic)
                    .name(self.id.clone()),
            )
    }
}

/// One menu action item (Phase 46 in-app menu).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuItem {
    pub id: String,
    pub label: String,
    pub enabled: bool,
}

impl MenuItem {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            enabled: true,
        }
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

/// Top-level menu with a title and items (Phase 46).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Menu {
    pub title: String,
    pub items: Vec<MenuItem>,
}

impl Menu {
    pub fn new(title: impl Into<String>, items: impl IntoIterator<Item = MenuItem>) -> Self {
        Self {
            title: title.into(),
            items: items.into_iter().collect(),
        }
    }
}

/// In-app menu bar Element (Phase 46). Platform `DesktopMenu`/muda remains a shell option.
pub struct MenuBar {
    menus: Vec<Menu>,
    on_action: Option<Action<String>>,
}

impl MenuBar {
    pub fn new(menus: impl IntoIterator<Item = Menu>) -> Self {
        Self {
            menus: menus.into_iter().collect(),
            on_action: None,
        }
    }

    pub fn on_action(mut self, handler: Action<String>) -> Self {
        self.on_action = Some(handler);
        self
    }
}

impl Element for MenuBar {
    fn describe(&self, _cx: &mut crate::view::BuildCx) -> crate::view::ElementSpec {
        let mut spec = crate::view::ElementSpec::custom("menu-bar", [])
            .with_menu_bar(self.menus.clone())
            .on(crate::view::Event::Change, 0)
            .accessibility(
                crate::view::Accessibility::new()
                    .role(crate::view::AccessibilityRole::Generic)
                    .name("Menu bar")
                    .action(crate::view::AccessibilityAction::Focus),
            );
        if let Some(handler) = self.on_action.clone() {
            spec = spec.on_menu_action(handler);
        }
        spec
    }
}

/// In-app context menu Element (Phase 46). Opened by list secondary-click or [`Self::show_at`].
pub struct ContextMenu {
    items: Vec<MenuItem>,
    on_action: Option<Action<String>>,
    show_at: Option<(f64, f64)>,
}

impl ContextMenu {
    pub fn new(items: impl IntoIterator<Item = MenuItem>) -> Self {
        Self {
            items: items.into_iter().collect(),
            on_action: None,
            show_at: None,
        }
    }

    pub fn on_action(mut self, handler: Action<String>) -> Self {
        self.on_action = Some(handler);
        self
    }

    /// Requests the host to open the menu at logical coordinates (consumed once per distinct point).
    pub fn show_at(mut self, x: f64, y: f64) -> Self {
        self.show_at = Some((x, y));
        self
    }
}

impl Element for ContextMenu {
    fn describe(&self, _cx: &mut crate::view::BuildCx) -> crate::view::ElementSpec {
        let mut spec = crate::view::ElementSpec::custom("context-menu", [])
            .with_context_items(self.items.clone())
            .on(crate::view::Event::Change, 0)
            .accessibility(
                crate::view::Accessibility::new()
                    .role(crate::view::AccessibilityRole::Generic)
                    .name("Context menu"),
            );
        if let Some(handler) = self.on_action.clone() {
            spec = spec.on_menu_action(handler);
        }
        if let Some((x, y)) = self.show_at {
            spec = spec.with_context_show_at(x, y);
        }
        spec
    }
}
