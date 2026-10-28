//! Open declarative Element contracts and the private retained-Engine adapter.
//!
//! [`crate::view::Element`] is deliberately a trait instead of a public `View` enum. Components
//! outside this crate can therefore participate in the same node tree and lifecycle
//! as built-ins without depending on the legacy declarative implementation.

use crate::app::{Action, AppContext};
use crate::model::{SelectionChange, SortDescriptor};
use crate::widgets::{
    ButtonCallback, ChoiceId, InputConfig, InputConfigError, InputKind, InputValue,
    ItemLabelFactory, Menu, MenuItem, TableCellFactory, TreeNode,
};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use synui_core::{
    A11yAction, A11yOverride, A11yRole, AlignItems, Dimension, EventResult, JustifyContent, Point,
    WidgetId, WidgetStore,
};
use synui_layout::LayoutEngine;
use synui_render::{IconId, ImageId};
use synui_widgets::{
    AppBar as NativeAppBar, BottomNavigation as NativeBottomNavigation,
    BottomSheet as NativeBottomSheet, Button as NativeButton, Card as NativeCard,
    CellSpan as NativeCellSpan, Checkbox as NativeCheckbox,
    CircularProgress as NativeCircularProgress, ColumnDef as NativeColumnDef,
    ContextMenu as NativeContextMenu, DataTable as NativeDataTable, Dialog as NativeDialog,
    Dropdown as NativeDropdown, DropdownMenu, Flex, Icon as NativeIcon, Image as NativeImage,
    Label, LinearProgress as NativeLinearProgress, ListItem as NativeListItem,
    ListView as NativeListView, Lottie as NativeLottie, Menu as NativeMenu,
    MenuBar as NativeMenuBar, MenuItem as NativeMenuItem, MenuPanel,
    MorphingShape as NativeMorphingShape, NavigationDrawer as NativeNavigationDrawer,
    Radio as NativeRadio, RichEdit as NativeRichEdit, ScrollView as NativeScrollView,
    Slider as NativeSlider, SnackBar as NativeSnackBar, SplitDirection as NativeSplitDirection,
    SplitPane as NativeSplitPane, StyledBox as NativeStyledBox, Switch as NativeSwitch,
    TabBar as NativeTabBar, TextArea as NativeTextArea, TextField as NativeTextField,
    TreeNode as NativeTreeNode, TreeView as NativeTreeView,
};

static NEXT_APP_SEED: AtomicU64 = AtomicU64::new(1);

/// Stable application value used to identify an item in a repeated collection.
///
/// Business keys are scoped to one sibling collection. They are incorporated
/// into automatic identity, so keyed items retain state and handlers when their
/// collection is reordered.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BusinessKey(String);

impl BusinessKey {
    /// Creates a canonical string business key.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
    /// Creates a canonical signed-integer business key.
    pub fn integer(value: i64) -> Self {
        Self(format!("i:{value}"))
    }
    /// Creates a canonical unsigned-integer business key.
    pub fn unsigned(value: u64) -> Self {
        Self(format!("u:{value}"))
    }
    /// Returns the key's canonical application-defined value.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for BusinessKey {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for BusinessKey {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

/// Backward-compatible short name for [`BusinessKey`].
pub type Key = BusinessKey;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum IdentitySegment {
    Root,
    StaticSlot(usize),
    BusinessKey(BusinessKey),
}

/// App-local automatic identity assigned to a mounted Element.
///
/// Equality compares the full canonical path, not only its cached hash, so a
/// hash collision cannot associate state or actions with the wrong Element.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct IdentityPath {
    app_seed: u64,
    segments: Vec<IdentitySegment>,
    hash: u64,
}

impl IdentityPath {
    fn root(app_seed: u64, key: Option<&BusinessKey>) -> Self {
        let mut segments = vec![IdentitySegment::Root];
        if let Some(key) = key {
            segments.push(IdentitySegment::BusinessKey(key.clone()));
        }
        Self::from_segments(app_seed, segments)
    }

    fn child(&self, slot: usize, key: Option<&BusinessKey>) -> Self {
        let mut segments = self.segments.clone();
        segments.push(match key {
            Some(key) => IdentitySegment::BusinessKey(key.clone()),
            None => IdentitySegment::StaticSlot(slot),
        });
        Self::from_segments(self.app_seed, segments)
    }

    fn from_segments(app_seed: u64, segments: Vec<IdentitySegment>) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        app_seed.hash(&mut hasher);
        segments.hash(&mut hasher);
        let hash = hasher.finish();
        Self {
            app_seed,
            segments,
            hash,
        }
    }

    fn is_descendant_of(&self, parent: &Self) -> bool {
        self.app_seed == parent.app_seed && self.segments.starts_with(&parent.segments)
    }

    /// Returns the cached app-local path hash used by the runtime index.
    pub fn path_hash(&self) -> u64 {
        self.hash
    }
}

/// Events supported by facade elements.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Event {
    Press,
    Change,
    Custom(String),
}

/// A named event position declared by an [`ElementSpec`].
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct EventSlot {
    event: Event,
    slot: u16,
}

impl EventSlot {
    /// Creates a stable position for an event handler supplied by a later lifecycle layer.
    pub fn new(event: Event, slot: u16) -> Self {
        Self { event, slot }
    }
    /// Returns the event kind associated with this slot.
    pub fn event(&self) -> &Event {
        &self.event
    }
    /// Returns the element-local slot number.
    pub fn slot(&self) -> u16 {
        self.slot
    }
}

/// Stable semantic roles accepted by declarative Elements.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AccessibilityRole {
    Generic,
    Window,
    Button,
    Label,
    TextInput,
    Checkbox,
    Switch,
    Slider,
    ProgressBar,
    ListItem,
    List,
    Grid,
    Row,
    GridCell,
    Tab,
    Dialog,
    Status,
}

impl AccessibilityRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Generic => "generic",
            Self::Window => "window",
            Self::Button => "button",
            Self::Label => "label",
            Self::TextInput => "text-input",
            Self::Checkbox => "checkbox",
            Self::Switch => "switch",
            Self::Slider => "slider",
            Self::ProgressBar => "progress-bar",
            Self::ListItem => "list-item",
            Self::List => "list",
            Self::Grid => "grid",
            Self::Row => "row",
            Self::GridCell => "grid-cell",
            Self::Tab => "tab",
            Self::Dialog => "dialog",
            Self::Status => "status",
        }
    }
}

impl AsRef<str> for AccessibilityRole {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl AccessibilityRole {
    fn engine_role(self) -> A11yRole {
        match self {
            Self::Generic => A11yRole::Generic,
            Self::Window => A11yRole::Window,
            Self::Button => A11yRole::Button,
            Self::Label => A11yRole::Label,
            Self::TextInput => A11yRole::TextInput,
            Self::Checkbox => A11yRole::Checkbox,
            Self::Switch => A11yRole::Switch,
            Self::Slider => A11yRole::Slider,
            Self::ProgressBar => A11yRole::ProgressBar,
            Self::ListItem => A11yRole::ListItem,
            Self::List => A11yRole::List,
            Self::Grid => A11yRole::Grid,
            Self::Row => A11yRole::Row,
            Self::GridCell => A11yRole::GridCell,
            Self::Tab => A11yRole::Tab,
            Self::Dialog => A11yRole::Dialog,
            Self::Status => A11yRole::Status,
        }
    }
}

/// Defined validation error for an unsupported compatibility string role.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[error("unknown accessibility role {role:?}")]
pub struct AccessibilityRoleError {
    role: String,
}

impl AccessibilityRoleError {
    pub fn role(&self) -> &str {
        &self.role
    }
}

impl std::str::FromStr for AccessibilityRole {
    type Err = AccessibilityRoleError;

    fn from_str(role: &str) -> Result<Self, Self::Err> {
        match role.trim().to_ascii_lowercase().as_str() {
            "generic" | "group" => Ok(Self::Generic),
            "window" => Ok(Self::Window),
            "button" => Ok(Self::Button),
            "label" => Ok(Self::Label),
            "text-input" | "textbox" => Ok(Self::TextInput),
            "checkbox" | "check-box" => Ok(Self::Checkbox),
            "switch" => Ok(Self::Switch),
            "slider" => Ok(Self::Slider),
            "progress-bar" | "progressbar" => Ok(Self::ProgressBar),
            "list-item" | "listitem" => Ok(Self::ListItem),
            "list" => Ok(Self::List),
            "grid" | "table" => Ok(Self::Grid),
            "row" => Ok(Self::Row),
            "grid-cell" | "gridcell" | "cell" => Ok(Self::GridCell),
            "tab" => Ok(Self::Tab),
            "dialog" => Ok(Self::Dialog),
            "status" => Ok(Self::Status),
            _ => Err(AccessibilityRoleError {
                role: role.to_owned(),
            }),
        }
    }
}

/// Stable actions exposed to assistive technology.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AccessibilityAction {
    Click,
    Focus,
    SetValue,
}

/// Payload delivered when assistive technology invokes an Element action.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccessibilityActionRequest {
    action: AccessibilityAction,
    value: Option<String>,
}

impl AccessibilityActionRequest {
    fn new(action: AccessibilityAction, value: Option<String>) -> Self {
        Self { action, value }
    }

    /// Returns the semantic action requested by the assistive technology.
    pub fn action(&self) -> AccessibilityAction {
        self.action
    }

    /// Returns the requested UTF-8 value for [`AccessibilityAction::SetValue`].
    pub fn value(&self) -> Option<&str> {
        self.value.as_deref()
    }
}

/// Sparse, typed accessibility states. Applying a state again replaces its prior value.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AccessibilityState {
    Disabled(bool),
    Checked(bool),
    Selected(bool),
    Expanded(bool),
    ReadOnly(bool),
    Required(bool),
    Invalid(bool),
}

/// Accessibility metadata carried by a declarative Element description.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Accessibility {
    role: Option<Result<AccessibilityRole, AccessibilityRoleError>>,
    name: Option<String>,
    value: Option<String>,
    actions: Option<Vec<AccessibilityAction>>,
    disabled: Option<bool>,
    checked: Option<bool>,
    selected: Option<bool>,
    expanded: Option<bool>,
    read_only: Option<bool>,
    required: Option<bool>,
    invalid: Option<bool>,
    row_count: Option<usize>,
    column_count: Option<usize>,
    row_index: Option<usize>,
    column_index: Option<usize>,
}

impl Accessibility {
    /// Creates empty accessibility metadata; all native widget defaults are preserved.
    pub fn new() -> Self {
        Self::default()
    }
    /// Sets a typed role or parses a backward-compatible string role.
    /// Unknown strings are retained and reported as [`ElementError::AccessibilityRole`] at validation.
    pub fn role(mut self, role: impl AsRef<str>) -> Self {
        self.role = Some(role.as_ref().parse());
        self
    }
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }
    pub fn state(mut self, state: AccessibilityState) -> Self {
        match state {
            AccessibilityState::Disabled(value) => self.disabled = Some(value),
            AccessibilityState::Checked(value) => self.checked = Some(value),
            AccessibilityState::Selected(value) => self.selected = Some(value),
            AccessibilityState::Expanded(value) => self.expanded = Some(value),
            AccessibilityState::ReadOnly(value) => self.read_only = Some(value),
            AccessibilityState::Required(value) => self.required = Some(value),
            AccessibilityState::Invalid(value) => self.invalid = Some(value),
        }
        self
    }
    pub fn action(mut self, action: AccessibilityAction) -> Self {
        let actions = self.actions.get_or_insert_with(Vec::new);
        if !actions.contains(&action) {
            actions.push(action);
        }
        self
    }
    pub fn logical_grid(mut self, row_count: usize, column_count: usize) -> Self {
        self.row_count = Some(row_count);
        self.column_count = Some(column_count);
        self
    }
    pub fn row_count(mut self, row_count: usize) -> Self {
        self.row_count = Some(row_count);
        self
    }
    pub fn column_count(mut self, column_count: usize) -> Self {
        self.column_count = Some(column_count);
        self
    }
    pub fn mounted_position(mut self, row_index: usize, column_index: usize) -> Self {
        self.row_index = Some(row_index);
        self.column_index = Some(column_index);
        self
    }
    pub fn row_index(mut self, row_index: usize) -> Self {
        self.row_index = Some(row_index);
        self
    }
    pub fn column_index(mut self, column_index: usize) -> Self {
        self.column_index = Some(column_index);
        self
    }
    pub fn role_name(&self) -> Option<&str> {
        match self.role.as_ref()? {
            Ok(role) => Some(role.as_str()),
            Err(error) => Some(error.role()),
        }
    }
    pub fn typed_role(&self) -> Result<Option<AccessibilityRole>, AccessibilityRoleError> {
        self.role.as_ref().map(|role| role.clone()).transpose()
    }
    pub fn name_value(&self) -> Option<&str> {
        self.name.as_deref()
    }
    pub fn value_text(&self) -> Option<&str> {
        self.value.as_deref()
    }
    /// Returns the declared disabled state, when present.
    pub fn disabled_state(&self) -> Option<bool> {
        self.disabled
    }
    /// Returns the declared checked state, when present.
    pub fn checked_state(&self) -> Option<bool> {
        self.checked
    }
    /// Returns the declared selected state, when present.
    pub fn selected_state(&self) -> Option<bool> {
        self.selected
    }
    /// Returns the declared expanded state, when present.
    pub fn expanded_state(&self) -> Option<bool> {
        self.expanded
    }
    /// Returns the declared read-only state, when present.
    pub fn read_only_state(&self) -> Option<bool> {
        self.read_only
    }
    /// Returns the declared required state, when present.
    pub fn required_state(&self) -> Option<bool> {
        self.required
    }
    /// Returns the declared invalid state, when present.
    pub fn invalid_state(&self) -> Option<bool> {
        self.invalid
    }
    pub fn declared_actions(&self) -> Option<&[AccessibilityAction]> {
        self.actions.as_deref()
    }
    pub fn logical_row_count(&self) -> Option<usize> {
        self.row_count
    }
    pub fn logical_column_count(&self) -> Option<usize> {
        self.column_count
    }
    pub fn mounted_row_index(&self) -> Option<usize> {
        self.row_index
    }
    pub fn mounted_column_index(&self) -> Option<usize> {
        self.column_index
    }
}

/// Built-in shape understood by the initial retained Engine adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ElementKind {
    /// A leaf with textual content.
    Text,
    /// A flex container for child Elements.
    Container,
    /// An extension-defined shape. The current adapter retains it as a container.
    Custom(String),
}

/// Stable application-level direction for a composed container.
///
/// This is deliberately a descriptor rather than an Engine layout type. The
/// owning App applies it when the Element is mounted and preserves the same
/// descriptor during identity-preserving updates.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ContainerLayout {
    /// Children are arranged horizontally.
    Row,
    /// Children are arranged vertically.
    #[default]
    Column,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InputMode {
    Controlled,
    Uncontrolled,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct LegacyFlexConfig {
    row: bool,
    compact_row: bool,
    gap: f32,
    padding: [f32; 4],
}

#[derive(Clone, Debug)]
enum TypedInputAction {
    Text(Action<String>),
    Bool(Action<bool>),
    Number(Action<f64>),
    Choice(Action<ChoiceId>),
}

impl TypedInputAction {
    fn invoke(&self, context: &AppContext, value: InputValue) -> Result<(), InputConfigError> {
        match (self, value) {
            (Self::Text(action), InputValue::Text(value)) => action.invoke(context, value)?,
            (Self::Bool(action), InputValue::Bool(value)) => action.invoke(context, value)?,
            (Self::Number(action), InputValue::Number(value)) => action.invoke(context, value)?,
            (Self::Choice(action), InputValue::Choice(value)) => action.invoke(context, value)?,
            (action, value) => {
                let kind = match action {
                    Self::Text(_) => InputKind::Text,
                    Self::Bool(_) => InputKind::Bool,
                    Self::Number(_) => InputKind::Number,
                    Self::Choice(_) => InputKind::Choice,
                };
                return Err(InputConfigError::UnsupportedValue { kind, value });
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct TypedInputSpec {
    kind: InputKind,
    value: InputValue,
    mode: InputMode,
    choices: Vec<String>,
    range: Option<(f64, f64)>,
    on_change: Option<TypedInputAction>,
}

impl TypedInputSpec {
    fn validate(&self) -> Result<(), InputConfigError> {
        match (self.kind, &self.value) {
            (InputKind::Text, InputValue::Text(_)) | (InputKind::Bool, InputValue::Bool(_)) => {}
            (InputKind::Number, InputValue::Number(value)) => {
                let (min, max) = self.range.unwrap_or((0.0, 100.0));
                if !min.is_finite()
                    || !max.is_finite()
                    || min > max
                    || min < f32::MIN as f64
                    || max > f32::MAX as f64
                    || !value.is_finite()
                    || *value < min
                    || *value > max
                {
                    return Err(InputConfigError::OutOfRange {
                        value: *value,
                        min,
                        max,
                    });
                }
            }
            (InputKind::Choice, InputValue::Choice(choice)) => {
                if choice.index() >= self.choices.len() {
                    return Err(InputConfigError::InvalidChoice { choice: *choice });
                }
            }
            (kind, value) => {
                return Err(InputConfigError::UnsupportedValue {
                    kind,
                    value: value.clone(),
                });
            }
        }
        Ok(())
    }
}

/// A serializable-by-construction Element description consumed by the private Engine adapter.
///
/// It contains declarative attributes, child nodes, accessibility metadata, and
/// event positions but never exposes legacy View keys, binding handles, or memo state.
#[derive(Clone, Debug)]
enum CallbackHolder {
    Closure(ButtonCallback),
    Borrowed(Action<()>),
}

#[derive(Clone, Debug)]
pub struct ElementSpec {
    kind: ElementKind,
    text: Option<String>,
    key: Option<BusinessKey>,
    identity: Option<IdentityPath>,
    layout: Option<ContainerLayout>,
    properties: BTreeMap<String, String>,
    children: Vec<Node>,
    accessibility: Accessibility,
    event_slots: Vec<EventSlot>,
    // Cloneable private holder for either a Rust closure that becomes a generated
    // Action at mount/update or an existing Action borrowed from the application.
    press_callback: Option<CallbackHolder>,
    // External Elements may bind declared assistive-technology actions without
    // depending on AccessKit or retained Engine types.
    accessibility_handler: Option<Action<AccessibilityActionRequest>>,
    // Input values and actions remain typed all the way to the native adapter.
    input: Option<Result<TypedInputSpec, InputConfigError>>,
    /// VirtualList row label factory (facade-private).
    item_label: Option<ItemLabelFactory>,
    /// VirtualList primary selection Action.
    list_select: Option<Action<usize>>,
    /// VirtualList activate Action.
    list_activate: Option<Action<usize>>,
    /// VirtualList secondary-click Action `(row, x, y)`.
    list_context: Option<Action<(usize, f64, f64)>>,
    table_cell: Option<TableCellFactory>,
    table_select: Option<Action<SelectionChange>>,
    table_sort: Option<Action<Vec<SortDescriptor>>>,
    table_edit: Option<Action<(usize, usize, String)>>,
    tree_roots: Option<Vec<TreeNode>>,
    tree_select: Option<Action<String>>,
    tree_activate: Option<Action<String>>,
    tree_expand: Option<Action<(String, bool)>>,
    tab_change: Option<Action<usize>>,
    split_ratio: Option<Action<f64>>,
    rich_doc: Option<synui_core::BlockDoc>,
    rich_change: Option<Action<synui_core::BlockDoc>>,
    overlay_dismiss: Option<Action<()>>,
    drawer_change: Option<Action<bool>>,
    menu_bar_menus: Option<Vec<Menu>>,
    context_items: Option<Vec<MenuItem>>,
    context_show_at: Option<(f64, f64)>,
    menu_action: Option<Action<String>>,
}

impl ElementSpec {
    /// Creates a text specification.
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            kind: ElementKind::Text,
            text: Some(text.into()),
            key: None,
            identity: None,
            layout: None,
            properties: BTreeMap::new(),
            children: vec![],
            accessibility: Accessibility::new(),
            event_slots: vec![],
            press_callback: None,
            accessibility_handler: None,
            input: None,
            item_label: None,
            list_select: None,
            list_activate: None,
            list_context: None,
            table_cell: None,
            table_select: None,
            table_sort: None,
            table_edit: None,
            tree_roots: None,
            tree_select: None,
            tree_activate: None,
            tree_expand: None,
            tab_change: None,
            split_ratio: None,
            rich_doc: None,
            rich_change: None,
            overlay_dismiss: None,
            drawer_change: None,
            menu_bar_menus: None,
            context_items: None,
            context_show_at: None,
            menu_action: None,
        }
    }

    /// Creates a container specification from built-in and external child nodes.
    pub fn container(children: impl IntoIterator<Item = Node>) -> Self {
        Self {
            kind: ElementKind::Container,
            text: None,
            key: None,
            identity: None,
            layout: None,
            properties: BTreeMap::new(),
            children: children.into_iter().collect(),
            accessibility: Accessibility::new(),
            event_slots: vec![],
            press_callback: None,
            accessibility_handler: None,
            input: None,
            item_label: None,
            list_select: None,
            list_activate: None,
            list_context: None,
            table_cell: None,
            table_select: None,
            table_sort: None,
            table_edit: None,
            tree_roots: None,
            tree_select: None,
            tree_activate: None,
            tree_expand: None,
            tab_change: None,
            split_ratio: None,
            rich_doc: None,
            rich_change: None,
            overlay_dismiss: None,
            drawer_change: None,
            menu_bar_menus: None,
            context_items: None,
            context_show_at: None,
            menu_action: None,
        }
    }

    /// Creates a container with an explicit stable application-level layout direction.
    pub fn container_with_layout(
        children: impl IntoIterator<Item = Node>,
        layout: ContainerLayout,
    ) -> Self {
        let mut spec = Self::container(children);
        spec.layout = Some(layout);
        spec
    }

    /// Creates a horizontal row container.
    pub fn row(children: impl IntoIterator<Item = Node>) -> Self {
        Self::container_with_layout(children, ContainerLayout::Row)
    }

    /// Creates a vertical column container.
    pub fn column(children: impl IntoIterator<Item = Node>) -> Self {
        Self::container_with_layout(children, ContainerLayout::Column)
    }

    /// Creates an extension-defined specification.
    pub fn custom(kind: impl Into<String>, children: impl IntoIterator<Item = Node>) -> Self {
        let mut spec = Self::container(children);
        spec.kind = ElementKind::Custom(kind.into());
        spec
    }

    /// Attaches an application business key. Identity resolution is performed by the runtime.
    pub fn key(mut self, key: BusinessKey) -> Self {
        self.key = Some(key);
        self
    }

    /// Canonical descriptive alias for [`ElementSpec::key`].
    pub fn business_key(self, key: impl Into<BusinessKey>) -> Self {
        self.key(key.into())
    }
    /// Adds a declarative property.
    pub fn property(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.properties.insert(name.into(), value.into());
        self
    }
    /// Attaches accessibility metadata.
    pub fn accessibility(mut self, accessibility: Accessibility) -> Self {
        self.accessibility = accessibility;
        self
    }
    /// Declares an event position.
    pub fn on(mut self, event: Event, slot: u16) -> Self {
        self.event_slots.push(EventSlot::new(event, slot));
        self
    }
    /// Attaches a cloneable private Rust closure holder for native press routing.
    pub(crate) fn on_click(mut self, callback: ButtonCallback) -> Self {
        self.press_callback = Some(CallbackHolder::Closure(callback));
        self
    }
    /// Attaches an existing App-owned handler to this element's declared press slot.
    ///
    /// External Elements may use this together with [`ElementSpec::on`] to expose
    /// the same identity-scoped event path as built-in Buttons.
    pub fn on_press(mut self, handler: Action<()>) -> Self {
        self.press_callback = Some(CallbackHolder::Borrowed(handler));
        self
    }
    pub(crate) fn with_item_label(mut self, factory: ItemLabelFactory) -> Self {
        self.item_label = Some(factory);
        self
    }
    pub(crate) fn on_list_select(mut self, handler: Action<usize>) -> Self {
        self.list_select = Some(handler);
        self
    }
    pub(crate) fn on_list_activate(mut self, handler: Action<usize>) -> Self {
        self.list_activate = Some(handler);
        self
    }
    pub(crate) fn with_table_cell(mut self, factory: TableCellFactory) -> Self {
        self.table_cell = Some(factory);
        self
    }
    pub(crate) fn on_table_select(mut self, handler: Action<SelectionChange>) -> Self {
        self.table_select = Some(handler);
        self
    }
    pub(crate) fn on_table_sort(mut self, handler: Action<Vec<SortDescriptor>>) -> Self {
        self.table_sort = Some(handler);
        self
    }
    pub(crate) fn on_table_edit(mut self, handler: Action<(usize, usize, String)>) -> Self {
        self.table_edit = Some(handler);
        self
    }
    pub(crate) fn with_tree_roots(mut self, roots: Vec<TreeNode>) -> Self {
        self.tree_roots = Some(roots);
        self
    }
    pub(crate) fn on_tree_select(mut self, handler: Action<String>) -> Self {
        self.tree_select = Some(handler);
        self
    }
    pub(crate) fn on_tree_activate(mut self, handler: Action<String>) -> Self {
        self.tree_activate = Some(handler);
        self
    }
    pub(crate) fn on_tree_expand(mut self, handler: Action<(String, bool)>) -> Self {
        self.tree_expand = Some(handler);
        self
    }
    pub(crate) fn on_tab_change(mut self, handler: Action<usize>) -> Self {
        self.tab_change = Some(handler);
        self
    }
    pub(crate) fn on_split_ratio(mut self, handler: Action<f64>) -> Self {
        self.split_ratio = Some(handler);
        self
    }
    pub(crate) fn with_rich_doc(mut self, document: synui_core::BlockDoc) -> Self {
        self.rich_doc = Some(document);
        self
    }
    pub(crate) fn on_rich_change(mut self, handler: Action<synui_core::BlockDoc>) -> Self {
        self.rich_change = Some(handler);
        self
    }
    pub(crate) fn on_overlay_dismiss(mut self, handler: Action<()>) -> Self {
        self.overlay_dismiss = Some(handler);
        self
    }
    pub(crate) fn on_drawer_change(mut self, handler: Action<bool>) -> Self {
        self.drawer_change = Some(handler);
        self
    }
    pub(crate) fn on_list_context(mut self, handler: Action<(usize, f64, f64)>) -> Self {
        self.list_context = Some(handler);
        self
    }
    pub(crate) fn with_menu_bar(mut self, menus: Vec<Menu>) -> Self {
        self.menu_bar_menus = Some(menus);
        self
    }
    pub(crate) fn with_context_items(mut self, items: Vec<MenuItem>) -> Self {
        self.context_items = Some(items);
        self
    }
    pub(crate) fn with_context_show_at(mut self, x: f64, y: f64) -> Self {
        self.context_show_at = Some((x, y));
        self
    }
    pub(crate) fn on_menu_action(mut self, handler: Action<String>) -> Self {
        self.menu_action = Some(handler);
        self
    }
    /// Binds declared accessibility actions to an App-owned typed handler.
    ///
    /// The handler is borrowed: updating or unmounting the Element removes only
    /// the Engine route and never releases the application-owned Action.
    pub fn on_accessibility_action(mut self, handler: Action<AccessibilityActionRequest>) -> Self {
        self.accessibility_handler = Some(handler);
        self
    }
    pub(crate) fn with_text_input(
        mut self,
        placeholder: String,
        config: &InputConfig<String>,
    ) -> Self {
        self.properties.insert("placeholder".into(), placeholder);
        self.input = Some(match config {
            InputConfig::Controlled { value, on_change } => value
                .get()
                .map(|value| TypedInputSpec {
                    kind: InputKind::Text,
                    value: InputValue::Text(value),
                    mode: InputMode::Controlled,
                    choices: vec![],
                    range: None,
                    on_change: Some(TypedInputAction::Text(on_change.clone())),
                })
                .map_err(InputConfigError::from),
            InputConfig::Uncontrolled { initial, on_change } => Ok(TypedInputSpec {
                kind: InputKind::Text,
                value: InputValue::Text(initial.clone()),
                mode: InputMode::Uncontrolled,
                choices: vec![],
                range: None,
                on_change: on_change.clone().map(TypedInputAction::Text),
            }),
        });
        self
    }
    pub(crate) fn with_bool_input(mut self, config: &InputConfig<bool>) -> Self {
        self.input = Some(match config {
            InputConfig::Controlled { value, on_change } => value
                .get()
                .map(|value| TypedInputSpec {
                    kind: InputKind::Bool,
                    value: InputValue::Bool(value),
                    mode: InputMode::Controlled,
                    choices: vec![],
                    range: None,
                    on_change: Some(TypedInputAction::Bool(on_change.clone())),
                })
                .map_err(InputConfigError::from),
            InputConfig::Uncontrolled { initial, on_change } => Ok(TypedInputSpec {
                kind: InputKind::Bool,
                value: InputValue::Bool(*initial),
                mode: InputMode::Uncontrolled,
                choices: vec![],
                range: None,
                on_change: on_change.clone().map(TypedInputAction::Bool),
            }),
        });
        self
    }
    pub(crate) fn with_number_input(
        mut self,
        config: &InputConfig<f64>,
        min: f64,
        max: f64,
    ) -> Self {
        self.input = Some(match config {
            InputConfig::Controlled { value, on_change } => value
                .get()
                .map(|value| TypedInputSpec {
                    kind: InputKind::Number,
                    value: InputValue::Number(value),
                    mode: InputMode::Controlled,
                    choices: vec![],
                    range: Some((min, max)),
                    on_change: Some(TypedInputAction::Number(on_change.clone())),
                })
                .map_err(InputConfigError::from),
            InputConfig::Uncontrolled { initial, on_change } => Ok(TypedInputSpec {
                kind: InputKind::Number,
                value: InputValue::Number(*initial),
                mode: InputMode::Uncontrolled,
                choices: vec![],
                range: Some((min, max)),
                on_change: on_change.clone().map(TypedInputAction::Number),
            }),
        });
        self
    }
    pub(crate) fn with_choice_input(
        mut self,
        choices: Vec<String>,
        config: &InputConfig<ChoiceId>,
    ) -> Self {
        self.input = Some(match config {
            InputConfig::Controlled { value, on_change } => value
                .get()
                .map(|value| TypedInputSpec {
                    kind: InputKind::Choice,
                    value: InputValue::Choice(value),
                    mode: InputMode::Controlled,
                    choices,
                    range: None,
                    on_change: Some(TypedInputAction::Choice(on_change.clone())),
                })
                .map_err(InputConfigError::from),
            InputConfig::Uncontrolled { initial, on_change } => Ok(TypedInputSpec {
                kind: InputKind::Choice,
                value: InputValue::Choice(*initial),
                mode: InputMode::Uncontrolled,
                choices,
                range: None,
                on_change: on_change.clone().map(TypedInputAction::Choice),
            }),
        });
        self
    }
    fn typed_input(&self) -> Result<Option<&TypedInputSpec>, InputConfigError> {
        match self.input.as_ref() {
            Some(Ok(input)) => {
                input.validate()?;
                Ok(Some(input))
            }
            Some(Err(error)) => Err(error.clone()),
            None => Ok(None),
        }
    }
    /// Returns the built-in or extension-defined shape.
    pub fn kind(&self) -> &ElementKind {
        &self.kind
    }
    /// Returns the optional text content.
    pub fn text_content(&self) -> Option<&str> {
        self.text.as_deref()
    }
    /// Returns the optional business key.
    pub fn key_value(&self) -> Option<&BusinessKey> {
        self.key.as_ref()
    }
    /// Returns the automatic identity after this snapshot has been resolved by an adapter.
    pub fn identity(&self) -> Option<&IdentityPath> {
        self.identity.as_ref()
    }
    /// Returns the stable container layout, if this is a configured container.
    pub fn layout(&self) -> Option<ContainerLayout> {
        self.layout
    }
    /// Returns declarative attributes.
    pub fn properties(&self) -> &BTreeMap<String, String> {
        &self.properties
    }
    /// Returns declarative child nodes.
    pub fn children(&self) -> &[Node] {
        &self.children
    }
    /// Returns declared accessibility metadata.
    pub fn accessibility_metadata(&self) -> &Accessibility {
        &self.accessibility
    }
    /// Returns declared event positions.
    pub fn event_slots(&self) -> &[EventSlot] {
        &self.event_slots
    }
}

/// Failure reported by an Element lifecycle operation.
#[derive(Debug, thiserror::Error)]
pub enum ElementError {
    /// A Text element omitted its required content.
    #[error("text Element specification is missing text content")]
    MissingText,
    /// Two siblings in one repeated collection used the same business key.
    #[error("duplicate business key {key:?} under automatic identity {parent:?}")]
    DuplicateBusinessKey {
        key: BusinessKey,
        parent: IdentityPath,
    },
    /// A lifecycle operation was attempted without a resolved automatic identity.
    #[error("Element automatic identity has not been resolved")]
    IdentityUnavailable,
    /// A lifecycle operation was attempted after its retained widget was removed.
    #[error("Element is no longer mounted")]
    NotMounted,
    /// A closure-backed Element was mounted without an owning App Context.
    #[error("closure event handlers require an owning App Context")]
    MissingAppContext,
    /// App resource registration or strict generated-Action release failed.
    #[error(transparent)]
    App(#[from] crate::app::AppError),
    /// A compatibility string did not map to a supported stable accessibility role.
    #[error(transparent)]
    AccessibilityRole(#[from] AccessibilityRoleError),
    /// A typed input configuration failed validation before native store access.
    #[error(transparent)]
    Input(#[from] InputConfigError),
    /// A private compatibility Element carried a missing or invalid layout property.
    #[error("invalid property {name}={value:?} for compatibility Element kind {kind}")]
    InvalidProperty {
        kind: String,
        name: &'static str,
        value: Option<String>,
    },
    /// The retained widget did not have the native type required by its typed input spec.
    #[error("retained widget is not the expected native {expected}")]
    NativeWidgetType { expected: &'static str },
}

/// Opaque retained state returned when an Element has been mounted.
#[derive(Debug)]
pub struct Mounted {
    widget: Option<WidgetId>,
    children: Vec<Mounted>,
    identity: IdentityPath,
    previous: ElementSpec,
    observed_input: Option<InputValue>,
    observed_list_selection: Option<usize>,
    observed_table_selection: Option<Vec<usize>>,
    observed_table_sort: Option<(usize, bool)>,
    observed_tree_selection: Option<String>,
    observed_tab: Option<usize>,
    observed_rich_doc: Option<synui_core::BlockDoc>,
    observed_drawer_open: Option<bool>,
    observed_overlay_open: Option<bool>,
    observed_context_show: Option<(f64, f64)>,
}

impl Mounted {
    fn new(
        widget: WidgetId,
        children: Vec<Mounted>,
        identity: IdentityPath,
        previous: ElementSpec,
        observed_input: Option<InputValue>,
    ) -> Self {
        let observed_context_show = previous.context_show_at;
        Self {
            widget: Some(widget),
            children,
            identity,
            previous,
            observed_input,
            observed_list_selection: None,
            observed_table_selection: None,
            observed_table_sort: None,
            observed_tree_selection: None,
            observed_tab: None,
            observed_rich_doc: None,
            observed_drawer_open: None,
            observed_overlay_open: None,
            observed_context_show,
        }
    }
    /// Returns whether this lifecycle state still owns a retained widget subtree.
    pub fn is_mounted(&self) -> bool {
        self.widget.is_some()
    }
    /// Returns the stable automatic identity of this mounted Element.
    pub fn identity(&self) -> &IdentityPath {
        &self.identity
    }
}

/// Context passed while an Element creates its pure declarative description.
#[derive(Default)]
pub struct BuildCx {
    _private: (),
}

impl BuildCx {
    /// Starts a description build. Automatic identity is supplied by a later runtime layer.
    pub fn new() -> Self {
        Self::default()
    }
}

/// Context passed while a description is translated into retained Engine widgets.
pub struct MountCx<'a> {
    adapter: &'a mut EngineAdapter,
}

impl<'a> MountCx<'a> {
    /// Mounts a specification and all its built-in or external child nodes.
    pub fn mount_spec(&mut self, spec: &mut ElementSpec) -> Result<Mounted, ElementError> {
        self.adapter.mount_spec(spec)
    }
}

/// Context passed while an identity-preserving Element is updated.
pub struct UpdateCx<'a> {
    adapter: &'a mut EngineAdapter,
    mounted: &'a mut Mounted,
    next: &'a mut ElementSpec,
}

impl<'a> UpdateCx<'a> {
    /// Returns the newly described state for this update.
    pub fn next(&self) -> &ElementSpec {
        self.next
    }
    /// Replaces the retained Engine representation with the newly described state.
    /// Identity-based incremental diffing is layered on this same contract by the runtime.
    pub fn replace_spec(&mut self, _previous: &ElementSpec) -> Result<(), ElementError> {
        self.adapter.replace_spec(self.mounted, self.next)
    }
}

/// Context passed while an Element relinquishes its retained resources.
pub struct UnmountCx<'a> {
    adapter: &'a mut EngineAdapter,
    mounted: &'a mut Mounted,
}

impl<'a> UnmountCx<'a> {
    /// Removes this Element's retained widget subtree.
    pub fn unmount(&mut self) -> Result<(), ElementError> {
        self.adapter.unmount_mounted(self.mounted)
    }
}

/// Open extension contract for built-in and third-party declarative UI components.
///
/// The default lifecycle translates the returned [`ElementSpec`] through the same
/// adapter used by [`Node`] built-ins. Component authors may override a lifecycle
/// method for specialized retained behavior while retaining this stable boundary.
pub trait Element: 'static {
    /// Produces this Element's declarative shape, children, accessibility, and event slots.
    fn describe(&self, cx: &mut BuildCx) -> ElementSpec;

    /// Creates retained state for this Element.
    fn mount(&mut self, cx: &mut MountCx<'_>) -> Result<Mounted, ElementError> {
        let mut spec = self.describe(&mut BuildCx::new());
        cx.mount_spec(&mut spec)
    }

    /// Updates retained state when this Element's automatic identity is unchanged.
    fn update(
        &mut self,
        cx: &mut UpdateCx<'_>,
        previous: &ElementSpec,
    ) -> Result<(), ElementError> {
        cx.replace_spec(previous)
    }

    /// Removes retained state. Event cleanup is coordinated by the runtime before this call.
    fn unmount(&mut self, cx: &mut UnmountCx<'_>) {
        let _ = cx.unmount();
    }

    /// Produces a built-in Node snapshot for composition convenience.
    fn node(&self) -> Node {
        Node::from_spec(self.describe(&mut BuildCx::new()))
    }
}

impl Element for Box<dyn Element> {
    fn describe(&self, cx: &mut BuildCx) -> ElementSpec {
        (**self).describe(cx)
    }
    fn mount(&mut self, cx: &mut MountCx<'_>) -> Result<Mounted, ElementError> {
        (**self).mount(cx)
    }
    fn update(
        &mut self,
        cx: &mut UpdateCx<'_>,
        previous: &ElementSpec,
    ) -> Result<(), ElementError> {
        (**self).update(cx, previous)
    }
    fn unmount(&mut self, cx: &mut UnmountCx<'_>) {
        (**self).unmount(cx)
    }
}

/// A unified declarative tree node. It is either a built-in specification or an
/// owned `Box<dyn Element>` supplied by an external component crate.
pub enum Node {
    Builtin(ElementSpec),
    External {
        element: Rc<RefCell<Box<dyn Element>>>,
        key: Option<BusinessKey>,
    },
}

impl Clone for Node {
    fn clone(&self) -> Self {
        match self {
            Self::Builtin(spec) => Self::Builtin(spec.clone()),
            Self::External { element, key } => Self::External {
                element: Rc::clone(element),
                key: key.clone(),
            },
        }
    }
}

impl std::fmt::Debug for Node {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Builtin(spec) => formatter.debug_tuple("Builtin").field(spec).finish(),
            Self::External { key, .. } => formatter
                .debug_struct("External")
                .field("key", key)
                .finish_non_exhaustive(),
        }
    }
}

impl Node {
    /// Creates a built-in text node.
    pub fn text(text: impl Into<String>) -> Self {
        Self::Builtin(ElementSpec::text(text))
    }
    /// Creates a built-in container node from child nodes.
    pub fn container(children: impl IntoIterator<Item = Node>) -> Self {
        Self::Builtin(ElementSpec::container(children))
    }
    /// Creates a horizontal row node without exposing Engine layout types.
    ///
    /// For padding / gap / background, prefer [`Self::styled`] or
    /// [`crate::widgets::Styled::row`].
    pub fn row(children: impl IntoIterator<Item = Node>) -> Self {
        Self::Builtin(ElementSpec::row(children))
    }
    /// Creates a vertical column node without exposing Engine layout types.
    ///
    /// Layout defaults only (fill width, column flex). For padding / gap /
    /// background / align, use [`Self::styled`] or [`crate::widgets::Styled::column`].
    pub fn column(children: impl IntoIterator<Item = Node>) -> Self {
        Self::Builtin(ElementSpec::column(children))
    }
    /// Wraps this node in a prelude [`crate::widgets::Styled`] host (CSS subset).
    ///
    /// Prefer [`crate::widgets::Styled::column`] / [`crate::widgets::Styled::row`]
    /// when `gap` / `align-items` must apply to the same flex host as the children
    /// (wrapping a [`Self::column`] only styles the outer box around one flex child).
    pub fn styled(self, css: impl Into<String>) -> Self {
        crate::widgets::Styled::new(css, [self]).node()
    }
    /// Attaches hover tip text (shown after a short delay). See book `tooltips.md`.
    pub fn tooltip(mut self, text: impl Into<String>) -> Self {
        match &mut self {
            Self::Builtin(spec) => {
                spec.properties
                    .insert("tooltip".into(), text.into());
            }
            Self::External { .. } => {
                // External elements: wrap is not available here; use Element-level
                // `.tooltip` builders (Button / Icon) or a Builtin wrapper.
            }
        }
        self
    }
    /// Wraps an owned external Element so it can compose with built-in nodes.
    pub fn external(element: impl Element) -> Self {
        Self::External {
            element: Rc::new(RefCell::new(Box::new(element))),
            key: None,
        }
    }
    /// Wraps an external trait object without requiring a closed framework enum variant.
    pub fn boxed(element: Box<dyn Element>) -> Self {
        Self::External {
            element: Rc::new(RefCell::new(element)),
            key: None,
        }
    }
    /// Assigns an explicit business key to this built-in or external node.
    pub fn key(mut self, key: BusinessKey) -> Self {
        match &mut self {
            Self::Builtin(spec) => spec.key = Some(key),
            Self::External { key: node_key, .. } => *node_key = Some(key),
        }
        self
    }
    /// Returns the node shape for diagnostics.
    pub fn kind(&self) -> &str {
        match self {
            Self::Builtin(ElementSpec {
                kind: ElementKind::Text,
                ..
            }) => "text",
            Self::Builtin(ElementSpec {
                kind: ElementKind::Container,
                ..
            }) => "container",
            Self::Builtin(ElementSpec {
                kind: ElementKind::Custom(kind),
                ..
            }) => kind,
            Self::External { .. } => "external",
        }
    }
    /// Returns built-in child nodes; external Elements provide children from `describe`.
    pub fn children(&self) -> &[Node] {
        match self {
            Self::Builtin(spec) => spec.children(),
            Self::External { .. } => &[],
        }
    }
    /// Creates a built-in node from an Element description.
    pub fn from_spec(spec: ElementSpec) -> Self {
        Self::Builtin(spec)
    }
}

impl From<Box<dyn Element>> for Node {
    fn from(element: Box<dyn Element>) -> Self {
        Self::boxed(element)
    }
}

impl Element for Node {
    fn describe(&self, cx: &mut BuildCx) -> ElementSpec {
        match self {
            Self::Builtin(spec) => spec.clone(),
            Self::External { element, key } => {
                let mut spec = element.borrow().describe(cx);
                if let Some(key) = key {
                    spec.key = Some(key.clone());
                }
                spec
            }
        }
    }

    fn mount(&mut self, cx: &mut MountCx<'_>) -> Result<Mounted, ElementError> {
        match self {
            Self::Builtin(spec) => {
                let mut spec = spec.clone();
                cx.mount_spec(&mut spec)
            }
            Self::External { element, .. } => element.borrow_mut().mount(cx),
        }
    }

    fn update(
        &mut self,
        cx: &mut UpdateCx<'_>,
        previous: &ElementSpec,
    ) -> Result<(), ElementError> {
        match self {
            Self::Builtin(_) => cx.replace_spec(previous),
            Self::External { element, .. } => element.borrow_mut().update(cx, previous),
        }
    }

    fn unmount(&mut self, cx: &mut UnmountCx<'_>) {
        match self {
            Self::Builtin(_) => {
                let _ = cx.unmount();
            }
            Self::External { element, .. } => element.borrow_mut().unmount(cx),
        }
    }
}

/// Context passed to facade event handlers.
#[derive(Clone)]
pub struct EventContext {
    app: AppContext,
}

impl EventContext {
    /// Creates an event context for the supplied active application context.
    pub fn new(app: AppContext) -> Self {
        Self { app }
    }
    /// Returns the App Context that owns this event dispatch.
    pub fn app(&self) -> &AppContext {
        &self.app
    }
}

/// App-owned typed event handler.
pub type EventHandler<E> = Action<E>;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ActionRouteKey {
    identity: IdentityPath,
    event: Event,
    slot: u16,
}

#[derive(Clone, Debug)]
enum RoutedAction {
    Generated(Action<()>),
    Borrowed(Action<()>),
}

impl RoutedAction {
    fn action(&self) -> Action<()> {
        match self {
            Self::Generated(action) | Self::Borrowed(action) => action.clone(),
        }
    }
}

type ActionRoutes = Rc<RefCell<HashMap<ActionRouteKey, RoutedAction>>>;

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TeardownStep {
    RoutesDisabled,
    GeneratedActionsReleased,
    StoreRemoved,
}

/// Private bridge from open Element descriptions to retained Engine services.
///
/// The adapter owns the `WidgetStore` tree and marks it for retained layout and
/// render processing. The platform window host consumes that same Engine pathway;
/// explicit legacy View keys, bindings, and memo scheduling remain absent here and
/// are intentionally reserved for a later private compatibility adapter.
pub(crate) struct EngineAdapter {
    store: WidgetStore,
    context: Option<AppContext>,
    _layout: LayoutEngine,
    root: Option<WidgetId>,
    app_seed: u64,
    pending_identity: Option<IdentityPath>,
    action_routes: ActionRoutes,
    #[cfg(test)]
    teardown_steps: Vec<TeardownStep>,
}

impl Default for EngineAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl EngineAdapter {
    /// Creates an adapter backed by the retained WidgetStore, layout, render, and window path.
    pub(crate) fn new() -> Self {
        Self {
            store: WidgetStore::new(),
            context: None,
            _layout: LayoutEngine::new(),
            root: None,
            app_seed: NEXT_APP_SEED.fetch_add(1, Ordering::Relaxed),
            pending_identity: None,
            action_routes: Rc::new(RefCell::new(HashMap::new())),
            #[cfg(test)]
            teardown_steps: Vec::new(),
        }
    }

    /// Creates an adapter whose native event callbacks use `context` as their
    /// only App resource owner.
    pub(crate) fn with_context(context: AppContext) -> Self {
        let mut adapter = Self::new();
        adapter.app_seed = context.id().0;
        adapter.context = Some(context);
        adapter
    }

    /// Temporarily uses the native host's retained store for one mount/update
    /// operation without exposing WidgetStore through the public facade.
    pub(crate) fn with_host_store<R>(
        &mut self,
        store: &mut WidgetStore,
        operation: impl FnOnce(&mut Self) -> R,
    ) -> R {
        std::mem::swap(&mut self.store, store);
        let result = operation(self);
        std::mem::swap(&mut self.store, store);
        result
    }

    /// Performs the complete typed/identity validation pass without touching WidgetStore.
    pub(crate) fn validate_root(&self, root: &mut dyn Element) -> Result<(), ElementError> {
        let mut spec = root.describe(&mut BuildCx::new());
        let identity = IdentityPath::root(self.app_seed, spec.key.as_ref());
        Self::resolve_spec_tree(&mut spec, identity)
    }

    /// Mounts a complete root Element through its lifecycle contract.
    pub fn mount_root(
        &mut self,
        root: &mut dyn Element,
    ) -> Result<(ElementSpec, Mounted), ElementError> {
        let mut spec = root.describe(&mut BuildCx::new());
        let identity = IdentityPath::root(self.app_seed, spec.key.as_ref());
        Self::resolve_spec_tree(&mut spec, identity.clone())?;
        self.pending_identity = Some(identity);
        let mounted = root.mount(&mut MountCx { adapter: self });
        self.pending_identity = None;
        let mut mounted = mounted?;
        mounted.previous = spec.clone();
        if let Some(widget) = mounted.widget {
            self.store.set_root(widget);
            self.root = Some(widget);
        }
        Ok((spec, mounted))
    }

    /// Updates a mounted root through the same lifecycle contract used for external Elements.
    pub fn update_root(
        &mut self,
        root: &mut dyn Element,
        mounted: &mut Mounted,
        previous: &ElementSpec,
    ) -> Result<ElementSpec, ElementError> {
        if !mounted.is_mounted() {
            return Err(ElementError::NotMounted);
        }
        let mut next = root.describe(&mut BuildCx::new());
        let identity = IdentityPath::root(self.app_seed, next.key.as_ref());
        Self::resolve_spec_tree(&mut next, identity.clone())?;

        if identity != mounted.identity {
            let was_root = mounted.widget == self.root;
            self.unmount_mounted(mounted)?;
            self.pending_identity = Some(identity);
            let replacement = root.mount(&mut MountCx { adapter: self });
            self.pending_identity = None;
            *mounted = replacement?;
            if was_root {
                let widget = mounted.widget.ok_or(ElementError::NotMounted)?;
                self.store.set_root(widget);
                self.root = Some(widget);
            }
        } else {
            root.update(
                &mut UpdateCx {
                    adapter: self,
                    mounted,
                    next: &mut next,
                },
                previous,
            )?;
            mounted.previous = next.clone();
        }
        Ok(next)
    }

    /// Unmounts a root through its lifecycle contract.
    pub fn unmount_root(&mut self, root: &mut dyn Element, mounted: &mut Mounted) {
        root.unmount(&mut UnmountCx {
            adapter: self,
            mounted,
        });
    }

    /// Number of retained widgets, useful to Engine integration tests without exposing WidgetStore.
    pub fn retained_widget_count(&self) -> usize {
        self.store.widget_count()
    }

    /// Polls native input values before a frame's revision decision and dispatches typed Actions.
    pub(crate) fn poll_input_changes(&mut self, mounted: &mut Mounted) -> Result<(), ElementError> {
        DropdownMenu::apply_pending_selections(&mut self.store);
        MenuPanel::apply_pending(&mut self.store);
        let mut changes = Vec::new();
        self.collect_input_changes(mounted, &mut changes)?;
        let mut list_events = Vec::new();
        self.collect_list_events(mounted, &mut list_events)?;
        let mut list_contexts = Vec::new();
        let mut context_points = Vec::new();
        self.collect_list_context_events(mounted, &mut list_contexts, &mut context_points)?;
        for (x, y) in context_points {
            let _ = self.show_first_context_menu(mounted, x, y);
        }
        let mut table_selects = Vec::new();
        let mut table_sorts = Vec::new();
        let mut table_edits = Vec::new();
        self.collect_table_events(
            mounted,
            &mut table_selects,
            &mut table_sorts,
            &mut table_edits,
        )?;
        let mut tree_selects = Vec::new();
        let mut tree_activates = Vec::new();
        let mut tree_expands = Vec::new();
        self.collect_tree_events(
            mounted,
            &mut tree_selects,
            &mut tree_activates,
            &mut tree_expands,
        )?;
        let mut tab_changes = Vec::new();
        self.collect_tab_events(mounted, &mut tab_changes)?;
        let mut split_ratios = Vec::new();
        self.collect_split_events(mounted, &mut split_ratios)?;
        let mut rich_changes = Vec::new();
        self.collect_rich_events(mounted, &mut rich_changes)?;
        let mut drawer_changes = Vec::new();
        self.collect_drawer_events(mounted, &mut drawer_changes)?;
        let mut overlay_dismisses = Vec::new();
        self.collect_overlay_events(mounted, &mut overlay_dismisses)?;
        let mut menu_actions = Vec::new();
        self.collect_menu_events(mounted, &mut menu_actions)?;
        if let Some(context) = self.context.as_ref() {
            for (action, value) in changes {
                action.invoke(context, value)?;
            }
            for (action, index) in list_events {
                action.invoke(context, index)?;
            }
            for (action, index, x, y) in list_contexts {
                action.invoke(context, (index, x, y))?;
            }
            for (action, value) in table_selects {
                action.invoke(context, value)?;
            }
            for (action, value) in table_sorts {
                action.invoke(context, value)?;
            }
            for (action, value) in table_edits {
                action.invoke(context, value)?;
            }
            for (action, value) in tree_selects {
                action.invoke(context, value)?;
            }
            for (action, value) in tree_activates {
                action.invoke(context, value)?;
            }
            for (action, value) in tree_expands {
                action.invoke(context, value)?;
            }
            for (action, value) in tab_changes {
                action.invoke(context, value)?;
            }
            for (action, value) in split_ratios {
                action.invoke(context, value)?;
            }
            for (action, value) in rich_changes {
                action.invoke(context, value)?;
            }
            for (action, value) in drawer_changes {
                action.invoke(context, value)?;
            }
            for action in overlay_dismisses {
                action.invoke(context, ())?;
            }
            for (action, value) in menu_actions {
                action.invoke(context, value)?;
            }
        }
        Ok(())
    }

    /// Explicitly overwrites one mounted input; ordinary uncontrolled updates never call this.
    pub(crate) fn set_value(
        &mut self,
        mounted: &mut Mounted,
        value: InputValue,
    ) -> Result<(), ElementError> {
        let input = mounted
            .previous
            .typed_input()?
            .ok_or(ElementError::NativeWidgetType {
                expected: "typed input spec",
            })?;
        let mut candidate = input.clone();
        candidate.value = value.clone();
        candidate.validate()?;
        let widget = mounted.widget.ok_or(ElementError::NotMounted)?;
        self.write_native_input(widget, &mounted.previous, &candidate, &value)?;
        mounted.observed_input = self.read_native_input(widget, &mounted.previous)?;
        self.store.mark_layout_dirty();
        self.store.mark_paint_dirty();
        Ok(())
    }

    fn collect_input_changes(
        &self,
        mounted: &mut Mounted,
        changes: &mut Vec<(TypedInputAction, InputValue)>,
    ) -> Result<(), ElementError> {
        if let Some(widget) = mounted.widget {
            if let Some(current) = self.read_native_input(widget, &mounted.previous)? {
                if mounted.observed_input.as_ref() != Some(&current) {
                    mounted.observed_input = Some(current.clone());
                    if let Some(action) = mounted
                        .previous
                        .typed_input()?
                        .and_then(|input| input.on_change.clone())
                    {
                        changes.push((action, current));
                    }
                }
            }
        }
        for child in &mut mounted.children {
            self.collect_input_changes(child, changes)?;
        }
        Ok(())
    }

    fn collect_list_events(
        &mut self,
        mounted: &mut Mounted,
        events: &mut Vec<(Action<usize>, usize)>,
    ) -> Result<(), ElementError> {
        if let Some(widget) = mounted.widget {
            if matches!(
                &mounted.previous.kind,
                ElementKind::Custom(kind) if kind == "virtual-list"
            ) {
                let (selected, activated) = {
                    let list = self
                        .store
                        .get_mut(widget)
                        .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeListView>())
                        .ok_or(ElementError::NativeWidgetType {
                            expected: "ListView",
                        })?;
                    let selected = list
                        .selected
                        .iter()
                        .copied()
                        .min()
                        .or_else(|| list.selected.iter().copied().next());
                    let activated = list.take_activated();
                    (selected, activated)
                };
                if selected != mounted.observed_list_selection {
                    mounted.observed_list_selection = selected;
                    if let (Some(action), Some(index)) =
                        (mounted.previous.list_select.clone(), selected)
                    {
                        events.push((action, index));
                    }
                }
                if let (Some(action), Some(index)) =
                    (mounted.previous.list_activate.clone(), activated)
                {
                    events.push((action, index));
                }
            }
        }
        for child in &mut mounted.children {
            self.collect_list_events(child, events)?;
        }
        Ok(())
    }

    fn collect_list_context_events(
        &mut self,
        mounted: &mut Mounted,
        events: &mut Vec<(Action<(usize, f64, f64)>, usize, f64, f64)>,
        points: &mut Vec<(f64, f64)>,
    ) -> Result<(), ElementError> {
        if let Some(widget) = mounted.widget {
            if matches!(
                &mounted.previous.kind,
                ElementKind::Custom(kind) if kind == "virtual-list"
            ) {
                let request = {
                    let list = self
                        .store
                        .get_mut(widget)
                        .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeListView>())
                        .ok_or(ElementError::NativeWidgetType {
                            expected: "ListView",
                        })?;
                    list.take_context_request()
                };
                if let Some((index, x, y)) = request {
                    points.push((x, y));
                    if let Some(action) = mounted.previous.list_context.clone() {
                        events.push((action, index, x, y));
                    }
                }
            }
        }
        for child in &mut mounted.children {
            self.collect_list_context_events(child, events, points)?;
        }
        Ok(())
    }

    fn show_first_context_menu(&mut self, mounted: &Mounted, x: f64, y: f64) -> bool {
        if matches!(
            &mounted.previous.kind,
            ElementKind::Custom(kind) if kind == "context-menu"
        ) {
            if let Some(widget) = mounted.widget {
                if let Some(menu) = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeContextMenu>())
                {
                    menu.show_at(Point::new(x, y));
                    return true;
                }
            }
        }
        for child in &mounted.children {
            if self.show_first_context_menu(child, x, y) {
                return true;
            }
        }
        false
    }

    fn collect_menu_events(
        &mut self,
        mounted: &mut Mounted,
        events: &mut Vec<(Action<String>, String)>,
    ) -> Result<(), ElementError> {
        if let Some(widget) = mounted.widget {
            let action_id = match &mounted.previous.kind {
                ElementKind::Custom(kind) if kind == "menu-bar" => self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeMenuBar>())
                    .ok_or(ElementError::NativeWidgetType {
                        expected: "MenuBar",
                    })?
                    .last_action
                    .take(),
                ElementKind::Custom(kind) if kind == "context-menu" => self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeContextMenu>())
                    .ok_or(ElementError::NativeWidgetType {
                        expected: "ContextMenu",
                    })?
                    .last_action
                    .take(),
                _ => None,
            };
            if let (Some(action), Some(id)) = (mounted.previous.menu_action.clone(), action_id) {
                events.push((action, id));
            }
        }
        for child in &mut mounted.children {
            self.collect_menu_events(child, events)?;
        }
        Ok(())
    }

    fn collect_table_events(
        &mut self,
        mounted: &mut Mounted,
        selects: &mut Vec<(Action<SelectionChange>, SelectionChange)>,
        sorts: &mut Vec<(Action<Vec<SortDescriptor>>, Vec<SortDescriptor>)>,
        edits: &mut Vec<(Action<(usize, usize, String)>, (usize, usize, String))>,
    ) -> Result<(), ElementError> {
        if let Some(widget) = mounted.widget {
            if matches!(
                &mounted.previous.kind,
                ElementKind::Custom(kind) if kind == "data-table"
            ) {
                let (selected, sort, edit) = {
                    let table = self
                        .store
                        .get_mut(widget)
                        .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeDataTable>())
                        .ok_or(ElementError::NativeWidgetType {
                            expected: "DataTable",
                        })?;
                    let mut selected: Vec<usize> = table.selected_rows.iter().copied().collect();
                    selected.sort_unstable();
                    let sort = table.sort_column.map(|column| (column, table.sort_asc));
                    let edit = table.take_pending_edit();
                    (selected, sort, edit)
                };
                if mounted.observed_table_selection.as_ref() != Some(&selected) {
                    mounted.observed_table_selection = Some(selected.clone());
                    if let Some(action) = mounted.previous.table_select.clone() {
                        let column_count = mounted
                            .previous
                            .properties
                            .get("columns")
                            .map(|encoded| {
                                encoded.split(';').filter(|part| !part.is_empty()).count()
                            })
                            .unwrap_or(0);
                        let ranges = selected
                            .into_iter()
                            .filter_map(|row| {
                                crate::model::CellRange::new(row, row + 1, 0, column_count.max(1))
                                    .ok()
                            })
                            .collect();
                        selects.push((action, SelectionChange { ranges }));
                    }
                }
                if mounted.observed_table_sort != sort {
                    mounted.observed_table_sort = sort;
                    if let (Some(action), Some((column, asc))) =
                        (mounted.previous.table_sort.clone(), sort)
                    {
                        let descriptor = if asc {
                            SortDescriptor::ascending(column)
                        } else {
                            SortDescriptor::descending(column)
                        };
                        sorts.push((action, vec![descriptor]));
                    }
                }
                if let (Some(action), Some(edit)) = (mounted.previous.table_edit.clone(), edit) {
                    edits.push((action, edit));
                }
            }
        }
        for child in &mut mounted.children {
            self.collect_table_events(child, selects, sorts, edits)?;
        }
        Ok(())
    }

    fn collect_tree_events(
        &mut self,
        mounted: &mut Mounted,
        selects: &mut Vec<(Action<String>, String)>,
        activates: &mut Vec<(Action<String>, String)>,
        expands: &mut Vec<(Action<(String, bool)>, (String, bool))>,
    ) -> Result<(), ElementError> {
        if let Some(widget) = mounted.widget {
            if matches!(
                &mounted.previous.kind,
                ElementKind::Custom(kind) if kind == "virtual-tree"
            ) {
                let (selected, activated, expand) = {
                    let tree = self
                        .store
                        .get_mut(widget)
                        .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeTreeView>())
                        .ok_or(ElementError::NativeWidgetType {
                            expected: "TreeView",
                        })?;
                    (
                        tree.selected.clone(),
                        tree.take_activated(),
                        tree.take_expand_dirty(),
                    )
                };
                if selected != mounted.observed_tree_selection {
                    mounted.observed_tree_selection = selected.clone();
                    if let (Some(action), Some(id)) =
                        (mounted.previous.tree_select.clone(), selected)
                    {
                        selects.push((action, id));
                    }
                }
                if let (Some(action), Some(id)) =
                    (mounted.previous.tree_activate.clone(), activated)
                {
                    activates.push((action, id));
                }
                if let (Some(action), Some(change)) = (mounted.previous.tree_expand.clone(), expand)
                {
                    expands.push((action, change));
                }
            }
        }
        for child in &mut mounted.children {
            self.collect_tree_events(child, selects, activates, expands)?;
        }
        Ok(())
    }

    fn collect_tab_events(
        &mut self,
        mounted: &mut Mounted,
        changes: &mut Vec<(Action<usize>, usize)>,
    ) -> Result<(), ElementError> {
        if let Some(widget) = mounted.widget {
            let selected = match &mounted.previous.kind {
                ElementKind::Custom(kind) if kind == "tab-bar" => Some(
                    self.store
                        .get(widget)
                        .and_then(|widget| widget.as_any().downcast_ref::<NativeTabBar>())
                        .ok_or(ElementError::NativeWidgetType { expected: "TabBar" })?
                        .selected,
                ),
                ElementKind::Custom(kind) if kind == "bottom-navigation" => Some(
                    self.store
                        .get(widget)
                        .and_then(|widget| widget.as_any().downcast_ref::<NativeBottomNavigation>())
                        .ok_or(ElementError::NativeWidgetType {
                            expected: "BottomNavigation",
                        })?
                        .selected,
                ),
                _ => None,
            };
            if let Some(selected) = selected {
                if mounted.observed_tab != Some(selected) {
                    let previous = mounted.observed_tab;
                    mounted.observed_tab = Some(selected);
                    if previous.is_some() {
                        if let Some(action) = mounted.previous.tab_change.clone() {
                            changes.push((action, selected));
                        }
                    }
                }
            }
        }
        for child in &mut mounted.children {
            self.collect_tab_events(child, changes)?;
        }
        Ok(())
    }

    fn collect_split_events(
        &mut self,
        mounted: &mut Mounted,
        changes: &mut Vec<(Action<f64>, f64)>,
    ) -> Result<(), ElementError> {
        if let Some(widget) = mounted.widget {
            if matches!(
                &mounted.previous.kind,
                ElementKind::Custom(kind) if kind == "split-pane"
            ) {
                let (dirty, ratio) = {
                    let split = self
                        .store
                        .get_mut(widget)
                        .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeSplitPane>())
                        .ok_or(ElementError::NativeWidgetType {
                            expected: "SplitPane",
                        })?;
                    (split.take_ratio_dirty(), split.ratio as f64)
                };
                if dirty {
                    if let Some(action) = mounted.previous.split_ratio.clone() {
                        changes.push((action, ratio));
                    }
                }
            }
        }
        for child in &mut mounted.children {
            self.collect_split_events(child, changes)?;
        }
        Ok(())
    }

    fn collect_rich_events(
        &mut self,
        mounted: &mut Mounted,
        changes: &mut Vec<(Action<synui_core::BlockDoc>, synui_core::BlockDoc)>,
    ) -> Result<(), ElementError> {
        if let Some(widget) = mounted.widget {
            if matches!(
                &mounted.previous.kind,
                ElementKind::Custom(kind) if kind == "rich-document"
            ) {
                let doc = {
                    let editor = self
                        .store
                        .get_mut(widget)
                        .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeRichEdit>())
                        .ok_or(ElementError::NativeWidgetType {
                            expected: "RichEdit",
                        })?;
                    editor.commit_pending_edit();
                    editor.doc.clone()
                };
                if mounted.observed_rich_doc.as_ref() != Some(&doc) {
                    let previous = mounted.observed_rich_doc.replace(doc.clone());
                    if previous.is_some() {
                        if let Some(action) = mounted.previous.rich_change.clone() {
                            changes.push((action, doc));
                        }
                    }
                }
            }
        }
        for child in &mut mounted.children {
            self.collect_rich_events(child, changes)?;
        }
        Ok(())
    }

    fn collect_drawer_events(
        &mut self,
        mounted: &mut Mounted,
        changes: &mut Vec<(Action<bool>, bool)>,
    ) -> Result<(), ElementError> {
        if let Some(widget) = mounted.widget {
            if matches!(
                &mounted.previous.kind,
                ElementKind::Custom(kind) if kind == "navigation-drawer"
            ) {
                let open = self
                    .store
                    .get(widget)
                    .and_then(|widget| widget.as_any().downcast_ref::<NativeNavigationDrawer>())
                    .ok_or(ElementError::NativeWidgetType {
                        expected: "NavigationDrawer",
                    })?
                    .open;
                if mounted.observed_drawer_open != Some(open) {
                    let previous = mounted.observed_drawer_open.replace(open);
                    if previous.is_some() {
                        if let Some(action) = mounted.previous.drawer_change.clone() {
                            changes.push((action, open));
                        }
                    }
                }
            }
        }
        for child in &mut mounted.children {
            self.collect_drawer_events(child, changes)?;
        }
        Ok(())
    }

    fn collect_overlay_events(
        &mut self,
        mounted: &mut Mounted,
        dismisses: &mut Vec<Action<()>>,
    ) -> Result<(), ElementError> {
        if let Some(widget) = mounted.widget {
            let open = match &mounted.previous.kind {
                ElementKind::Custom(kind) if kind == "dialog" => Some(
                    self.store
                        .get(widget)
                        .and_then(|widget| widget.as_any().downcast_ref::<NativeDialog>())
                        .ok_or(ElementError::NativeWidgetType { expected: "Dialog" })?
                        .open,
                ),
                ElementKind::Custom(kind) if kind == "bottom-sheet" => Some(
                    self.store
                        .get(widget)
                        .and_then(|widget| widget.as_any().downcast_ref::<NativeBottomSheet>())
                        .ok_or(ElementError::NativeWidgetType {
                            expected: "BottomSheet",
                        })?
                        .open,
                ),
                ElementKind::Custom(kind) if kind == "snack-bar" => Some(
                    self.store
                        .get(widget)
                        .and_then(|widget| widget.as_any().downcast_ref::<NativeSnackBar>())
                        .ok_or(ElementError::NativeWidgetType {
                            expected: "SnackBar",
                        })?
                        .visible,
                ),
                _ => None,
            };
            if let Some(open) = open {
                if mounted.observed_overlay_open != Some(open) {
                    let previous = mounted.observed_overlay_open.replace(open);
                    if previous == Some(true) && !open {
                        if let Some(action) = mounted.previous.overlay_dismiss.clone() {
                            dismisses.push(action);
                        }
                    }
                }
            }
        }
        for child in &mut mounted.children {
            self.collect_overlay_events(child, dismisses)?;
        }
        Ok(())
    }

    fn read_native_input(
        &self,
        widget: WidgetId,
        spec: &ElementSpec,
    ) -> Result<Option<InputValue>, ElementError> {
        let Some(input) = spec.typed_input()? else {
            return Ok(None);
        };
        let value = match input.kind {
            InputKind::Text => {
                let text = match &spec.kind {
                    ElementKind::Custom(kind) if kind == "text-area" => self
                        .store
                        .get(widget)
                        .and_then(|widget| widget.as_any().downcast_ref::<NativeTextArea>())
                        .ok_or(ElementError::NativeWidgetType {
                            expected: "TextArea",
                        })?
                        .edit
                        .text
                        .clone(),
                    _ => self
                        .store
                        .get(widget)
                        .and_then(|widget| widget.as_any().downcast_ref::<NativeTextField>())
                        .ok_or(ElementError::NativeWidgetType {
                            expected: "TextField",
                        })?
                        .edit
                        .text
                        .clone(),
                };
                InputValue::Text(text)
            }
            InputKind::Bool => InputValue::Bool(Self::read_bool_native(&self.store, widget, spec)?),
            InputKind::Number => InputValue::Number(
                self.store
                    .get(widget)
                    .and_then(|widget| widget.as_any().downcast_ref::<NativeSlider>())
                    .ok_or(ElementError::NativeWidgetType { expected: "Slider" })?
                    .value as f64,
            ),
            InputKind::Choice => InputValue::Choice(ChoiceId::new(
                self.store
                    .get(widget)
                    .and_then(|widget| widget.as_any().downcast_ref::<NativeDropdown>())
                    .ok_or(ElementError::NativeWidgetType {
                        expected: "Dropdown",
                    })?
                    .selected,
            )),
        };
        let mut candidate = input.clone();
        candidate.value = value.clone();
        if let Some((min, max)) = candidate.range {
            candidate.range = Some((min as f32 as f64, max as f32 as f64));
        }
        candidate.validate()?;
        Ok(Some(value))
    }

    fn read_bool_native(
        store: &WidgetStore,
        widget: WidgetId,
        spec: &ElementSpec,
    ) -> Result<bool, ElementError> {
        match &spec.kind {
            ElementKind::Custom(kind) if kind == "switch" => Ok(store
                .get(widget)
                .and_then(|widget| widget.as_any().downcast_ref::<NativeSwitch>())
                .ok_or(ElementError::NativeWidgetType { expected: "Switch" })?
                .on),
            ElementKind::Custom(kind) if kind == "radio" => Ok(store
                .get(widget)
                .and_then(|widget| widget.as_any().downcast_ref::<NativeRadio>())
                .ok_or(ElementError::NativeWidgetType { expected: "Radio" })?
                .selected),
            _ => Ok(store
                .get(widget)
                .and_then(|widget| widget.as_any().downcast_ref::<NativeCheckbox>())
                .ok_or(ElementError::NativeWidgetType {
                    expected: "Checkbox",
                })?
                .checked),
        }
    }

    fn write_bool_native(
        store: &mut WidgetStore,
        widget: WidgetId,
        spec: &ElementSpec,
        value: bool,
    ) -> Result<(), ElementError> {
        match &spec.kind {
            ElementKind::Custom(kind) if kind == "switch" => {
                store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeSwitch>())
                    .ok_or(ElementError::NativeWidgetType { expected: "Switch" })?
                    .on = value;
            }
            ElementKind::Custom(kind) if kind == "radio" => {
                store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeRadio>())
                    .ok_or(ElementError::NativeWidgetType { expected: "Radio" })?
                    .selected = value;
            }
            _ => {
                store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeCheckbox>())
                    .ok_or(ElementError::NativeWidgetType {
                        expected: "Checkbox",
                    })?
                    .checked = value;
            }
        }
        Ok(())
    }

    fn write_native_input(
        &mut self,
        widget: WidgetId,
        spec: &ElementSpec,
        input: &TypedInputSpec,
        value: &InputValue,
    ) -> Result<(), ElementError> {
        match (input.kind, value) {
            (InputKind::Text, InputValue::Text(value)) => match &spec.kind {
                ElementKind::Custom(kind) if kind == "text-area" => {
                    let area = self
                        .store
                        .get_mut(widget)
                        .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeTextArea>())
                        .ok_or(ElementError::NativeWidgetType {
                            expected: "TextArea",
                        })?;
                    area.edit.text = value.clone();
                    area.notify_text_changed();
                }
                _ => {
                    let field = self
                        .store
                        .get_mut(widget)
                        .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeTextField>())
                        .ok_or(ElementError::NativeWidgetType {
                            expected: "TextField",
                        })?;
                    field.edit.text = value.clone();
                    field.notify_text_changed();
                }
            },
            (InputKind::Bool, InputValue::Bool(value)) => {
                Self::write_bool_native(&mut self.store, widget, spec, *value)?;
            }
            (InputKind::Number, InputValue::Number(value)) => {
                self.store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeSlider>())
                    .ok_or(ElementError::NativeWidgetType { expected: "Slider" })?
                    .value = *value as f32;
            }
            (InputKind::Choice, InputValue::Choice(value)) => {
                let dropdown = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeDropdown>())
                    .ok_or(ElementError::NativeWidgetType {
                        expected: "Dropdown",
                    })?;
                dropdown.selected = value.index();
                dropdown.highlighted = value.index();
            }
            (kind, value) => {
                return Err(InputConfigError::UnsupportedValue {
                    kind,
                    value: value.clone(),
                }
                .into());
            }
        }
        Ok(())
    }

    fn resolve_spec_tree(
        spec: &mut ElementSpec,
        identity: IdentityPath,
    ) -> Result<(), ElementError> {
        Self::validate_spec(spec)?;
        spec.identity = Some(identity.clone());
        let mut keys = HashSet::new();
        for (slot, child) in spec.children.iter().enumerate() {
            let mut child_spec = child.describe(&mut BuildCx::new());
            if let Some(key) = child_spec.key.as_ref() {
                if !keys.insert(key.clone()) {
                    return Err(ElementError::DuplicateBusinessKey {
                        key: key.clone(),
                        parent: identity,
                    });
                }
            }
            let child_identity = identity.child(slot, child_spec.key.as_ref());
            Self::resolve_spec_tree(&mut child_spec, child_identity)?;
        }
        Ok(())
    }

    fn validate_spec(spec: &ElementSpec) -> Result<(), ElementError> {
        spec.accessibility.typed_role()?;
        // Compatibility layout properties are parsed before child mount so a
        // malformed private snapshot cannot retain a partial subtree.
        Self::legacy_flex_config(spec)?;
        let Some(input) = spec.typed_input()? else {
            return Ok(());
        };
        let expected_ok = match input.kind {
            InputKind::Text => matches!(
                &spec.kind,
                ElementKind::Custom(kind) if matches!(kind.as_str(), "text-input" | "text-area")
            ),
            InputKind::Bool => matches!(
                &spec.kind,
                ElementKind::Custom(kind) if matches!(kind.as_str(), "checkbox" | "switch" | "radio")
            ),
            InputKind::Number => {
                matches!(&spec.kind, ElementKind::Custom(kind) if kind == "number-input")
            }
            InputKind::Choice => {
                matches!(&spec.kind, ElementKind::Custom(kind) if kind == "select")
            }
        };
        if !expected_ok {
            return Err(InputConfigError::UnsupportedValue {
                kind: input.kind,
                value: input.value.clone(),
            }
            .into());
        }
        Ok(())
    }

    fn password_prop(spec: &ElementSpec) -> bool {
        spec.properties
            .get("password")
            .map(|value| value == "true")
            .unwrap_or(false)
    }

    fn mount_spec(&mut self, spec: &mut ElementSpec) -> Result<Mounted, ElementError> {
        let identity = self
            .pending_identity
            .take()
            .or_else(|| spec.identity.clone())
            .unwrap_or_else(|| IdentityPath::root(self.app_seed, spec.key.as_ref()));
        Self::resolve_spec_tree(spec, identity.clone())?;
        self.validate_accessibility_spec(spec)?;

        let mut children = Vec::with_capacity(spec.children.len());
        for (slot, child) in spec.children.iter_mut().enumerate() {
            let mut child_spec = child.describe(&mut BuildCx::new());
            let child_identity = identity.child(slot, child_spec.key.as_ref());
            Self::resolve_spec_tree(&mut child_spec, child_identity.clone())?;
            self.pending_identity = Some(child_identity);
            match child.mount(&mut MountCx { adapter: self }) {
                Ok(mut mounted_child) => {
                    mounted_child.previous = child_spec;
                    children.push(mounted_child);
                }
                Err(error) => {
                    self.pending_identity = None;
                    for child in &mut children {
                        let _ = self.unmount_mounted(child);
                    }
                    return Err(error);
                }
            }
        }
        self.pending_identity = None;

        let child_ids = children
            .iter()
            .filter_map(|child| child.widget)
            .collect::<Vec<_>>();
        let widget = self.insert_widget(spec, &identity)?;
        if let Err(error) = self.sync_actions(spec, &identity) {
            self.store.remove(widget);
            for child in &mut children {
                let _ = self.unmount_mounted(child);
            }
            return Err(error);
        }
        if let Err(error) = self.sync_accessibility(widget, spec, &identity) {
            self.store.remove(widget);
            for child in &mut children {
                let _ = self.unmount_mounted(child);
            }
            return Err(error);
        }
        self.store.set_children(widget, child_ids.clone());
        self.sync_host_children(widget, &child_ids, spec)?;
        let observed_input = self.read_native_input(widget, spec)?;
        self.store.mark_layout_dirty();
        self.store.mark_paint_dirty();
        Ok(Mounted::new(
            widget,
            children,
            identity,
            spec.clone(),
            observed_input,
        ))
    }

    fn reconcile_spec(
        &mut self,
        mounted: &mut Mounted,
        next: &mut ElementSpec,
    ) -> Result<(), ElementError> {
        let identity = next
            .identity
            .clone()
            .ok_or(ElementError::IdentityUnavailable)?;
        if identity != mounted.identity {
            return self.replace_spec(mounted, next);
        }

        let previous = mounted.previous.clone();
        self.validate_accessibility_spec(next)?;
        self.sync_actions(next, &identity)?;
        self.reconcile_children(mounted, next)?;
        self.update_widget(mounted, next, &identity)?;
        mounted.previous = next.clone();
        Self::mark_reconcile_dirty(&mut self.store, mounted.widget, &previous, next);
        Ok(())
    }

    /// Prefer scoped paint/measure dirtiness over full-tree layout on every State rebuild.
    fn mark_reconcile_dirty(
        store: &mut WidgetStore,
        widget: Option<WidgetId>,
        previous: &ElementSpec,
        next: &ElementSpec,
    ) {
        // Status/Label copy lives in `text` or property "text"/"message" — not a
        // structural layout change for SplitPane/DataTable siblings.
        let content_text_changed = previous.text != next.text
            || previous.properties.get("text") != next.properties.get("text")
            || previous.properties.get("message") != next.properties.get("message");
        // Split ratio / tab selected: sync_split_panes + widget paint, not taffy.
        let chrome_props_changed = previous.properties.get("ratio") != next.properties.get("ratio")
            || previous.properties.get("selected") != next.properties.get("selected");
        let input_value_changed = match (previous.typed_input(), next.typed_input()) {
            (Ok(Some(prev)), Ok(Some(next_input))) => prev.value != next_input.value,
            _ => false,
        };
        let layout_props_changed = previous.kind != next.kind
            || previous.layout != next.layout
            || previous.children.len() != next.children.len()
            || Self::layout_relevant_properties(previous) != Self::layout_relevant_properties(next);
        match (
            layout_props_changed,
            content_text_changed || chrome_props_changed || input_value_changed,
            widget,
        ) {
            (true, _, Some(id)) => store.mark_layout_dirty_widget(id),
            (true, _, None) => store.mark_layout_dirty(),
            // Hugging `Node::text`: measured width tracks glyphs. Paint-only left
            // stale bounds so a wider digit (or "9"→"10") spilled past the damage erase.
            (false, true, Some(id)) if previous.text != next.text => {
                store.mark_layout_dirty_widget(id);
            }
            // Facade `Label` property("text"): length change needs hug remeasure (ghost).
            // Same-length Volume ticks stay paint-only so slider drag does not Taffy every frame.
            (false, true, Some(id))
                if matches!(&previous.kind, ElementKind::Custom(kind) if kind == "label")
                    && matches!(&next.kind, ElementKind::Custom(kind) if kind == "label")
                    && Self::facade_label_text_len(previous)
                        != Self::facade_label_text_len(next) =>
            {
                store.mark_layout_dirty_widget(id);
            }
            // Status/detail copy, same-length facade Label, split/tab chrome, or controlled input.
            (false, true, Some(id)) => store.bump_paint_epoch_widget(id),
            (false, true, None) => store.mark_paint_dirty(),
            // Unchanged nodes must not be dirtied — repainting DataTable/SplitPane on
            // every selection looks like Detail jumping up over the table bottom.
            (false, false, _) => {}
        }
    }

    fn facade_label_text_len(spec: &ElementSpec) -> usize {
        spec.properties.get("text").map(|s| s.len()).unwrap_or(0)
    }

    fn layout_relevant_properties(spec: &ElementSpec) -> BTreeMap<String, String> {
        spec.properties
            .iter()
            .filter(|(key, _)| {
                !matches!(
                    key.as_str(),
                    // Paint-only / sync_split chrome — must not trigger taffy, or the
                    // secondary pane is drawn at flex mid-height for a frame.
                    "text"
                        | "message"
                        | "truncate"
                        | "max_lines"
                        | "name"
                        | "label"
                        | "ratio"
                        | "selected"
                        | "playing"
                        | "speed"
                        | "json"
                        | "id"
                )
            })
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect()
    }

    fn reconcile_children(
        &mut self,
        mounted: &mut Mounted,
        next: &mut ElementSpec,
    ) -> Result<(), ElementError> {
        let parent_identity = mounted.identity.clone();
        let mut previous = std::mem::take(&mut mounted.children)
            .into_iter()
            .map(|child| (child.identity.clone(), child))
            .collect::<HashMap<_, _>>();
        let mut children = Vec::with_capacity(next.children.len());

        for (slot, child) in next.children.iter_mut().enumerate() {
            let mut child_spec = child.describe(&mut BuildCx::new());
            let child_identity = parent_identity.child(slot, child_spec.key.as_ref());
            Self::resolve_spec_tree(&mut child_spec, child_identity.clone())?;
            if let Some(mut retained) = previous.remove(&child_identity) {
                let old_spec = retained.previous.clone();
                child.update(
                    &mut UpdateCx {
                        adapter: self,
                        mounted: &mut retained,
                        next: &mut child_spec,
                    },
                    &old_spec,
                )?;
                retained.previous = child_spec;
                children.push(retained);
            } else {
                self.pending_identity = Some(child_identity);
                let mounted_child = child.mount(&mut MountCx { adapter: self });
                self.pending_identity = None;
                children.push(mounted_child?);
            }
        }

        for (_, mut removed) in previous {
            self.unmount_mounted(&mut removed)?;
        }
        let child_ids = children
            .iter()
            .filter_map(|child| child.widget)
            .collect::<Vec<_>>();
        let widget = mounted.widget.ok_or(ElementError::NotMounted)?;
        self.store.set_children(widget, child_ids.clone());
        self.sync_host_children(widget, &child_ids, next)?;
        mounted.children = children;
        Ok(())
    }

    fn is_legacy_flex_kind(kind: &str) -> bool {
        matches!(kind, "legacy-flex-row" | "legacy-flex-column")
    }

    fn invalid_legacy_property(spec: &ElementSpec, name: &'static str) -> ElementError {
        ElementError::InvalidProperty {
            kind: match &spec.kind {
                ElementKind::Custom(kind) => kind.clone(),
                other => format!("{other:?}"),
            },
            name,
            value: spec.properties.get(name).cloned(),
        }
    }

    fn legacy_flex_config(spec: &ElementSpec) -> Result<Option<LegacyFlexConfig>, ElementError> {
        let ElementKind::Custom(kind) = &spec.kind else {
            return Ok(None);
        };
        if !Self::is_legacy_flex_kind(kind) {
            return Ok(None);
        }

        let source = spec
            .properties
            .get("legacy-source")
            .map(String::as_str)
            .ok_or_else(|| Self::invalid_legacy_property(spec, "legacy-source"))?;
        let (row, compact_row) = match (kind.as_str(), source) {
            ("legacy-flex-row", "flex") => (true, false),
            ("legacy-flex-row", "row") => (true, true),
            ("legacy-flex-column", "flex" | "column") => (false, false),
            _ => {
                return Err(Self::invalid_legacy_property(spec, "legacy-source"));
            }
        };

        let parse_non_negative = |name: &'static str| -> Result<f32, ElementError> {
            let raw = spec
                .properties
                .get(name)
                .ok_or_else(|| Self::invalid_legacy_property(spec, name))?;
            let value = raw
                .parse::<f32>()
                .map_err(|_| Self::invalid_legacy_property(spec, name))?;
            if !value.is_finite() || value < 0.0 {
                return Err(Self::invalid_legacy_property(spec, name));
            }
            Ok(value)
        };

        Ok(Some(LegacyFlexConfig {
            row,
            compact_row,
            gap: parse_non_negative("legacy-gap")?,
            padding: [
                parse_non_negative("legacy-padding-top")?,
                parse_non_negative("legacy-padding-right")?,
                parse_non_negative("legacy-padding-bottom")?,
                parse_non_negative("legacy-padding-left")?,
            ],
        }))
    }

    fn make_legacy_flex(spec: &ElementSpec) -> Result<Flex, ElementError> {
        let config = Self::legacy_flex_config(spec)?
            .ok_or_else(|| Self::invalid_legacy_property(spec, "legacy-source"))?;
        let mut flex = Flex::new()
            .with_justify(JustifyContent::FlexStart)
            .with_width(Dimension::Fill)
            .with_gap(config.gap)
            .with_padding(config.padding);
        if config.compact_row {
            flex = flex
                .with_align(AlignItems::Center)
                .with_height(Dimension::Auto);
        } else {
            flex = flex
                .with_align(AlignItems::Stretch)
                .with_height(Dimension::Fill);
        }
        Ok(if config.row {
            flex.row()
        } else {
            flex.column()
        })
    }

    fn make_container(spec: &ElementSpec) -> Flex {
        // Center on the cross axis so Auto-width labels stay visually centered
        // (Stretch was making chrome labels look left-aligned).
        // Rows must be content-height: Fill made toolbars (Export MD / Load sample)
        // claim half the column and leave huge gaps around the buttons.
        let flex = Flex::new()
            .with_justify(JustifyContent::FlexStart)
            .with_align(AlignItems::Center)
            .with_width(Dimension::Fill);
        match spec.layout {
            Some(ContainerLayout::Row) => flex.with_height(Dimension::Auto).row(),
            Some(ContainerLayout::Column) | None => flex.with_height(Dimension::Fill).column(),
        }
    }

    fn insert_widget(
        &mut self,
        spec: &ElementSpec,
        identity: &IdentityPath,
    ) -> Result<WidgetId, ElementError> {
        let id = self.insert_widget_impl(spec, identity)?;
        Self::sync_tooltip_prop(&mut self.store, id, spec);
        Ok(id)
    }

    fn sync_tooltip_prop(store: &mut synui_core::WidgetStore, id: WidgetId, spec: &ElementSpec) {
        match spec.properties.get("tooltip") {
            Some(text) if !text.is_empty() => store.set_widget_tooltip(id, text.clone()),
            _ => store.clear_widget_tooltip(id),
        }
    }

    fn insert_widget_impl(
        &mut self,
        spec: &ElementSpec,
        identity: &IdentityPath,
    ) -> Result<WidgetId, ElementError> {
        Self::validate_spec(spec)?;
        match &spec.kind {
            ElementKind::Text => Ok(self.store.insert(Label::new(
                spec.text.as_deref().ok_or(ElementError::MissingText)?,
            ))),
            ElementKind::Custom(kind) if kind == "button" => {
                Ok(self.store.insert(self.make_button(spec, identity)))
            }
            ElementKind::Custom(kind) if kind == "text-input" => {
                let input =
                    spec.typed_input()?
                        .ok_or_else(|| InputConfigError::UnsupportedValue {
                            kind: InputKind::Text,
                            value: InputValue::Text(String::new()),
                        })?;
                let InputValue::Text(value) = &input.value else {
                    return Err(InputConfigError::UnsupportedValue {
                        kind: InputKind::Text,
                        value: input.value.clone(),
                    }
                    .into());
                };
                let mut field = NativeTextField::new(
                    spec.properties
                        .get("placeholder")
                        .cloned()
                        .unwrap_or_default(),
                );
                field.password = Self::password_prop(spec);
                field.edit.text = value.clone();
                field.notify_text_changed();
                Ok(self.store.insert(field))
            }
            ElementKind::Custom(kind) if kind == "text-area" => {
                let input =
                    spec.typed_input()?
                        .ok_or_else(|| InputConfigError::UnsupportedValue {
                            kind: InputKind::Text,
                            value: InputValue::Text(String::new()),
                        })?;
                let InputValue::Text(value) = &input.value else {
                    return Err(InputConfigError::UnsupportedValue {
                        kind: InputKind::Text,
                        value: input.value.clone(),
                    }
                    .into());
                };
                let mut area = NativeTextArea::new();
                area.edit.text = value.clone();
                area.notify_text_changed();
                Self::apply_text_area_chrome(&mut area, spec);
                Ok(self.store.insert(area))
            }
            ElementKind::Custom(kind) if kind == "checkbox" => {
                let input =
                    spec.typed_input()?
                        .ok_or_else(|| InputConfigError::UnsupportedValue {
                            kind: InputKind::Bool,
                            value: InputValue::Bool(false),
                        })?;
                let InputValue::Bool(value) = input.value else {
                    return Err(InputConfigError::UnsupportedValue {
                        kind: InputKind::Bool,
                        value: input.value.clone(),
                    }
                    .into());
                };
                let label = spec.properties.get("label").cloned().unwrap_or_default();
                let mut checkbox = NativeCheckbox::new(label);
                checkbox.checked = value;
                Ok(self.store.insert(checkbox))
            }
            ElementKind::Custom(kind) if kind == "switch" => {
                let input =
                    spec.typed_input()?
                        .ok_or_else(|| InputConfigError::UnsupportedValue {
                            kind: InputKind::Bool,
                            value: InputValue::Bool(false),
                        })?;
                let InputValue::Bool(value) = input.value else {
                    return Err(InputConfigError::UnsupportedValue {
                        kind: InputKind::Bool,
                        value: input.value.clone(),
                    }
                    .into());
                };
                let mut switch = NativeSwitch::new();
                switch.on = value;
                Ok(self.store.insert(switch))
            }
            ElementKind::Custom(kind) if kind == "radio" => {
                let input =
                    spec.typed_input()?
                        .ok_or_else(|| InputConfigError::UnsupportedValue {
                            kind: InputKind::Bool,
                            value: InputValue::Bool(false),
                        })?;
                let InputValue::Bool(value) = input.value else {
                    return Err(InputConfigError::UnsupportedValue {
                        kind: InputKind::Bool,
                        value: input.value.clone(),
                    }
                    .into());
                };
                let label = spec.properties.get("label").cloned().unwrap_or_default();
                let mut radio = NativeRadio::new(label);
                radio.selected = value;
                Ok(self.store.insert(radio))
            }
            ElementKind::Custom(kind) if kind == "number-input" => {
                let input =
                    spec.typed_input()?
                        .ok_or_else(|| InputConfigError::UnsupportedValue {
                            kind: InputKind::Number,
                            value: InputValue::Number(0.0),
                        })?;
                let InputValue::Number(value) = input.value else {
                    return Err(InputConfigError::UnsupportedValue {
                        kind: InputKind::Number,
                        value: input.value.clone(),
                    }
                    .into());
                };
                let (min, max) = input.range.unwrap_or((0.0, 100.0));
                let mut slider = NativeSlider::new(min as f32, max as f32);
                slider.value = value as f32;
                Ok(self.store.insert(slider))
            }
            ElementKind::Custom(kind) if kind == "select" => {
                let input = spec
                    .typed_input()?
                    .ok_or_else(|| InputConfigError::InvalidChoice {
                        choice: ChoiceId::new(0),
                    })?;
                let InputValue::Choice(value) = input.value else {
                    return Err(InputConfigError::UnsupportedValue {
                        kind: InputKind::Choice,
                        value: input.value.clone(),
                    }
                    .into());
                };
                let mut dropdown = NativeDropdown::new(input.choices.clone());
                dropdown.selected = value.index();
                dropdown.highlighted = value.index();
                Ok(self.store.insert(dropdown))
            }
            ElementKind::Custom(kind) if Self::is_legacy_flex_kind(kind) => {
                Ok(self.store.insert(Self::make_legacy_flex(spec)?))
            }
            ElementKind::Custom(kind) if kind == "scroll" => {
                Ok(self.store.insert(Self::make_scroll(spec)))
            }
            ElementKind::Custom(kind) if kind == "styled" => {
                Ok(self.store.insert(self.make_styled(spec, identity)?))
            }
            ElementKind::Custom(kind) if kind == "virtual-list" => {
                Ok(self.store.insert(Self::make_virtual_list(spec)))
            }
            ElementKind::Custom(kind) if kind == "data-table" => {
                Ok(self.store.insert(Self::make_data_table(spec)))
            }
            ElementKind::Custom(kind) if kind == "virtual-tree" => {
                Ok(self.store.insert(Self::make_virtual_tree(spec)))
            }
            ElementKind::Custom(kind) if kind == "split-pane" => {
                Ok(self.store.insert(Self::make_split_pane(spec)))
            }
            ElementKind::Custom(kind) if kind == "tab-bar" => {
                Ok(self.store.insert(Self::make_tab_bar(spec)))
            }
            ElementKind::Custom(kind) if kind == "rich-document" => {
                Ok(self.store.insert(Self::make_rich_document(spec)))
            }
            ElementKind::Custom(kind) if kind == "app-bar" => {
                Ok(self.store.insert(Self::make_app_bar(spec)))
            }
            ElementKind::Custom(kind) if kind == "navigation-drawer" => {
                Ok(self.store.insert(Self::make_navigation_drawer(spec)))
            }
            ElementKind::Custom(kind) if kind == "bottom-navigation" => {
                Ok(self.store.insert(Self::make_bottom_navigation(spec)))
            }
            ElementKind::Custom(kind) if kind == "dialog" => {
                Ok(self.store.insert(Self::make_dialog(spec)))
            }
            ElementKind::Custom(kind) if kind == "card" => {
                Ok(self.store.insert(Self::make_card(spec)))
            }
            ElementKind::Custom(kind) if kind == "list-item" => {
                Ok(self.store.insert(self.make_list_item(spec, identity)))
            }
            ElementKind::Custom(kind) if kind == "bottom-sheet" => {
                Ok(self.store.insert(Self::make_bottom_sheet(spec)))
            }
            ElementKind::Custom(kind) if kind == "snack-bar" => {
                Ok(self.store.insert(Self::make_snack_bar(spec)))
            }
            ElementKind::Custom(kind) if kind == "icon" => {
                Ok(self.store.insert(Self::make_icon(spec)))
            }
            ElementKind::Custom(kind) if kind == "linear-progress" => {
                Ok(self.store.insert(Self::make_linear_progress(spec)))
            }
            ElementKind::Custom(kind) if kind == "circular-progress" => {
                Ok(self.store.insert(Self::make_circular_progress(spec)))
            }
            ElementKind::Custom(kind) if kind == "lottie" => {
                Ok(self.store.insert(Self::make_lottie(spec)?))
            }
            ElementKind::Custom(kind) if kind == "morphing-shape" => {
                Ok(self.store.insert(Self::make_morphing_shape(spec)))
            }
            ElementKind::Custom(kind) if kind == "label" => {
                Ok(self.store.insert(Self::make_facade_label(spec)))
            }
            ElementKind::Custom(kind) if kind == "image" => {
                Ok(self.store.insert(Self::make_image(spec)))
            }
            ElementKind::Custom(kind) if kind == "menu-bar" => {
                Ok(self.store.insert(Self::make_menu_bar(spec)))
            }
            ElementKind::Custom(kind) if kind == "context-menu" => {
                Ok(self.store.insert(Self::make_context_menu(spec)))
            }
            ElementKind::Container => Ok(self.store.insert(if spec.layout.is_some() {
                Self::make_container(spec)
            } else {
                Flex::new()
            })),
            ElementKind::Custom(_) => Ok(self.store.insert(Flex::new())),
        }
    }

    fn facade_menu_item(item: &MenuItem) -> NativeMenuItem {
        let mut native = NativeMenuItem::new(item.id.clone(), item.label.clone());
        native.enabled = item.enabled;
        native
    }

    fn facade_menus(menus: &[Menu]) -> Vec<NativeMenu> {
        menus
            .iter()
            .map(|menu| {
                NativeMenu::new(
                    menu.title.clone(),
                    menu.items.iter().map(Self::facade_menu_item).collect(),
                )
            })
            .collect()
    }

    fn make_menu_bar(spec: &ElementSpec) -> NativeMenuBar {
        let menus = spec
            .menu_bar_menus
            .as_deref()
            .map(Self::facade_menus)
            .unwrap_or_default();
        NativeMenuBar::new(menus)
    }

    fn make_context_menu(spec: &ElementSpec) -> NativeContextMenu {
        let items = spec
            .context_items
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(Self::facade_menu_item)
            .collect();
        let mut menu = NativeContextMenu::new(items);
        if let Some((x, y)) = spec.context_show_at {
            menu.show_at(Point::new(x, y));
        }
        menu
    }

    fn make_scroll(spec: &ElementSpec) -> NativeScrollView {
        let mut scroll = NativeScrollView::new();
        scroll.scroll_chaining = spec
            .properties
            .get("scroll-chaining")
            .map(|value| value != "false")
            .unwrap_or(true);
        scroll.set_background(
            spec.properties
                .get("background")
                .and_then(|value| Self::parse_css_color(value)),
        );
        scroll
    }

    fn make_styled(
        &self,
        spec: &ElementSpec,
        identity: &IdentityPath,
    ) -> Result<NativeStyledBox, ElementError> {
        let css = spec
            .properties
            .get("css")
            .cloned()
            .unwrap_or_else(|| String::from("width:fill"));
        let style =
            crate::style::validate(crate::style::StyleInput::text(css.clone())).map_err(|_| {
                ElementError::InvalidProperty {
                    kind: String::from("styled"),
                    name: "css",
                    value: Some(css),
                }
            })?;
        let mut styled = NativeStyledBox::new(style.into_engine());
        let has_press = spec.press_callback.is_some()
            || spec
                .event_slots
                .iter()
                .any(|slot| slot.event == Event::Press);
        if has_press {
            let slot = spec
                .event_slots
                .iter()
                .find(|slot| slot.event == Event::Press)
                .map_or(0, |slot| slot.slot);
            let route = ActionRouteKey {
                identity: identity.clone(),
                event: Event::Press,
                slot,
            };
            let routes = Rc::clone(&self.action_routes);
            let context = self.context.clone();
            styled = styled.on_press(move || {
                let action = routes.borrow().get(&route).map(RoutedAction::action);
                if let (Some(action), Some(context)) = (action, context.as_ref()) {
                    let _ = action.invoke(context, ());
                }
            });
        }
        Ok(styled)
    }

    fn make_virtual_list(spec: &ElementSpec) -> NativeListView {
        let row_count = spec
            .properties
            .get("row-count")
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        let item_extent = spec
            .properties
            .get("item-extent")
            .and_then(|value| value.parse().ok())
            .unwrap_or(32.0_f64)
            .max(1.0);
        let measure_heights = spec
            .properties
            .get("measure-heights")
            .is_some_and(|value| value == "true");
        let mut list = NativeListView::new(row_count, item_extent);
        if measure_heights {
            list.enable_measure_heights(true);
        }
        let label_factory = spec
            .item_label
            .clone()
            .unwrap_or_else(|| ItemLabelFactory::new(|index| format!("Item {index}")));
        let label_for_builder = label_factory.clone();
        list.set_label_fn(move |index| label_factory.label(index));
        list.set_item_builder(move |index| Box::new(Label::new(label_for_builder.label(index))));
        list
    }

    fn parse_table_columns(spec: &ElementSpec) -> Vec<NativeColumnDef> {
        spec.properties
            .get("columns")
            .map(|encoded| {
                encoded
                    .split(';')
                    .filter(|part| !part.is_empty())
                    .filter_map(|part| {
                        let mut pieces = part.split('|');
                        let id = pieces.next()?.to_owned();
                        let title = pieces.next()?.to_owned();
                        let width = pieces.next()?.parse().unwrap_or(80.0);
                        Some(NativeColumnDef::new(id, title, width))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn parse_table_spans(spec: &ElementSpec) -> Vec<NativeCellSpan> {
        spec.properties
            .get("spans")
            .map(|encoded| {
                encoded
                    .split(';')
                    .filter(|part| !part.is_empty())
                    .filter_map(|part| {
                        let mut pieces = part.split('|');
                        let row: usize = pieces.next()?.parse().ok()?;
                        let col: usize = pieces.next()?.parse().ok()?;
                        let row_span: usize = pieces.next()?.parse().ok()?;
                        let col_span: usize = pieces.next()?.parse().ok()?;
                        Some(NativeCellSpan::new(row, col, row_span, col_span))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn make_data_table(spec: &ElementSpec) -> NativeDataTable {
        let columns = Self::parse_table_columns(spec);
        let row_count = spec
            .properties
            .get("row-count")
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        let freeze_leading = spec
            .properties
            .get("freeze-leading")
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        let filter_query = spec
            .properties
            .get("filter-query")
            .cloned()
            .unwrap_or_default();
        let mount_cells = spec
            .properties
            .get("mount-cells")
            .map(|value| value != "false")
            .unwrap_or(true);
        let cell = spec
            .table_cell
            .clone()
            .unwrap_or_else(|| TableCellFactory::new(|row, col| format!("{row},{col}")));
        let mut table =
            NativeDataTable::new(columns, row_count).with_freeze_leading(freeze_leading);
        let cell_for_fn = cell.clone();
        table.set_cell_fn(move |row, col| cell_for_fn.cell(row, col));
        table.set_filter_query(filter_query);
        table.set_spans(Self::parse_table_spans(spec));
        if mount_cells {
            let cell_for_mount = cell;
            table.set_cell_builder(move |row, col| {
                Box::new(Label::new(cell_for_mount.cell(row, col)))
            });
        }
        table
    }

    fn facade_tree_to_native(nodes: &[TreeNode]) -> Vec<NativeTreeNode> {
        nodes
            .iter()
            .map(|node| {
                NativeTreeNode::new(node.id.clone(), node.label.clone())
                    .with_children(Self::facade_tree_to_native(&node.children))
            })
            .collect()
    }

    fn make_virtual_tree(spec: &ElementSpec) -> NativeTreeView {
        let roots = spec.tree_roots.clone().unwrap_or_default();
        // Paint-path labels (no cell mount); expand/select still virtualized by TreeView.
        NativeTreeView::new(Self::facade_tree_to_native(&roots))
    }

    fn make_split_pane(spec: &ElementSpec) -> NativeSplitPane {
        let direction = match spec.properties.get("direction").map(String::as_str) {
            Some("vertical") => NativeSplitDirection::Vertical,
            _ => NativeSplitDirection::Horizontal,
        };
        let ratio = spec
            .properties
            .get("ratio")
            .and_then(|value| value.parse().ok())
            .unwrap_or(0.35_f32);
        let mut pane = NativeSplitPane::new(direction).with_ratio(ratio);
        if let Some(color) = spec
            .properties
            .get("divider-color")
            .and_then(|value| Self::parse_css_color(value))
        {
            pane.set_divider_color(Some(color));
        }
        if let Some(css) = spec.properties.get("divider-css") {
            let _ = pane.set_divider_css(css);
        }
        pane
    }

    fn parse_css_color(raw: &str) -> Option<synui_render::peniko::Color> {
        let trimmed = raw.trim();
        if trimmed.eq_ignore_ascii_case("transparent") {
            return Some(synui_render::peniko::Color::TRANSPARENT);
        }
        let s = trimmed.trim_start_matches('#');
        let (r, g, b, a) = match s.len() {
            3 => {
                let r = u8::from_str_radix(&s[0..1].repeat(2), 16).ok()?;
                let g = u8::from_str_radix(&s[1..2].repeat(2), 16).ok()?;
                let b = u8::from_str_radix(&s[2..3].repeat(2), 16).ok()?;
                (r, g, b, 255)
            }
            6 => {
                let r = u8::from_str_radix(&s[0..2], 16).ok()?;
                let g = u8::from_str_radix(&s[2..4], 16).ok()?;
                let b = u8::from_str_radix(&s[4..6], 16).ok()?;
                (r, g, b, 255)
            }
            8 => {
                let r = u8::from_str_radix(&s[0..2], 16).ok()?;
                let g = u8::from_str_radix(&s[2..4], 16).ok()?;
                let b = u8::from_str_radix(&s[4..6], 16).ok()?;
                let a = u8::from_str_radix(&s[6..8], 16).ok()?;
                (r, g, b, a)
            }
            _ => return None,
        };
        Some(synui_render::peniko::Color::from_rgba8(r, g, b, a))
    }

    fn apply_text_area_chrome(area: &mut NativeTextArea, spec: &ElementSpec) {
        if let Some(raw) = spec.properties.get("background") {
            area.set_background(Self::parse_css_color(raw));
        } else {
            area.set_background(None);
        }
        area.set_border(
            spec.properties
                .get("border")
                .map(|value| value != "false")
                .unwrap_or(true),
        );
    }

    fn make_tab_bar(spec: &ElementSpec) -> NativeTabBar {
        let labels = spec
            .properties
            .get("labels")
            .map(|encoded| {
                encoded
                    .split('|')
                    .filter(|part| !part.is_empty())
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let selected = spec
            .properties
            .get("selected")
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        let mut bar = NativeTabBar::new(labels);
        bar.selected = selected;
        bar.set_background(
            spec.properties
                .get("background")
                .and_then(|value| Self::parse_css_color(value)),
        );
        bar
    }

    fn make_rich_document(spec: &ElementSpec) -> NativeRichEdit {
        let doc = spec
            .rich_doc
            .clone()
            .unwrap_or_else(|| synui_core::BlockDoc::from_plain(""));
        NativeRichEdit::with_doc(doc)
    }

    fn make_app_bar(spec: &ElementSpec) -> NativeAppBar {
        let mut bar = NativeAppBar::new(
            spec.properties
                .get("title")
                .cloned()
                .unwrap_or_else(|| "App".into()),
        );
        bar.set_background(
            spec.properties
                .get("background")
                .and_then(|value| Self::parse_css_color(value)),
        );
        bar
    }

    fn make_navigation_drawer(spec: &ElementSpec) -> NativeNavigationDrawer {
        let mut drawer = NativeNavigationDrawer::new();
        drawer.open = spec
            .properties
            .get("open")
            .map(|value| value == "true")
            .unwrap_or(false);
        drawer.set_background(
            spec.properties
                .get("background")
                .and_then(|value| Self::parse_css_color(value)),
        );
        drawer
    }

    fn make_bottom_navigation(spec: &ElementSpec) -> NativeBottomNavigation {
        let labels = spec
            .properties
            .get("labels")
            .map(|encoded| {
                encoded
                    .split('|')
                    .filter(|part| !part.is_empty())
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let selected = spec
            .properties
            .get("selected")
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        let mut bar = NativeBottomNavigation::new(labels);
        bar.selected = selected;
        bar.set_background(
            spec.properties
                .get("background")
                .and_then(|value| Self::parse_css_color(value)),
        );
        bar
    }

    fn make_dialog(spec: &ElementSpec) -> NativeDialog {
        let mut dialog = NativeDialog::new(
            spec.properties
                .get("title")
                .cloned()
                .unwrap_or_else(|| "Dialog".into()),
        );
        dialog.open = spec
            .properties
            .get("open")
            .map(|value| value == "true")
            .unwrap_or(false);
        dialog
    }

    fn make_card(spec: &ElementSpec) -> NativeCard {
        let mut card = NativeCard::new();
        card.elevation = spec
            .properties
            .get("elevation")
            .and_then(|value| value.parse().ok())
            .unwrap_or(1);
        card
    }

    fn make_list_item(&self, spec: &ElementSpec, identity: &IdentityPath) -> NativeListItem {
        let mut item =
            NativeListItem::new(spec.properties.get("title").cloned().unwrap_or_default());
        item.subtitle = spec.properties.get("subtitle").cloned();
        let slot = spec
            .event_slots
            .iter()
            .find(|slot| slot.event == Event::Press)
            .map_or(0, |slot| slot.slot);
        let route = ActionRouteKey {
            identity: identity.clone(),
            event: Event::Press,
            slot,
        };
        let routes = Rc::clone(&self.action_routes);
        let context = self.context.clone();
        item = item.on_press(move || {
            let action = routes.borrow().get(&route).map(RoutedAction::action);
            if let (Some(action), Some(context)) = (action, context.as_ref()) {
                let _ = action.invoke(context, ());
            }
        });
        item.set_background(
            spec.properties
                .get("background")
                .and_then(|value| Self::parse_css_color(value)),
        );
        item
    }

    fn make_bottom_sheet(spec: &ElementSpec) -> NativeBottomSheet {
        let mut sheet = NativeBottomSheet::new();
        sheet.open = spec
            .properties
            .get("open")
            .map(|value| value == "true")
            .unwrap_or(false);
        sheet
    }

    fn make_snack_bar(spec: &ElementSpec) -> NativeSnackBar {
        let mut snack =
            NativeSnackBar::new(spec.properties.get("message").cloned().unwrap_or_default());
        snack.visible = spec
            .properties
            .get("visible")
            .map(|value| value == "true")
            .unwrap_or(false);
        snack
    }

    fn make_icon(spec: &ElementSpec) -> NativeIcon {
        let name = spec
            .properties
            .get("name")
            .cloned()
            .unwrap_or_else(|| "circle".into());
        let size = spec
            .properties
            .get("size")
            .and_then(|value| value.parse().ok())
            .unwrap_or(24.0_f32);
        let mut icon = NativeIcon::new(IconId(name));
        icon.size = size;
        icon
    }

    fn make_linear_progress(spec: &ElementSpec) -> NativeLinearProgress {
        let value = spec
            .properties
            .get("value")
            .and_then(|value| value.parse::<f32>().ok())
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        let indeterminate = spec
            .properties
            .get("indeterminate")
            .map(|value| value == "true")
            .unwrap_or(false);
        let mut progress = NativeLinearProgress::new();
        progress.value = value;
        progress.indeterminate = indeterminate;
        progress
    }

    fn make_circular_progress(spec: &ElementSpec) -> NativeCircularProgress {
        let value = spec
            .properties
            .get("value")
            .and_then(|value| value.parse::<f32>().ok())
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        let indeterminate = spec
            .properties
            .get("indeterminate")
            .map(|value| value == "true")
            .unwrap_or(true);
        let mut progress = NativeCircularProgress::new();
        progress.value = value;
        progress.indeterminate = indeterminate;
        progress
    }

    fn make_lottie(spec: &ElementSpec) -> Result<NativeLottie, ElementError> {
        let asset_id = spec
            .properties
            .get("id")
            .map(String::as_str)
            .filter(|id| !id.is_empty());
        let json = spec.properties.get("json").map(String::as_str);
        let mut lottie = match (asset_id, json) {
            (Some(id), _) => NativeLottie::from_asset(id),
            (None, Some(json)) => {
                NativeLottie::from_json(json).map_err(|err| ElementError::InvalidProperty {
                    kind: String::from("lottie"),
                    name: "json",
                    value: Some(format!("{err}")),
                })?
            }
            (None, None) => {
                return Err(ElementError::InvalidProperty {
                    kind: String::from("lottie"),
                    name: "id",
                    value: Some(String::from("missing id or json")),
                });
            }
        };
        let width = spec
            .properties
            .get("width")
            .and_then(|value| value.parse().ok())
            .unwrap_or(160.0);
        let height = spec
            .properties
            .get("height")
            .and_then(|value| value.parse().ok())
            .unwrap_or(160.0);
        lottie.set_size(width, height);
        let playing = spec
            .properties
            .get("playing")
            .map(|value| value != "false")
            .unwrap_or(true);
        lottie.set_playing(playing);
        Ok(lottie)
    }

    fn make_morphing_shape(spec: &ElementSpec) -> NativeMorphingShape {
        let size = spec
            .properties
            .get("size")
            .and_then(|value| value.parse().ok())
            .unwrap_or(120.0);
        let playing = spec
            .properties
            .get("playing")
            .map(|value| value != "false")
            .unwrap_or(true);
        let speed = spec
            .properties
            .get("speed")
            .and_then(|value| value.parse().ok())
            .unwrap_or(0.45);
        let mut morph = NativeMorphingShape::new().with_size(size);
        morph.set_playing(playing);
        morph.set_speed(speed);
        morph
    }

    fn facade_label_display_text(spec: &ElementSpec) -> String {
        let text = spec.properties.get("text").cloned().unwrap_or_default();
        let truncate = spec
            .properties
            .get("truncate")
            .map(|value| value == "true")
            .unwrap_or(false);
        if !truncate {
            return text;
        }
        let max_lines = spec
            .properties
            .get("max_lines")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(1)
            .max(1);
        let lines: Vec<&str> = text.lines().collect();
        if lines.len() <= max_lines {
            return text;
        }
        let mut clipped = lines[..max_lines].join("\n");
        clipped.push('…');
        clipped
    }

    fn make_facade_label(spec: &ElementSpec) -> Label {
        let truncate = spec
            .properties
            .get("truncate")
            .map(|value| value == "true")
            .unwrap_or(false);
        let wrap = spec
            .properties
            .get("wrap")
            .map(|value| value == "true")
            .unwrap_or(false);
        let mut label = Label::new(Self::facade_label_display_text(spec));
        label.truncate = truncate;
        label.wrap = wrap;
        label
    }

    fn make_image(spec: &ElementSpec) -> NativeImage {
        let id = spec
            .properties
            .get("id")
            .cloned()
            .unwrap_or_else(|| "image".into());
        let width = spec
            .properties
            .get("width")
            .and_then(|value| value.parse().ok())
            .unwrap_or(120.0_f32);
        let height = spec
            .properties
            .get("height")
            .and_then(|value| value.parse().ok())
            .unwrap_or(120.0_f32);
        NativeImage::new(ImageId(id)).with_size(width, height)
    }

    fn sync_host_children(
        &mut self,
        widget: WidgetId,
        child_ids: &[WidgetId],
        spec: &ElementSpec,
    ) -> Result<(), ElementError> {
        match &spec.kind {
            ElementKind::Custom(kind) if kind == "scroll" => {
                let scroll = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeScrollView>())
                    .ok_or(ElementError::NativeWidgetType {
                        expected: "ScrollView",
                    })?;
                scroll.replace_children(child_ids.to_vec());
            }
            ElementKind::Custom(kind) if kind == "styled" => {
                let styled = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeStyledBox>())
                    .ok_or(ElementError::NativeWidgetType {
                        expected: "StyledBox",
                    })?;
                styled.replace_children(child_ids.to_vec());
            }
            ElementKind::Custom(kind) if kind == "split-pane" => {
                let split = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeSplitPane>())
                    .ok_or(ElementError::NativeWidgetType {
                        expected: "SplitPane",
                    })?;
                split.replace_children(child_ids.to_vec());
            }
            ElementKind::Custom(kind) if kind == "app-bar" => {
                let bar = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeAppBar>())
                    .ok_or(ElementError::NativeWidgetType { expected: "AppBar" })?;
                bar.replace_children(child_ids.to_vec());
            }
            ElementKind::Custom(kind) if kind == "navigation-drawer" => {
                let drawer = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeNavigationDrawer>())
                    .ok_or(ElementError::NativeWidgetType {
                        expected: "NavigationDrawer",
                    })?;
                drawer.replace_children(child_ids.to_vec());
            }
            ElementKind::Custom(kind) if kind == "dialog" => {
                let dialog = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeDialog>())
                    .ok_or(ElementError::NativeWidgetType { expected: "Dialog" })?;
                dialog.replace_children(child_ids.to_vec());
            }
            ElementKind::Custom(kind) if kind == "card" => {
                let card = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeCard>())
                    .ok_or(ElementError::NativeWidgetType { expected: "Card" })?;
                card.replace_children(child_ids.to_vec());
            }
            ElementKind::Custom(kind) if kind == "list-item" => {
                let item = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeListItem>())
                    .ok_or(ElementError::NativeWidgetType {
                        expected: "ListItem",
                    })?;
                item.replace_children(child_ids.to_vec());
            }
            ElementKind::Custom(kind) if kind == "bottom-sheet" => {
                let sheet = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeBottomSheet>())
                    .ok_or(ElementError::NativeWidgetType {
                        expected: "BottomSheet",
                    })?;
                sheet.replace_children(child_ids.to_vec());
            }
            _ => {}
        }
        Ok(())
    }

    fn make_button(&self, spec: &ElementSpec, identity: &IdentityPath) -> NativeButton {
        let mut button =
            NativeButton::new(spec.properties.get("label").cloned().unwrap_or_default());
        let slot = spec
            .event_slots
            .iter()
            .find(|slot| slot.event == Event::Press)
            .map_or(0, |slot| slot.slot);
        let route = ActionRouteKey {
            identity: identity.clone(),
            event: Event::Press,
            slot,
        };
        let routes = Rc::clone(&self.action_routes);
        let context = self.context.clone();
        button = button.on_press(move || {
            // Clone the target while borrowing the route table, then release the
            // borrow before entering the App runtime or user callback.
            let action = routes.borrow().get(&route).map(RoutedAction::action);
            if let (Some(action), Some(context)) = (action, context.as_ref()) {
                let _ = action.invoke(context, ());
            }
        });
        button
    }

    fn update_widget(
        &mut self,
        mounted: &mut Mounted,
        next: &ElementSpec,
        identity: &IdentityPath,
    ) -> Result<(), ElementError> {
        Self::validate_spec(next)?;
        let widget = mounted.widget.ok_or(ElementError::NotMounted)?;
        Self::sync_tooltip_prop(&mut self.store, widget, next);
        match (&mounted.previous.kind, &next.kind) {
            (ElementKind::Text, ElementKind::Text) => {
                let label = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<Label>())
                    .ok_or(ElementError::NativeWidgetType { expected: "Label" })?;
                label.set_text(next.text.clone().ok_or(ElementError::MissingText)?);
            }
            (ElementKind::Custom(old), ElementKind::Custom(new))
                if old == "button" && new == "button" =>
            {
                let button = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeButton>())
                    .ok_or(ElementError::NativeWidgetType { expected: "Button" })?;
                button.label = next.properties.get("label").cloned().unwrap_or_default();
            }
            (ElementKind::Custom(old), ElementKind::Custom(new))
                if old == "card" && new == "card" =>
            {
                let card = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeCard>())
                    .ok_or(ElementError::NativeWidgetType { expected: "Card" })?;
                card.elevation = next
                    .properties
                    .get("elevation")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(1);
            }
            (ElementKind::Custom(old), ElementKind::Custom(new))
                if old == "list-item" && new == "list-item" =>
            {
                let item = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeListItem>())
                    .ok_or(ElementError::NativeWidgetType {
                        expected: "ListItem",
                    })?;
                item.title = next.properties.get("title").cloned().unwrap_or_default();
                item.subtitle = next.properties.get("subtitle").cloned();
                item.set_background(
                    next.properties
                        .get("background")
                        .and_then(|value| Self::parse_css_color(value)),
                );
            }
            (ElementKind::Custom(old), ElementKind::Custom(new))
                if old == "text-input" && new == "text-input" =>
            {
                let focused = self.store.focus_manager().focused() == Some(widget);
                let field = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeTextField>())
                    .ok_or(ElementError::NativeWidgetType {
                        expected: "TextField",
                    })?;
                field.placeholder = next
                    .properties
                    .get("placeholder")
                    .cloned()
                    .unwrap_or_default();
                field.password = Self::password_prop(next);
                let input = next.typed_input()?.ok_or(ElementError::NativeWidgetType {
                    expected: "typed TextField spec",
                })?;
                if input.mode == InputMode::Controlled {
                    let InputValue::Text(value) = &input.value else {
                        return Err(InputConfigError::UnsupportedValue {
                            kind: InputKind::Text,
                            value: input.value.clone(),
                        }
                        .into());
                    };
                    if !focused && field.edit.text != *value {
                        field.edit.text = value.clone();
                        field.notify_text_changed();
                    }
                }
            }
            (ElementKind::Custom(old), ElementKind::Custom(new))
                if old == "text-area" && new == "text-area" =>
            {
                let focused = self.store.focus_manager().focused() == Some(widget);
                let area = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeTextArea>())
                    .ok_or(ElementError::NativeWidgetType {
                        expected: "TextArea",
                    })?;
                let input = next.typed_input()?.ok_or(ElementError::NativeWidgetType {
                    expected: "typed TextArea spec",
                })?;
                if input.mode == InputMode::Controlled {
                    let InputValue::Text(value) = &input.value else {
                        return Err(InputConfigError::UnsupportedValue {
                            kind: InputKind::Text,
                            value: input.value.clone(),
                        }
                        .into());
                    };
                    if !focused && area.edit.text != *value {
                        area.edit.text = value.clone();
                        area.notify_text_changed();
                    }
                }
                Self::apply_text_area_chrome(area, next);
            }
            (ElementKind::Custom(old), ElementKind::Custom(new))
                if old == "checkbox" && new == "checkbox" =>
            {
                let input = next.typed_input()?.ok_or(ElementError::NativeWidgetType {
                    expected: "typed Checkbox spec",
                })?;
                let checkbox = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeCheckbox>())
                    .ok_or(ElementError::NativeWidgetType {
                        expected: "Checkbox",
                    })?;
                if let Some(label) = next.properties.get("label") {
                    checkbox.label = label.clone();
                }
                if input.mode == InputMode::Controlled {
                    let InputValue::Bool(value) = &input.value else {
                        return Err(InputConfigError::UnsupportedValue {
                            kind: InputKind::Bool,
                            value: input.value.clone(),
                        }
                        .into());
                    };
                    checkbox.checked = *value;
                }
            }
            (ElementKind::Custom(old), ElementKind::Custom(new))
                if old == "switch" && new == "switch" =>
            {
                let input = next.typed_input()?.ok_or(ElementError::NativeWidgetType {
                    expected: "typed Switch spec",
                })?;
                if input.mode == InputMode::Controlled {
                    let InputValue::Bool(value) = &input.value else {
                        return Err(InputConfigError::UnsupportedValue {
                            kind: InputKind::Bool,
                            value: input.value.clone(),
                        }
                        .into());
                    };
                    let switch = self
                        .store
                        .get_mut(widget)
                        .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeSwitch>())
                        .ok_or(ElementError::NativeWidgetType { expected: "Switch" })?;
                    switch.on = *value;
                }
            }
            (ElementKind::Custom(old), ElementKind::Custom(new))
                if old == "radio" && new == "radio" =>
            {
                let input = next.typed_input()?.ok_or(ElementError::NativeWidgetType {
                    expected: "typed Radio spec",
                })?;
                let radio = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeRadio>())
                    .ok_or(ElementError::NativeWidgetType { expected: "Radio" })?;
                if let Some(label) = next.properties.get("label") {
                    radio.label = label.clone();
                }
                if input.mode == InputMode::Controlled {
                    let InputValue::Bool(value) = &input.value else {
                        return Err(InputConfigError::UnsupportedValue {
                            kind: InputKind::Bool,
                            value: input.value.clone(),
                        }
                        .into());
                    };
                    radio.selected = *value;
                }
            }
            (ElementKind::Custom(old), ElementKind::Custom(new))
                if old == "number-input" && new == "number-input" =>
            {
                let input = next.typed_input()?.ok_or(ElementError::NativeWidgetType {
                    expected: "typed Slider spec",
                })?;
                let (min, max) = input.range.unwrap_or((0.0, 100.0));
                let slider = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeSlider>())
                    .ok_or(ElementError::NativeWidgetType { expected: "Slider" })?;
                slider.min = min as f32;
                slider.max = max as f32;
                if input.mode == InputMode::Controlled {
                    let InputValue::Number(value) = &input.value else {
                        return Err(InputConfigError::UnsupportedValue {
                            kind: InputKind::Number,
                            value: input.value.clone(),
                        }
                        .into());
                    };
                    slider.value = *value as f32;
                } else {
                    slider.value = slider.value.clamp(slider.min, slider.max);
                }
            }
            (ElementKind::Custom(old), ElementKind::Custom(new))
                if old == "select" && new == "select" =>
            {
                let input = next.typed_input()?.ok_or(ElementError::NativeWidgetType {
                    expected: "typed Dropdown spec",
                })?;
                let dropdown = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeDropdown>())
                    .ok_or(ElementError::NativeWidgetType {
                        expected: "Dropdown",
                    })?;
                dropdown.options = input.choices.clone();
                if input.mode == InputMode::Controlled {
                    let InputValue::Choice(value) = &input.value else {
                        return Err(InputConfigError::UnsupportedValue {
                            kind: InputKind::Choice,
                            value: input.value.clone(),
                        }
                        .into());
                    };
                    dropdown.selected = value.index();
                    dropdown.highlighted = value.index();
                } else if dropdown.selected >= dropdown.options.len() {
                    let InputValue::Choice(value) = &input.value else {
                        return Err(InputConfigError::UnsupportedValue {
                            kind: InputKind::Choice,
                            value: input.value.clone(),
                        }
                        .into());
                    };
                    dropdown.selected = value.index();
                    dropdown.highlighted = value.index();
                }
            }
            (ElementKind::Custom(old), ElementKind::Custom(new))
                if old == "scroll" && new == "scroll" =>
            {
                let scroll = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeScrollView>())
                    .ok_or(ElementError::NativeWidgetType {
                        expected: "ScrollView",
                    })?;
                scroll.scroll_chaining = next
                    .properties
                    .get("scroll-chaining")
                    .map(|value| value != "false")
                    .unwrap_or(true);
                scroll.set_background(
                    next.properties
                        .get("background")
                        .and_then(|value| Self::parse_css_color(value)),
                );
            }
            (ElementKind::Custom(old), ElementKind::Custom(new))
                if old == "styled" && new == "styled" =>
            {
                let css = next
                    .properties
                    .get("css")
                    .cloned()
                    .unwrap_or_else(|| String::from("width:fill"));
                let style = crate::style::validate(crate::style::StyleInput::text(css.clone()))
                    .map_err(|_| ElementError::InvalidProperty {
                        kind: String::from("styled"),
                        name: "css",
                        value: Some(css),
                    })?;
                let engine_style = style.into_engine();
                let styled = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeStyledBox>())
                    .ok_or(ElementError::NativeWidgetType {
                        expected: "StyledBox",
                    })?;
                styled.apply_style(&engine_style);
            }
            (ElementKind::Custom(old), ElementKind::Custom(new))
                if old == "virtual-list" && new == "virtual-list" =>
            {
                let row_count = next
                    .properties
                    .get("row-count")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0);
                let item_extent = next
                    .properties
                    .get("item-extent")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(32.0_f64)
                    .max(1.0);
                let measure_heights = next
                    .properties
                    .get("measure-heights")
                    .is_some_and(|value| value == "true");
                let label_factory = next
                    .item_label
                    .clone()
                    .unwrap_or_else(|| ItemLabelFactory::new(|index| format!("Item {index}")));
                let list = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeListView>())
                    .ok_or(ElementError::NativeWidgetType {
                        expected: "ListView",
                    })?;
                list.item_count = row_count;
                list.item_extent = item_extent;
                list.enable_measure_heights(measure_heights);
                let label_for_builder = label_factory.clone();
                list.set_label_fn(move |index| label_factory.label(index));
                list.set_item_builder(move |index| {
                    Box::new(Label::new(label_for_builder.label(index)))
                });
                list.ensure_height_cache();
            }
            (ElementKind::Custom(old), ElementKind::Custom(new))
                if old == "data-table" && new == "data-table" =>
            {
                let columns = Self::parse_table_columns(next);
                let row_count = next
                    .properties
                    .get("row-count")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0);
                let freeze_leading = next
                    .properties
                    .get("freeze-leading")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0);
                let filter_query = next
                    .properties
                    .get("filter-query")
                    .cloned()
                    .unwrap_or_default();
                let mount_cells = next
                    .properties
                    .get("mount-cells")
                    .map(|value| value != "false")
                    .unwrap_or(true);
                let cell = next
                    .table_cell
                    .clone()
                    .unwrap_or_else(|| TableCellFactory::new(|row, col| format!("{row},{col}")));
                let (mounted_texts, spans_changed) = {
                    let table = self
                        .store
                        .get_mut(widget)
                        .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeDataTable>())
                        .ok_or(ElementError::NativeWidgetType {
                            expected: "DataTable",
                        })?;
                    let structure_changed = table.data_row_count != row_count
                        || table.columns.len() != columns.len()
                        || table
                            .columns
                            .iter()
                            .zip(columns.iter())
                            .any(|(prev, next_col)| {
                                prev.id != next_col.id
                                    || prev.title != next_col.title
                                    || (prev.width - next_col.width).abs() > f64::EPSILON
                            });
                    let filter_changed = table.filter_query != filter_query;
                    // Avoid set_columns_and_rows on every State rebuild — it resets sort /
                    // heights and was a major source of selection flicker.
                    if structure_changed {
                        table.set_columns_and_rows(columns, row_count);
                    }
                    if table.freeze_leading != freeze_leading {
                        table.freeze_leading = freeze_leading;
                    }
                    let cell_for_fn = cell.clone();
                    table.set_cell_fn(move |row, col| cell_for_fn.cell(row, col));
                    // Refresh builders/labels only when structure/filter changes or an
                    // edit may have changed cell text (detected below by diff).
                    if mount_cells && (structure_changed || filter_changed || !table.uses_mount()) {
                        let cell_for_mount = cell;
                        table.set_cell_builder(move |row, col| {
                            Box::new(Label::new(cell_for_mount.cell(row, col)))
                        });
                    }
                    if filter_changed {
                        table.set_filter_query(filter_query);
                    }
                    let want_spans = Self::parse_table_spans(next);
                    let spans_changed = table.spans() != want_spans.as_slice();
                    if spans_changed {
                        table.set_spans(want_spans);
                    }
                    let mounted = table
                        .mounted_cells()
                        .iter()
                        .map(|&(visual_row, col, cell_id)| {
                            let text = table.cell_text(table.data_row(visual_row), col);
                            (cell_id, text)
                        })
                        .collect::<Vec<_>>();
                    (mounted, spans_changed)
                };
                if spans_changed {
                    self.store.mark_layout_dirty_widget(widget);
                }
                for (cell_id, text) in mounted_texts {
                    if let Some(label) = self
                        .store
                        .get_mut(cell_id)
                        .and_then(|widget| widget.as_any_mut().downcast_mut::<Label>())
                    {
                        if label.text != text {
                            label.set_text(text);
                            self.store.bump_paint_epoch_widget(cell_id);
                        }
                    }
                }
            }
            (ElementKind::Custom(old), ElementKind::Custom(new))
                if old == "virtual-tree" && new == "virtual-tree" =>
            {
                let roots = next.tree_roots.clone().unwrap_or_default();
                let native_roots = Self::facade_tree_to_native(&roots);
                let tree = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeTreeView>())
                    .ok_or(ElementError::NativeWidgetType {
                        expected: "TreeView",
                    })?;
                if tree.roots != native_roots {
                    tree.set_roots(native_roots);
                }
            }
            (ElementKind::Custom(old), ElementKind::Custom(new))
                if old == "split-pane" && new == "split-pane" =>
            {
                let direction = match next.properties.get("direction").map(String::as_str) {
                    Some("vertical") => NativeSplitDirection::Vertical,
                    _ => NativeSplitDirection::Horizontal,
                };
                let ratio = next
                    .properties
                    .get("ratio")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0.35_f32);
                let divider_color = next
                    .properties
                    .get("divider-color")
                    .and_then(|value| Self::parse_css_color(value));
                let divider_css = next.properties.get("divider-css").cloned();
                let split = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeSplitPane>())
                    .ok_or(ElementError::NativeWidgetType {
                        expected: "SplitPane",
                    })?;
                split.direction = direction;
                // Keep the live drag ratio; controlled State catches up after PointerUp.
                if !split.is_dragging() {
                    split.set_ratio(ratio);
                }
                split.set_divider_color(divider_color);
                if let Some(css) = divider_css {
                    let _ = split.set_divider_css(&css);
                } else {
                    split.set_divider_visual(synui_core::VisualStyle::default());
                }
            }
            (ElementKind::Custom(old), ElementKind::Custom(new))
                if old == "tab-bar" && new == "tab-bar" =>
            {
                let labels = next
                    .properties
                    .get("labels")
                    .map(|encoded| {
                        encoded
                            .split('|')
                            .filter(|part| !part.is_empty())
                            .map(str::to_owned)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let selected = next
                    .properties
                    .get("selected")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0);
                let tab = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeTabBar>())
                    .ok_or(ElementError::NativeWidgetType { expected: "TabBar" })?;
                tab.tabs = labels;
                tab.selected = selected.min(tab.tabs.len().saturating_sub(1));
                tab.set_background(
                    next.properties
                        .get("background")
                        .and_then(|value| Self::parse_css_color(value)),
                );
            }
            (ElementKind::Custom(old), ElementKind::Custom(new))
                if old == "rich-document" && new == "rich-document" =>
            {
                let doc = next
                    .rich_doc
                    .clone()
                    .unwrap_or_else(|| synui_core::BlockDoc::from_plain(""));
                let focused = self.store.focus_manager().focused() == Some(widget);
                let editor = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeRichEdit>())
                    .ok_or(ElementError::NativeWidgetType {
                        expected: "RichEdit",
                    })?;
                if !focused && editor.doc != doc {
                    editor.set_doc(doc);
                }
            }
            (ElementKind::Custom(old), ElementKind::Custom(new))
                if old == "app-bar" && new == "app-bar" =>
            {
                let title = next
                    .properties
                    .get("title")
                    .cloned()
                    .unwrap_or_else(|| "App".into());
                let bar = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeAppBar>())
                    .ok_or(ElementError::NativeWidgetType { expected: "AppBar" })?;
                bar.title = title;
                bar.set_background(
                    next.properties
                        .get("background")
                        .and_then(|value| Self::parse_css_color(value)),
                );
            }
            (ElementKind::Custom(old), ElementKind::Custom(new))
                if old == "navigation-drawer" && new == "navigation-drawer" =>
            {
                let open = next
                    .properties
                    .get("open")
                    .map(|value| value == "true")
                    .unwrap_or(false);
                let drawer = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeNavigationDrawer>())
                    .ok_or(ElementError::NativeWidgetType {
                        expected: "NavigationDrawer",
                    })?;
                drawer.open = open;
                drawer.set_background(
                    next.properties
                        .get("background")
                        .and_then(|value| Self::parse_css_color(value)),
                );
            }
            (ElementKind::Custom(old), ElementKind::Custom(new))
                if old == "bottom-navigation" && new == "bottom-navigation" =>
            {
                let labels = next
                    .properties
                    .get("labels")
                    .map(|encoded| {
                        encoded
                            .split('|')
                            .filter(|part| !part.is_empty())
                            .map(str::to_owned)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let selected = next
                    .properties
                    .get("selected")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0);
                let bar = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeBottomNavigation>())
                    .ok_or(ElementError::NativeWidgetType {
                        expected: "BottomNavigation",
                    })?;
                bar.labels = labels;
                bar.selected = selected.min(bar.labels.len().saturating_sub(1));
                bar.set_background(
                    next.properties
                        .get("background")
                        .and_then(|value| Self::parse_css_color(value)),
                );
            }
            (ElementKind::Custom(old), ElementKind::Custom(new))
                if old == "dialog" && new == "dialog" =>
            {
                let title = next
                    .properties
                    .get("title")
                    .cloned()
                    .unwrap_or_else(|| "Dialog".into());
                let open = next
                    .properties
                    .get("open")
                    .map(|value| value == "true")
                    .unwrap_or(false);
                let dialog = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeDialog>())
                    .ok_or(ElementError::NativeWidgetType { expected: "Dialog" })?;
                dialog.title = title;
                dialog.open = open;
            }
            (ElementKind::Custom(old), ElementKind::Custom(new))
                if old == "bottom-sheet" && new == "bottom-sheet" =>
            {
                let open = next
                    .properties
                    .get("open")
                    .map(|value| value == "true")
                    .unwrap_or(false);
                let sheet = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeBottomSheet>())
                    .ok_or(ElementError::NativeWidgetType {
                        expected: "BottomSheet",
                    })?;
                sheet.open = open;
            }
            (ElementKind::Custom(old), ElementKind::Custom(new))
                if old == "snack-bar" && new == "snack-bar" =>
            {
                let message = next.properties.get("message").cloned().unwrap_or_default();
                let visible = next
                    .properties
                    .get("visible")
                    .map(|value| value == "true")
                    .unwrap_or(false);
                let snack = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeSnackBar>())
                    .ok_or(ElementError::NativeWidgetType {
                        expected: "SnackBar",
                    })?;
                snack.message = message;
                snack.visible = visible;
            }
            (ElementKind::Custom(old), ElementKind::Custom(new))
                if old == "icon" && new == "icon" =>
            {
                let name = next
                    .properties
                    .get("name")
                    .cloned()
                    .unwrap_or_else(|| "circle".into());
                let size = next
                    .properties
                    .get("size")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(24.0_f32);
                let icon = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeIcon>())
                    .ok_or(ElementError::NativeWidgetType { expected: "Icon" })?;
                icon.icon_id = IconId(name);
                icon.size = size;
            }
            (ElementKind::Custom(old), ElementKind::Custom(new))
                if old == "linear-progress" && new == "linear-progress" =>
            {
                let value = next
                    .properties
                    .get("value")
                    .and_then(|value| value.parse::<f32>().ok())
                    .unwrap_or(0.0)
                    .clamp(0.0, 1.0);
                let indeterminate = next
                    .properties
                    .get("indeterminate")
                    .map(|value| value == "true")
                    .unwrap_or(false);
                let progress = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeLinearProgress>())
                    .ok_or(ElementError::NativeWidgetType {
                        expected: "LinearProgress",
                    })?;
                progress.value = value;
                progress.indeterminate = indeterminate;
            }
            (ElementKind::Custom(old), ElementKind::Custom(new))
                if old == "circular-progress" && new == "circular-progress" =>
            {
                let value = next
                    .properties
                    .get("value")
                    .and_then(|value| value.parse::<f32>().ok())
                    .unwrap_or(0.0)
                    .clamp(0.0, 1.0);
                let indeterminate = next
                    .properties
                    .get("indeterminate")
                    .map(|value| value == "true")
                    .unwrap_or(true);
                let progress = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeCircularProgress>())
                    .ok_or(ElementError::NativeWidgetType {
                        expected: "CircularProgress",
                    })?;
                progress.value = value;
                progress.indeterminate = indeterminate;
            }
            (ElementKind::Custom(old), ElementKind::Custom(new))
                if old == "lottie" && new == "lottie" =>
            {
                if mounted.previous.properties.get("json") != next.properties.get("json")
                    || mounted.previous.properties.get("id") != next.properties.get("id")
                {
                    let replacement = Self::make_lottie(next)?;
                    let id = synui_core::Widget::id(&replacement);
                    self.store.replace(widget, replacement);
                    mounted.widget = Some(id);
                } else {
                    let width = next
                        .properties
                        .get("width")
                        .and_then(|value| value.parse().ok())
                        .unwrap_or(160.0);
                    let height = next
                        .properties
                        .get("height")
                        .and_then(|value| value.parse().ok())
                        .unwrap_or(160.0);
                    let playing = next
                        .properties
                        .get("playing")
                        .map(|value| value != "false")
                        .unwrap_or(true);
                    let lottie = self
                        .store
                        .get_mut(widget)
                        .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeLottie>())
                        .ok_or(ElementError::NativeWidgetType { expected: "Lottie" })?;
                    lottie.set_size(width, height);
                    lottie.set_playing(playing);
                }
            }
            (ElementKind::Custom(old), ElementKind::Custom(new))
                if old == "morphing-shape" && new == "morphing-shape" =>
            {
                let size = next
                    .properties
                    .get("size")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(120.0);
                let playing = next
                    .properties
                    .get("playing")
                    .map(|value| value != "false")
                    .unwrap_or(true);
                let speed = next
                    .properties
                    .get("speed")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0.45);
                let morph = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeMorphingShape>())
                    .ok_or(ElementError::NativeWidgetType {
                        expected: "MorphingShape",
                    })?;
                morph.set_size(size);
                morph.set_playing(playing);
                morph.set_speed(speed);
            }
            (ElementKind::Custom(old), ElementKind::Custom(new))
                if old == "label" && new == "label" =>
            {
                let text = Self::facade_label_display_text(next);
                let truncate = next
                    .properties
                    .get("truncate")
                    .map(|value| value == "true")
                    .unwrap_or(false);
                let wrap = next
                    .properties
                    .get("wrap")
                    .map(|value| value == "true")
                    .unwrap_or(false);
                let label = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<Label>())
                    .ok_or(ElementError::NativeWidgetType { expected: "Label" })?;
                label.truncate = truncate;
                label.wrap = wrap;
                label.set_text(text);
            }
            (ElementKind::Custom(old), ElementKind::Custom(new))
                if old == "image" && new == "image" =>
            {
                let replacement = Self::make_image(next);
                let id = synui_core::Widget::id(&replacement);
                self.store.replace(widget, replacement);
                mounted.widget = Some(id);
            }
            (ElementKind::Custom(old), ElementKind::Custom(new))
                if old == "menu-bar" && new == "menu-bar" =>
            {
                let menus = next
                    .menu_bar_menus
                    .as_deref()
                    .map(Self::facade_menus)
                    .unwrap_or_default();
                let bar = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeMenuBar>())
                    .ok_or(ElementError::NativeWidgetType {
                        expected: "MenuBar",
                    })?;
                bar.menus = menus;
            }
            (ElementKind::Custom(old), ElementKind::Custom(new))
                if old == "context-menu" && new == "context-menu" =>
            {
                let items = next
                    .context_items
                    .as_deref()
                    .unwrap_or(&[])
                    .iter()
                    .map(Self::facade_menu_item)
                    .collect::<Vec<_>>();
                let show_at = next.context_show_at;
                let menu = self
                    .store
                    .get_mut(widget)
                    .and_then(|widget| widget.as_any_mut().downcast_mut::<NativeContextMenu>())
                    .ok_or(ElementError::NativeWidgetType {
                        expected: "ContextMenu",
                    })?;
                menu.items = items;
                if let Some((x, y)) = show_at {
                    if mounted.observed_context_show != Some((x, y)) {
                        menu.show_at(Point::new(x, y));
                        mounted.observed_context_show = Some((x, y));
                    }
                }
            }
            (old, new) if Self::is_container(old) && Self::is_container(new) => {}
            _ => self.replace_native_widget(mounted, next, identity)?,
        }
        let current_widget = mounted.widget.ok_or(ElementError::NotMounted)?;
        if next.typed_input()?.is_some() {
            if next
                .typed_input()?
                .is_some_and(|input| input.mode == InputMode::Controlled)
            {
                mounted.observed_input = self.read_native_input(current_widget, next)?;
            }
        } else {
            mounted.observed_input = None;
        }
        self.sync_accessibility(current_widget, next, identity)?;
        Ok(())
    }

    fn is_container(kind: &ElementKind) -> bool {
        match kind {
            ElementKind::Container => true,
            ElementKind::Custom(name) => !matches!(
                name.as_str(),
                "button"
                    | "text-input"
                    | "text-area"
                    | "checkbox"
                    | "switch"
                    | "radio"
                    | "number-input"
                    | "select"
                    | "virtual-list"
                    | "data-table"
                    | "virtual-tree"
                    | "tab-bar"
                    | "rich-document"
                    | "bottom-navigation"
                    | "snack-bar"
                    | "icon"
                    | "linear-progress"
                    | "circular-progress"
                    | "lottie"
                    | "morphing-shape"
                    | "label"
                    | "image"
                    | "menu-bar"
                    | "context-menu"
                    | "legacy-flex-row"
                    | "legacy-flex-column"
            ),
            ElementKind::Text => false,
        }
    }

    fn replace_native_widget(
        &mut self,
        mounted: &mut Mounted,
        next: &ElementSpec,
        identity: &IdentityPath,
    ) -> Result<(), ElementError> {
        Self::validate_spec(next)?;
        let old = mounted.widget.ok_or(ElementError::NotMounted)?;
        let new_id = match &next.kind {
            ElementKind::Text => {
                let replacement =
                    Label::new(next.text.as_deref().ok_or(ElementError::MissingText)?);
                let id = synui_core::Widget::id(&replacement);
                self.store.replace(old, replacement);
                id
            }
            ElementKind::Custom(kind) if kind == "button" => {
                let replacement = self.make_button(next, identity);
                let id = synui_core::Widget::id(&replacement);
                self.store.replace(old, replacement);
                id
            }
            ElementKind::Custom(kind) if kind == "text-input" => {
                let input = next.typed_input()?.ok_or(ElementError::NativeWidgetType {
                    expected: "typed TextField spec",
                })?;
                let InputValue::Text(value) = &input.value else {
                    return Err(InputConfigError::UnsupportedValue {
                        kind: InputKind::Text,
                        value: input.value.clone(),
                    }
                    .into());
                };
                let mut replacement = NativeTextField::new(
                    next.properties
                        .get("placeholder")
                        .cloned()
                        .unwrap_or_default(),
                );
                replacement.password = Self::password_prop(next);
                replacement.edit.text = value.clone();
                replacement.notify_text_changed();
                let id = synui_core::Widget::id(&replacement);
                self.store.replace(old, replacement);
                id
            }
            ElementKind::Custom(kind) if kind == "text-area" => {
                let input = next.typed_input()?.ok_or(ElementError::NativeWidgetType {
                    expected: "typed TextArea spec",
                })?;
                let InputValue::Text(value) = &input.value else {
                    return Err(InputConfigError::UnsupportedValue {
                        kind: InputKind::Text,
                        value: input.value.clone(),
                    }
                    .into());
                };
                let mut replacement = NativeTextArea::new();
                replacement.edit.text = value.clone();
                replacement.notify_text_changed();
                Self::apply_text_area_chrome(&mut replacement, next);
                let id = synui_core::Widget::id(&replacement);
                self.store.replace(old, replacement);
                id
            }
            ElementKind::Custom(kind) if kind == "checkbox" => {
                let input = next.typed_input()?.ok_or(ElementError::NativeWidgetType {
                    expected: "typed Checkbox spec",
                })?;
                let InputValue::Bool(value) = &input.value else {
                    return Err(InputConfigError::UnsupportedValue {
                        kind: InputKind::Bool,
                        value: input.value.clone(),
                    }
                    .into());
                };
                let label = next.properties.get("label").cloned().unwrap_or_default();
                let mut replacement = NativeCheckbox::new(label);
                replacement.checked = *value;
                let id = synui_core::Widget::id(&replacement);
                self.store.replace(old, replacement);
                id
            }
            ElementKind::Custom(kind) if kind == "switch" => {
                let input = next.typed_input()?.ok_or(ElementError::NativeWidgetType {
                    expected: "typed Switch spec",
                })?;
                let InputValue::Bool(value) = &input.value else {
                    return Err(InputConfigError::UnsupportedValue {
                        kind: InputKind::Bool,
                        value: input.value.clone(),
                    }
                    .into());
                };
                let mut replacement = NativeSwitch::new();
                replacement.on = *value;
                let id = synui_core::Widget::id(&replacement);
                self.store.replace(old, replacement);
                id
            }
            ElementKind::Custom(kind) if kind == "radio" => {
                let input = next.typed_input()?.ok_or(ElementError::NativeWidgetType {
                    expected: "typed Radio spec",
                })?;
                let InputValue::Bool(value) = &input.value else {
                    return Err(InputConfigError::UnsupportedValue {
                        kind: InputKind::Bool,
                        value: input.value.clone(),
                    }
                    .into());
                };
                let label = next.properties.get("label").cloned().unwrap_or_default();
                let mut replacement = NativeRadio::new(label);
                replacement.selected = *value;
                let id = synui_core::Widget::id(&replacement);
                self.store.replace(old, replacement);
                id
            }
            ElementKind::Custom(kind) if kind == "number-input" => {
                let input = next.typed_input()?.ok_or(ElementError::NativeWidgetType {
                    expected: "typed Slider spec",
                })?;
                let InputValue::Number(value) = &input.value else {
                    return Err(InputConfigError::UnsupportedValue {
                        kind: InputKind::Number,
                        value: input.value.clone(),
                    }
                    .into());
                };
                let (min, max) = input.range.unwrap_or((0.0, 100.0));
                let mut replacement = NativeSlider::new(min as f32, max as f32);
                replacement.value = *value as f32;
                let id = synui_core::Widget::id(&replacement);
                self.store.replace(old, replacement);
                id
            }
            ElementKind::Custom(kind) if kind == "select" => {
                let input = next.typed_input()?.ok_or(ElementError::NativeWidgetType {
                    expected: "typed Dropdown spec",
                })?;
                let InputValue::Choice(value) = &input.value else {
                    return Err(InputConfigError::UnsupportedValue {
                        kind: InputKind::Choice,
                        value: input.value.clone(),
                    }
                    .into());
                };
                let mut replacement = NativeDropdown::new(input.choices.clone());
                replacement.selected = value.index();
                replacement.highlighted = value.index();
                let id = synui_core::Widget::id(&replacement);
                self.store.replace(old, replacement);
                id
            }
            ElementKind::Custom(kind) if Self::is_legacy_flex_kind(kind) => {
                let replacement = Self::make_legacy_flex(next)?;
                let id = synui_core::Widget::id(&replacement);
                self.store.replace(old, replacement);
                id
            }
            ElementKind::Custom(kind) if kind == "scroll" => {
                let replacement = Self::make_scroll(next);
                let id = synui_core::Widget::id(&replacement);
                self.store.replace(old, replacement);
                id
            }
            ElementKind::Custom(kind) if kind == "styled" => {
                let replacement = self.make_styled(next, identity)?;
                let id = synui_core::Widget::id(&replacement);
                self.store.replace(old, replacement);
                id
            }
            ElementKind::Custom(kind) if kind == "virtual-list" => {
                let replacement = Self::make_virtual_list(next);
                let id = synui_core::Widget::id(&replacement);
                self.store.replace(old, replacement);
                id
            }
            ElementKind::Custom(kind) if kind == "data-table" => {
                let replacement = Self::make_data_table(next);
                let id = synui_core::Widget::id(&replacement);
                self.store.replace(old, replacement);
                id
            }
            ElementKind::Custom(kind) if kind == "virtual-tree" => {
                let replacement = Self::make_virtual_tree(next);
                let id = synui_core::Widget::id(&replacement);
                self.store.replace(old, replacement);
                id
            }
            ElementKind::Custom(kind) if kind == "split-pane" => {
                let replacement = Self::make_split_pane(next);
                let id = synui_core::Widget::id(&replacement);
                self.store.replace(old, replacement);
                id
            }
            ElementKind::Custom(kind) if kind == "tab-bar" => {
                let replacement = Self::make_tab_bar(next);
                let id = synui_core::Widget::id(&replacement);
                self.store.replace(old, replacement);
                id
            }
            ElementKind::Custom(kind) if kind == "rich-document" => {
                let replacement = Self::make_rich_document(next);
                let id = synui_core::Widget::id(&replacement);
                self.store.replace(old, replacement);
                id
            }
            ElementKind::Custom(kind) if kind == "app-bar" => {
                let replacement = Self::make_app_bar(next);
                let id = synui_core::Widget::id(&replacement);
                self.store.replace(old, replacement);
                id
            }
            ElementKind::Custom(kind) if kind == "navigation-drawer" => {
                let replacement = Self::make_navigation_drawer(next);
                let id = synui_core::Widget::id(&replacement);
                self.store.replace(old, replacement);
                id
            }
            ElementKind::Custom(kind) if kind == "bottom-navigation" => {
                let replacement = Self::make_bottom_navigation(next);
                let id = synui_core::Widget::id(&replacement);
                self.store.replace(old, replacement);
                id
            }
            ElementKind::Custom(kind) if kind == "dialog" => {
                let replacement = Self::make_dialog(next);
                let id = synui_core::Widget::id(&replacement);
                self.store.replace(old, replacement);
                id
            }
            ElementKind::Custom(kind) if kind == "card" => {
                let replacement = Self::make_card(next);
                let id = synui_core::Widget::id(&replacement);
                self.store.replace(old, replacement);
                id
            }
            ElementKind::Custom(kind) if kind == "list-item" => {
                let replacement = self.make_list_item(next, identity);
                let id = synui_core::Widget::id(&replacement);
                self.store.replace(old, replacement);
                id
            }
            ElementKind::Custom(kind) if kind == "bottom-sheet" => {
                let replacement = Self::make_bottom_sheet(next);
                let id = synui_core::Widget::id(&replacement);
                self.store.replace(old, replacement);
                id
            }
            ElementKind::Custom(kind) if kind == "snack-bar" => {
                let replacement = Self::make_snack_bar(next);
                let id = synui_core::Widget::id(&replacement);
                self.store.replace(old, replacement);
                id
            }
            ElementKind::Custom(kind) if kind == "icon" => {
                let replacement = Self::make_icon(next);
                let id = synui_core::Widget::id(&replacement);
                self.store.replace(old, replacement);
                id
            }
            ElementKind::Custom(kind) if kind == "linear-progress" => {
                let replacement = Self::make_linear_progress(next);
                let id = synui_core::Widget::id(&replacement);
                self.store.replace(old, replacement);
                id
            }
            ElementKind::Custom(kind) if kind == "circular-progress" => {
                let replacement = Self::make_circular_progress(next);
                let id = synui_core::Widget::id(&replacement);
                self.store.replace(old, replacement);
                id
            }
            ElementKind::Custom(kind) if kind == "lottie" => {
                let replacement = Self::make_lottie(next)?;
                let id = synui_core::Widget::id(&replacement);
                self.store.replace(old, replacement);
                id
            }
            ElementKind::Custom(kind) if kind == "morphing-shape" => {
                let replacement = Self::make_morphing_shape(next);
                let id = synui_core::Widget::id(&replacement);
                self.store.replace(old, replacement);
                id
            }
            ElementKind::Custom(kind) if kind == "label" => {
                let replacement = Self::make_facade_label(next);
                let id = synui_core::Widget::id(&replacement);
                self.store.replace(old, replacement);
                id
            }
            ElementKind::Custom(kind) if kind == "image" => {
                let replacement = Self::make_image(next);
                let id = synui_core::Widget::id(&replacement);
                self.store.replace(old, replacement);
                id
            }
            ElementKind::Custom(kind) if kind == "menu-bar" => {
                let replacement = Self::make_menu_bar(next);
                let id = synui_core::Widget::id(&replacement);
                self.store.replace(old, replacement);
                id
            }
            ElementKind::Custom(kind) if kind == "context-menu" => {
                let replacement = Self::make_context_menu(next);
                let id = synui_core::Widget::id(&replacement);
                self.store.replace(old, replacement);
                id
            }
            ElementKind::Container => {
                let replacement = if next.layout.is_some() {
                    Self::make_container(next)
                } else {
                    Flex::new()
                };
                let id = synui_core::Widget::id(&replacement);
                self.store.replace(old, replacement);
                id
            }
            ElementKind::Custom(_) => {
                let replacement = Flex::new();
                let id = synui_core::Widget::id(&replacement);
                self.store.replace(old, replacement);
                id
            }
        };
        if self.root == Some(old) {
            self.root = Some(new_id);
        }
        mounted.widget = Some(new_id);
        mounted.observed_input = self.read_native_input(new_id, next)?;
        if next.context_show_at.is_some() {
            mounted.observed_context_show = next.context_show_at;
        }
        let child_ids = mounted
            .children
            .iter()
            .filter_map(|child| child.widget)
            .collect();
        self.store.set_children(new_id, child_ids);
        Ok(())
    }

    fn validate_accessibility_spec(&self, spec: &ElementSpec) -> Result<(), ElementError> {
        spec.accessibility.typed_role()?;
        if let Some(handler) = spec.accessibility_handler.as_ref() {
            let context = self
                .context
                .as_ref()
                .ok_or(ElementError::MissingAppContext)?;
            context.validate_action(handler)?;
        }
        Ok(())
    }

    fn sync_accessibility(
        &mut self,
        widget: WidgetId,
        spec: &ElementSpec,
        identity: &IdentityPath,
    ) -> Result<(), ElementError> {
        let semantic = &spec.accessibility;
        let accessibility_handler = spec.accessibility_handler.clone();
        if let Some(handler) = accessibility_handler.as_ref() {
            let context = self
                .context
                .as_ref()
                .ok_or(ElementError::MissingAppContext)?;
            context.validate_action(handler)?;
        }
        let declared_actions = semantic.actions.clone().unwrap_or_default();
        let mut actions = semantic.actions.as_ref().map(|actions| {
            actions
                .iter()
                .map(|action| match action {
                    AccessibilityAction::Click => A11yAction::Click,
                    AccessibilityAction::Focus => A11yAction::Focus,
                    AccessibilityAction::SetValue => A11yAction::SetValue,
                })
                .collect::<Vec<_>>()
        });
        let press_slot = spec
            .event_slots
            .iter()
            .find(|slot| slot.event == Event::Press)
            .map(|slot| slot.slot);
        let press_route = press_slot
            .map(|slot| ActionRouteKey {
                identity: identity.clone(),
                event: Event::Press,
                slot,
            })
            .filter(|route| self.action_routes.borrow().contains_key(route));
        if press_slot.is_some() {
            // Facade Press is executable as accessibility Click only while its
            // route is live. Keep an explicit empty override when it is absent
            // so the native Button's default Click metadata cannot outlive it.
            let actions = actions.get_or_insert_with(Vec::new);
            if press_route.is_some() && !actions.contains(&A11yAction::Click) {
                actions.push(A11yAction::Click);
            }
        }
        let focusable = actions
            .as_ref()
            .filter(|actions| actions.contains(&A11yAction::Focus))
            .map(|_| semantic.disabled != Some(true));
        let override_props = A11yOverride {
            role: semantic.typed_role()?.map(AccessibilityRole::engine_role),
            label: semantic.name.clone(),
            value: semantic.value.clone(),
            focusable,
            actions,
            disabled: semantic.disabled,
            checked: semantic.checked,
            selected: semantic.selected,
            expanded: semantic.expanded,
            read_only: semantic.read_only,
            required: semantic.required,
            invalid: semantic.invalid,
            row_count: semantic.row_count,
            col_count: semantic.column_count,
            row_index: semantic.row_index,
            col_index: semantic.column_index,
        };
        self.store.set_semantic_override(widget, override_props);

        self.store.clear_semantic_action_handler(widget);
        if press_route.is_some() || accessibility_handler.is_some() {
            let routes = Rc::clone(&self.action_routes);
            let context = self.context.clone();
            self.store
                .set_semantic_action_handler(widget, move |action, value| {
                    let facade_action = match action {
                        A11yAction::Click => AccessibilityAction::Click,
                        A11yAction::Focus => AccessibilityAction::Focus,
                        A11yAction::SetValue => AccessibilityAction::SetValue,
                    };
                    if declared_actions.contains(&facade_action) {
                        if let (Some(handler), Some(context)) =
                            (accessibility_handler.as_ref(), context.as_ref())
                        {
                            return if handler
                                .invoke(
                                    context,
                                    AccessibilityActionRequest::new(facade_action, value),
                                )
                                .is_ok()
                            {
                                EventResult::HANDLED
                            } else {
                                EventResult::IGNORED
                            };
                        }
                    }
                    if action == A11yAction::Click {
                        if let (Some(route), Some(context)) =
                            (press_route.as_ref(), context.as_ref())
                        {
                            // End the route-table borrow before invoking user code.
                            let routed = routes.borrow().get(route).map(RoutedAction::action);
                            if let Some(routed) = routed {
                                return if routed.invoke(context, ()).is_ok() {
                                    EventResult::HANDLED
                                } else {
                                    EventResult::IGNORED
                                };
                            }
                        }
                    }
                    EventResult::IGNORED
                });
        }
        Ok(())
    }

    fn sync_actions(
        &mut self,
        spec: &ElementSpec,
        identity: &IdentityPath,
    ) -> Result<(), ElementError> {
        let slots = spec
            .event_slots
            .iter()
            .filter(|slot| slot.event == Event::Press)
            .map(|slot| slot.slot)
            .collect::<Vec<_>>();
        let mut replacements = Vec::with_capacity(slots.len());

        if let Some(callback) = spec.press_callback.clone() {
            match callback {
                CallbackHolder::Closure(callback) => {
                    let context = self
                        .context
                        .as_ref()
                        .ok_or(ElementError::MissingAppContext)?;
                    for slot in slots {
                        let callback = callback.clone();
                        let action = match context.action(move |app, ()| {
                            callback.invoke(EventContext::new(app.clone()));
                        }) {
                            Ok(action) => action,
                            Err(error) => {
                                let generated = replacements.into_iter().map(|(_, action)| action);
                                let _ = self.release_generated_actions(generated);
                                return Err(error.into());
                            }
                        };
                        replacements.push((
                            ActionRouteKey {
                                identity: identity.clone(),
                                event: Event::Press,
                                slot,
                            },
                            RoutedAction::Generated(action),
                        ));
                    }
                }
                CallbackHolder::Borrowed(action) => {
                    let context = self
                        .context
                        .as_ref()
                        .ok_or(ElementError::MissingAppContext)?;
                    // Reject cross-Context or stale borrowed handles before the
                    // live route table or retained widget sidecars are changed.
                    context.validate_action(&action)?;
                    for slot in slots {
                        replacements.push((
                            ActionRouteKey {
                                identity: identity.clone(),
                                event: Event::Press,
                                slot,
                            },
                            RoutedAction::Borrowed(action.clone()),
                        ));
                    }
                }
            }
        }

        // Every generated replacement exists before the live table changes. The
        // single mutable borrow makes disabling old routes and publishing the new
        // routes atomic to native dispatch.
        let previous = {
            let mut routes = self.action_routes.borrow_mut();
            let keys = routes
                .keys()
                .filter(|key| key.identity == *identity)
                .cloned()
                .collect::<Vec<_>>();
            let previous = keys
                .into_iter()
                .filter_map(|key| routes.remove(&key))
                .collect::<Vec<_>>();
            routes.extend(replacements);
            previous
        };

        // Old generated runtime resources are released only after their routes
        // can no longer be selected. Borrowed Actions are deliberately retained.
        self.release_generated_actions(previous)
    }

    fn disable_action_subtree(&mut self, identity: &IdentityPath) -> Vec<RoutedAction> {
        let mut routes = self.action_routes.borrow_mut();
        let keys = routes
            .keys()
            .filter(|key| key.identity.is_descendant_of(identity))
            .cloned()
            .collect::<Vec<_>>();
        keys.into_iter()
            .filter_map(|key| routes.remove(&key))
            .collect()
    }

    fn release_generated_actions(
        &self,
        actions: impl IntoIterator<Item = RoutedAction>,
    ) -> Result<(), ElementError> {
        let mut first_error = None;
        for action in actions {
            if let RoutedAction::Generated(action) = action {
                let result = self
                    .context
                    .as_ref()
                    .ok_or(ElementError::MissingAppContext)
                    .and_then(|context| context.release_action(&action).map_err(Into::into));
                if first_error.is_none() {
                    first_error = result.err();
                }
            }
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    fn replace_spec(
        &mut self,
        mounted: &mut Mounted,
        next: &mut ElementSpec,
    ) -> Result<(), ElementError> {
        let identity = next
            .identity
            .clone()
            .ok_or(ElementError::IdentityUnavailable)?;
        if identity == mounted.identity {
            return self.reconcile_spec(mounted, next);
        }
        let was_root = mounted.widget == self.root;
        self.unmount_mounted(mounted)?;
        self.pending_identity = Some(identity);
        let replacement = self.mount_spec(next);
        self.pending_identity = None;
        *mounted = replacement?;
        if was_root {
            let widget = mounted.widget.ok_or(ElementError::NotMounted)?;
            self.store.set_root(widget);
            self.root = Some(widget);
        }
        Ok(())
    }

    fn unmount_mounted(&mut self, mounted: &mut Mounted) -> Result<(), ElementError> {
        let widget = mounted.widget.take().ok_or(ElementError::NotMounted)?;

        // Teardown is intentionally ordered: make every subtree route
        // unreachable, release only generated Actions, then detach the Engine
        // subtree. No user callback runs while either internal lock is held.
        let disabled = self.disable_action_subtree(&mounted.identity);
        #[cfg(test)]
        self.teardown_steps.push(TeardownStep::RoutesDisabled);
        let release_result = self.release_generated_actions(disabled);
        #[cfg(test)]
        self.teardown_steps
            .push(TeardownStep::GeneratedActionsReleased);
        self.store.remove(widget);
        #[cfg(test)]
        self.teardown_steps.push(TeardownStep::StoreRemoved);
        if self.root == Some(widget) {
            self.root = None;
        }
        mounted.children.clear();
        release_result
    }

    fn dispatch_action(
        &self,
        identity: &IdentityPath,
        event: Event,
        slot: u16,
    ) -> Result<(), crate::app::AppError> {
        // Clone the Action and end the RefCell borrow before entering runtime.
        let action = self
            .action_routes
            .borrow()
            .get(&ActionRouteKey {
                identity: identity.clone(),
                event,
                slot,
            })
            .map(RoutedAction::action);
        match (action, self.context.as_ref()) {
            (Some(action), Some(context)) => action.invoke(context, ()),
            _ => Ok(()),
        }
    }
}

/// Narrow retained-tree host used by versioned language bindings.
///
/// This type intentionally exposes no Engine store or widget layout. Language
/// bindings can only render facade [`Node`] trees and dispatch a declared
/// button press by root-child index.
#[doc(hidden)]
pub struct LanguageHost {
    adapter: EngineAdapter,
    root: Option<Node>,
    mounted: Option<Mounted>,
    previous: Option<ElementSpec>,
}

// The host is created and accessed only behind the ABI App UI-thread gate. It
// owns its complete Rc graph and never shares an internal reference externally.
unsafe impl Send for LanguageHost {}

impl LanguageHost {
    pub fn new(context: AppContext) -> Self {
        Self {
            adapter: EngineAdapter::with_context(context),
            root: None,
            mounted: None,
            previous: None,
        }
    }

    pub fn render(&mut self, mut root: Node) -> Result<(), ElementError> {
        if let (Some(mounted), Some(previous)) = (&mut self.mounted, &self.previous) {
            let next = self.adapter.update_root(&mut root, mounted, previous)?;
            self.previous = Some(next);
        } else {
            let (spec, mounted) = self.adapter.mount_root(&mut root)?;
            self.previous = Some(spec);
            self.mounted = Some(mounted);
        }
        self.root = Some(root);
        Ok(())
    }

    pub fn press_root_child(&self, index: usize) -> Result<(), crate::app::AppError> {
        let identity = self
            .mounted
            .as_ref()
            .and_then(|mounted| mounted.children.get(index))
            .map(|mounted| mounted.identity.clone());
        match identity {
            Some(identity) => self.adapter.dispatch_action(&identity, Event::Press, 0),
            None => Err(crate::app::AppError::StaleHandle),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    struct ExternalBadge;

    impl Element for ExternalBadge {
        fn describe(&self, _cx: &mut BuildCx) -> ElementSpec {
            ElementSpec::custom("external-badge", [Node::text("external")])
                .accessibility(Accessibility::new().role("status").name("External badge"))
                .on(Event::Press, 0)
        }
    }

    #[test]
    fn label_text_change_marks_layout_dirty_for_hug_remeasure() {
        let mut root = Node::text("Count: 6");
        let mut adapter = EngineAdapter::new();
        let (previous, mut mounted) = adapter.mount_root(&mut root).unwrap();
        adapter.store.clear_dirty();

        let mut next = Node::text("Count: 10");
        let _ = adapter
            .update_root(&mut next, &mut mounted, &previous)
            .unwrap();
        assert!(
            adapter.store.needs_layout(),
            "hugging label text change must remasure so damage erase covers old∪new glyphs"
        );
        let dirty = adapter.store.take_dirty_layout_widgets();
        assert!(
            !dirty.is_empty() || adapter.store.layout_is_full(),
            "label id should be in the partial layout dirty set"
        );
    }

    #[test]
    fn facade_label_length_change_marks_layout_dirty() {
        use crate::widgets::Label;

        let mut root = Label::new("Count: 6").node();
        let mut adapter = EngineAdapter::new();
        let (previous, mut mounted) = adapter.mount_root(&mut root).unwrap();
        adapter.store.clear_dirty();

        let mut next = Label::new("Count: 10").node();
        let _ = adapter
            .update_root(&mut next, &mut mounted, &previous)
            .unwrap();
        assert!(
            adapter.store.needs_layout(),
            "facade Label length change must remasure (ghost fix)"
        );
    }

    #[test]
    fn facade_label_same_length_text_stays_paint_only() {
        use crate::widgets::Label;

        let mut root = Label::new("Volume: 40").node();
        let mut adapter = EngineAdapter::new();
        let (previous, mut mounted) = adapter.mount_root(&mut root).unwrap();
        adapter.store.clear_dirty();

        let mut next = Label::new("Volume: 41").node();
        let _ = adapter
            .update_root(&mut next, &mut mounted, &previous)
            .unwrap();
        assert!(
            !adapter.store.needs_layout(),
            "same-length facade Label must not force Taffy (Volume drag)"
        );
        assert!(
            adapter.store.needs_paint(),
            "same-length facade Label still needs paint"
        );
    }

    #[test]
    fn every_builtin_and_external_spec_gets_semantics_and_large_grids_stay_constant_size() {
        use crate::widgets::{Button, Checkbox, DataTable, NumberInput, Select, TextInput};

        let mut root = Node::container([
            Node::text("built-in"),
            Button::new("button").node(),
            TextInput::new("placeholder").node(),
            Checkbox::new(true).node(),
            NumberInput::new(7.0).node(),
            Select::new(["a", "b"], 0).node(),
            DataTable::new()
                .logical_dimensions(100_000, 32)
                .mounted_position(512, 4)
                .node(),
            Node::external(ExternalBadge),
        ]);
        let mut adapter = EngineAdapter::new();
        let (_, mut mounted) = adapter.mount_root(&mut root).unwrap();

        fn assert_sidecars(adapter: &EngineAdapter, mounted: &Mounted) {
            let widget = mounted.widget.unwrap();
            assert!(adapter.store.semantic_override(widget).is_some());
            for child in &mounted.children {
                assert_sidecars(adapter, child);
            }
        }
        assert_sidecars(&adapter, &mounted);
        assert_eq!(adapter.retained_widget_count(), 10);

        let table = &mounted.children[6];
        assert!(
            table.children.is_empty(),
            "100k logical rows must not expand Elements"
        );
        let table_props = adapter.store.effective_a11y(table.widget.unwrap()).unwrap();
        assert_eq!(table_props.role, A11yRole::Grid);
        assert_eq!(table_props.row_count, Some(100_000));
        assert_eq!(table_props.col_count, Some(32));
        assert_eq!(table_props.row_index, Some(512));
        assert_eq!(table_props.col_index, Some(4));

        adapter.unmount_root(&mut root, &mut mounted);
        assert_eq!(adapter.retained_widget_count(), 0);
    }

    #[test]
    fn typed_accessibility_and_compatible_role_strings_reach_effective_semantics() {
        let accessibility = Accessibility::new()
            .role(AccessibilityRole::Checkbox)
            .name("Remember me")
            .value("yes")
            .state(AccessibilityState::Disabled(true))
            .state(AccessibilityState::Checked(true))
            .state(AccessibilityState::Selected(false))
            .state(AccessibilityState::Expanded(true))
            .state(AccessibilityState::ReadOnly(true))
            .state(AccessibilityState::Required(true))
            .state(AccessibilityState::Invalid(true))
            .action(AccessibilityAction::Focus)
            .logical_grid(100_000, 8)
            .mounted_position(10, 2);
        let mut root = Node::from_spec(
            ElementSpec::custom("external-semantic", []).accessibility(accessibility),
        );
        let mut adapter = EngineAdapter::new();
        let (_, mut mounted) = adapter.mount_root(&mut root).unwrap();
        let props = adapter
            .store
            .effective_a11y(mounted.widget.unwrap())
            .unwrap();
        assert_eq!(props.role, A11yRole::Checkbox);
        assert_eq!(props.label.as_deref(), Some("Remember me"));
        assert_eq!(props.value.as_deref(), Some("yes"));
        assert!(props.disabled && props.checked == Some(true));
        assert_eq!(props.selected, Some(false));
        assert_eq!(props.expanded, Some(true));
        assert!(props.read_only && props.required && props.invalid);
        assert_eq!(props.actions, vec![A11yAction::Focus]);
        assert_eq!((props.row_count, props.col_count), (Some(100_000), Some(8)));
        assert_eq!((props.row_index, props.col_index), (Some(10), Some(2)));

        assert_eq!(
            "table".parse::<AccessibilityRole>().unwrap(),
            AccessibilityRole::Grid
        );
        assert!("not-a-role".parse::<AccessibilityRole>().is_err());
        adapter.unmount_root(&mut root, &mut mounted);
    }

    // Feature: synui-api-abi-reset, Property 13: 无障碍语义端到端保真
    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 100,
            max_shrink_iters: 256,
            .. ProptestConfig::default()
        })]
        #[test]
        fn accessibility_semantics_and_virtual_grid_metadata_survive_adapter(
            role_index in 0u8..17,
            action_mask in 0u8..8,
            disabled in any::<bool>(),
            checked in any::<bool>(),
            selected in any::<bool>(),
            expanded in any::<bool>(),
            read_only in any::<bool>(),
            required in any::<bool>(),
            invalid in any::<bool>(),
            row_count in 0usize..=100_000,
            column_count in 0usize..=128,
            row_index in 0usize..=100_000,
            column_index in 0usize..=128,
        ) {
            let role = match role_index {
                0 => AccessibilityRole::Generic,
                1 => AccessibilityRole::Window,
                2 => AccessibilityRole::Button,
                3 => AccessibilityRole::Label,
                4 => AccessibilityRole::TextInput,
                5 => AccessibilityRole::Checkbox,
                6 => AccessibilityRole::Switch,
                7 => AccessibilityRole::Slider,
                8 => AccessibilityRole::ProgressBar,
                9 => AccessibilityRole::ListItem,
                10 => AccessibilityRole::List,
                11 => AccessibilityRole::Grid,
                12 => AccessibilityRole::Row,
                13 => AccessibilityRole::GridCell,
                14 => AccessibilityRole::Tab,
                15 => AccessibilityRole::Dialog,
                _ => AccessibilityRole::Status,
            };
            let name = format!("name-{role_index}");
            let value = format!("value-{action_mask}");
            let mut accessibility = Accessibility::new()
                .role(role)
                .name(name.clone())
                .value(value.clone())
                .state(AccessibilityState::Disabled(disabled))
                .state(AccessibilityState::Checked(checked))
                .state(AccessibilityState::Selected(selected))
                .state(AccessibilityState::Expanded(expanded))
                .state(AccessibilityState::ReadOnly(read_only))
                .state(AccessibilityState::Required(required))
                .state(AccessibilityState::Invalid(invalid))
                .logical_grid(row_count, column_count)
                .mounted_position(row_index, column_index);
            if action_mask & 1 != 0 {
                accessibility = accessibility.action(AccessibilityAction::Click);
            }
            if action_mask & 2 != 0 {
                accessibility = accessibility.action(AccessibilityAction::Focus);
            }
            if action_mask & 4 != 0 {
                accessibility = accessibility.action(AccessibilityAction::SetValue);
            }

            let mut root = Node::from_spec(
                ElementSpec::custom("external-semantic", []).accessibility(accessibility),
            );
            let mut adapter = EngineAdapter::new();
            let (_, mut mounted) = adapter.mount_root(&mut root).unwrap();
            let widget = mounted.widget.unwrap();
            let props = adapter.store.effective_a11y(widget).unwrap();
            let expected_actions = [
                (1, A11yAction::Click),
                (2, A11yAction::Focus),
                (4, A11yAction::SetValue),
            ]
            .into_iter()
            .filter_map(|(bit, action)| (action_mask & bit != 0).then_some(action))
            .collect::<Vec<_>>();

            prop_assert_eq!(props.role, role.engine_role());
            prop_assert_eq!(props.label.as_deref(), Some(name.as_str()));
            prop_assert_eq!(props.value.as_deref(), Some(value.as_str()));
            prop_assert_eq!(props.disabled, disabled);
            prop_assert_eq!(props.checked, Some(checked));
            prop_assert_eq!(props.selected, Some(selected));
            prop_assert_eq!(props.expanded, Some(expanded));
            prop_assert_eq!(props.read_only, read_only);
            prop_assert_eq!(props.required, required);
            prop_assert_eq!(props.invalid, invalid);
            prop_assert_eq!(props.actions, expected_actions);
            prop_assert_eq!(
                props.focusable,
                action_mask & 2 != 0 && !disabled
            );
            prop_assert_eq!(props.row_count, Some(row_count));
            prop_assert_eq!(props.col_count, Some(column_count));
            prop_assert_eq!(props.row_index, Some(row_index));
            prop_assert_eq!(props.col_index, Some(column_index));
            prop_assert!(mounted.children.is_empty());
            prop_assert_eq!(adapter.retained_widget_count(), 1);

            adapter.unmount_root(&mut root, &mut mounted);
            prop_assert_eq!(adapter.retained_widget_count(), 0);
        }
    }

    #[test]
    fn accessibility_click_metadata_tracks_press_route_and_unmount() {
        use crate::app::{App, AppConfig};
        use crate::widgets::Button;
        use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
        use std::sync::Arc;

        let app = App::new(AppConfig::default()).unwrap();
        let context = app.context();
        let calls = Arc::new(AtomicUsize::new(0));
        let action = context
            .action({
                let calls = Arc::clone(&calls);
                move |_, ()| {
                    calls.fetch_add(1, AtomicOrdering::SeqCst);
                }
            })
            .unwrap();
        let mut root = Button::new("run").on_press(action).node();
        let mut adapter = EngineAdapter::with_context(context);
        let (previous, mut mounted) = adapter.mount_root(&mut root).unwrap();
        let widget = mounted.widget.unwrap();
        assert_eq!(
            adapter
                .store
                .semantic_override(widget)
                .unwrap()
                .actions
                .as_deref(),
            Some(&[A11yAction::Click][..])
        );
        assert_eq!(
            adapter
                .store
                .dispatch_a11y_action(widget, A11yAction::Click, None),
            EventResult::HANDLED
        );
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 1);

        let mut without_route = Button::new("run disabled").node();
        adapter
            .update_root(&mut without_route, &mut mounted, &previous)
            .unwrap();
        assert_eq!(
            adapter
                .store
                .semantic_override(widget)
                .unwrap()
                .actions
                .as_deref(),
            Some(&[][..]),
            "removing the route must suppress native Click metadata"
        );
        let _ = adapter
            .store
            .dispatch_a11y_action(widget, A11yAction::Click, None);
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 1);

        adapter.unmount_root(&mut without_route, &mut mounted);
        assert!(adapter.store.semantic_override(widget).is_none());
        assert_eq!(adapter.retained_widget_count(), 0);
    }

    #[test]
    fn built_in_and_boxed_external_elements_share_the_retained_tree() {
        let mut root = Node::container([
            Node::text("built-in"),
            Node::from(Box::new(ExternalBadge) as Box<dyn Element>),
        ]);
        let mut adapter = EngineAdapter::new();
        let (spec, mut mounted) = adapter.mount_root(&mut root).unwrap();

        assert_eq!(adapter.retained_widget_count(), 4);
        let next = adapter.update_root(&mut root, &mut mounted, &spec).unwrap();
        assert_eq!(next.children().len(), 2);
        assert_eq!(adapter.retained_widget_count(), 4);

        adapter.unmount_root(&mut root, &mut mounted);
        assert!(!mounted.is_mounted());
        assert_eq!(adapter.retained_widget_count(), 0);
    }

    #[test]
    fn duplicate_business_keys_fail_before_mounting_widgets() {
        let mut root = Node::container([
            Node::text("first").key(Key::new("same")),
            Node::from(Box::new(ExternalBadge) as Box<dyn Element>).key(Key::new("same")),
        ]);
        let mut adapter = EngineAdapter::new();

        let error = adapter.mount_root(&mut root).unwrap_err();
        assert!(matches!(
            error,
            ElementError::DuplicateBusinessKey { ref key, .. } if key.as_str() == "same"
        ));
        assert_eq!(adapter.retained_widget_count(), 0);
    }

    struct PropertyExternalButton {
        label: String,
        slot: u16,
        action: Action<()>,
    }

    impl Element for PropertyExternalButton {
        fn describe(&self, _cx: &mut BuildCx) -> ElementSpec {
            ElementSpec::custom("button", [])
                .property("label", self.label.clone())
                .on(Event::Press, self.slot)
                .on_press(self.action.clone())
        }
    }

    // Feature: synui-api-abi-reset, Property 2: Element action 生命周期与稳定更新
    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 100,
            max_shrink_iters: 256,
            .. ProptestConfig::default()
        })]
        #[test]
        fn element_actions_replace_atomically_and_become_inactive_after_unmount(
            initial_external in any::<bool>(),
            initial_slot in 0u16..=3,
            updates in prop::collection::vec((any::<bool>(), 0u16..=3), 1..=8),
            unmount_at in 0usize..=8,
        ) {
            use crate::app::{App, AppConfig};
            use std::sync::{
                atomic::{AtomicUsize, Ordering as AtomicOrdering},
                Arc,
            };

            let app = App::new(AppConfig::default()).expect("App creation must succeed");
            let context = app.context();
            let key = Key::new("property-action");
            let mut call_counts = Vec::<Arc<AtomicUsize>>::new();
            let action_context = context.clone();
            let make_action = move |calls: Arc<AtomicUsize>| {
                action_context
                    .action(move |_, ()| {
                        calls.fetch_add(1, AtomicOrdering::SeqCst);
                    })
                    .expect("active Context must accept Action")
            };
            let build_root = |external: bool, slot: u16, revision: usize, action: Action<()>| {
                if external {
                    Node::external(PropertyExternalButton {
                        label: format!("external-{revision}"),
                        slot,
                        action,
                    })
                    .key(key.clone())
                } else {
                    Node::from_spec(
                        ElementSpec::custom("button", [])
                            .property("label", format!("built-in-{revision}"))
                            .on(Event::Press, slot)
                            .on_press(action),
                    )
                    .key(key.clone())
                }
            };

            let initial_calls = Arc::new(AtomicUsize::new(0));
            call_counts.push(initial_calls.clone());
            let initial_action = make_action(initial_calls);
            let mut root = build_root(initial_external, initial_slot, 0, initial_action);
            let mut adapter = EngineAdapter::with_context(context);
            let (mut previous, mut mounted) = adapter
                .mount_root(&mut root)
                .expect("initial Element must mount");
            let identity = mounted.identity.clone();
            adapter
                .dispatch_action(&identity, Event::Press, initial_slot)
                .expect("initial route must dispatch");
            prop_assert_eq!(call_counts[0].load(AtomicOrdering::SeqCst), 1);

            let stop_at = unmount_at.min(updates.len());
            let mut current_slot = initial_slot;
            for (revision, &(external, slot)) in updates.iter().enumerate() {
                if revision == stop_at {
                    adapter.unmount_root(&mut root, &mut mounted);
                    prop_assert!(!mounted.is_mounted());
                    adapter
                        .dispatch_action(&identity, Event::Press, current_slot)
                        .expect("dispatch after unmount must be harmless");
                    prop_assert_eq!(
                        call_counts.last().unwrap().load(AtomicOrdering::SeqCst),
                        1
                    );
                    break;
                }

                let calls = Arc::new(AtomicUsize::new(0));
                call_counts.push(calls.clone());
                let action = make_action(calls);
                let mut updated = build_root(external, slot, revision + 1, action);
                previous = adapter
                    .update_root(&mut updated, &mut mounted, &previous)
                    .expect("same identity update must succeed");
                root = updated;
                current_slot = slot;
                adapter
                    .dispatch_action(&identity, Event::Press, current_slot)
                    .expect("current route must dispatch");
                prop_assert_eq!(
                    call_counts.last().unwrap().load(AtomicOrdering::SeqCst),
                    1
                );
                for calls in &call_counts[..call_counts.len() - 1] {
                    prop_assert_eq!(calls.load(AtomicOrdering::SeqCst), 1);
                }
            }

            if mounted.is_mounted() {
                adapter.unmount_root(&mut root, &mut mounted);
                prop_assert!(!mounted.is_mounted());
                adapter
                    .dispatch_action(&identity, Event::Press, current_slot)
                    .expect("dispatch after final unmount must be harmless");
                prop_assert_eq!(
                    call_counts.last().unwrap().load(AtomicOrdering::SeqCst),
                    1
                );
            }
            prop_assert_eq!(adapter.retained_widget_count(), 0);
        }
    }

    #[test]
    fn keyed_reorder_preserves_identity_and_retained_widgets() {
        let mut root = Node::container([
            Node::text("A").key(Key::new("a")),
            Node::text("B").key(Key::new("b")),
        ]);
        let mut adapter = EngineAdapter::new();
        let (previous, mut mounted) = adapter.mount_root(&mut root).unwrap();
        let before = mounted
            .children
            .iter()
            .map(|child| {
                (
                    child.previous.key_value().unwrap().as_str().to_owned(),
                    (child.identity.clone(), child.widget.unwrap()),
                )
            })
            .collect::<HashMap<_, _>>();

        let mut reordered = Node::container([
            Node::text("B updated").key(Key::new("b")),
            Node::text("A updated").key(Key::new("a")),
        ]);
        adapter
            .update_root(&mut reordered, &mut mounted, &previous)
            .unwrap();

        assert_eq!(
            mounted.children[0].previous.text_content(),
            Some("B updated")
        );
        assert_eq!(
            mounted.children[1].previous.text_content(),
            Some("A updated")
        );
        for child in &mounted.children {
            let key = child.previous.key_value().unwrap().as_str();
            let (identity, widget) = &before[key];
            assert_eq!(&child.identity, identity);
            assert_eq!(child.widget, Some(*widget));
        }
    }

    // Feature: synui-api-abi-reset, Property 3: 自动 identity 与业务键重排序保持关联
    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 100,
            max_shrink_iters: 256,
            .. ProptestConfig::default()
        })]
        #[test]
        fn keyed_reordering_preserves_identity_widgets_and_action_state_associations(
            ranks in prop::collection::vec(any::<u64>(), 1..=8),
        ) {
            use crate::app::{App, AppConfig};
            use crate::widgets::Button;

            let app = App::new(AppConfig::default()).expect("App creation must succeed");
            let context = app.context();
            let resources = ranks
                .iter()
                .enumerate()
                .map(|(key_index, _)| {
                    let key = Key::unsigned(key_index as u64);
                    let state = context
                        .state(0_u64)
                        .expect("active Context must accept State");
                    let action = context
                        .action({
                            let state = state.clone();
                            move |_, ()| {
                                let current = state.get().expect("action State must remain live");
                                state
                                    .set(current + 1)
                                    .expect("action State must remain writable");
                            }
                        })
                        .expect("active Context must accept Action");
                    (key, state, action)
                })
                .collect::<Vec<_>>();

            let mut order = (0..resources.len()).collect::<Vec<_>>();
            order.sort_by_key(|index| (ranks[*index], *index));

            let build_root = |order: &[usize], revision: &str| {
                let keyed_children = order
                    .iter()
                    .map(|&key_index| {
                        Button::new(format!("{revision}-{}", key_index))
                            .on_press(resources[key_index].2.clone())
                            .node()
                            .key(resources[key_index].0.clone())
                    })
                    .collect::<Vec<_>>();
                Node::container([
                    Node::text("static header"),
                    Node::container(keyed_children),
                    Node::text("static footer"),
                ])
            };

            let mut initial = build_root(&(0..resources.len()).collect::<Vec<_>>(), "initial");
            let mut adapter = EngineAdapter::with_context(context);
            let (previous, mut mounted) = adapter
                .mount_root(&mut initial)
                .expect("unique keyed siblings must mount");

            let static_identities = [
                mounted.children[0].identity.clone(),
                mounted.children[1].identity.clone(),
                mounted.children[2].identity.clone(),
            ];
            let before = mounted.children[1]
                .children
                .iter()
                .map(|child| {
                    (
                        child.previous.key_value().unwrap().clone(),
                        (child.identity.clone(), child.widget.unwrap()),
                    )
                })
                .collect::<HashMap<_, _>>();
            prop_assert_eq!(before.len(), resources.len());

            let mut reordered = build_root(&order, "updated");
            adapter
                .update_root(&mut reordered, &mut mounted, &previous)
                .expect("keyed reorder must update");

            prop_assert_eq!(
                [
                    mounted.children[0].identity.clone(),
                    mounted.children[1].identity.clone(),
                    mounted.children[2].identity.clone(),
                ],
                static_identities,
                "static tree identities must remain stable"
            );

            let keyed_mounted = &mounted.children[1].children;
            let identities = keyed_mounted
                .iter()
                .map(|child| child.identity.clone())
                .collect::<Vec<_>>();
            prop_assert_eq!(
                identities.len(),
                identities.iter().collect::<HashSet<_>>().len(),
                "keyed sibling identities must be unique"
            );

            for child in keyed_mounted {
                let key = child.previous.key_value().unwrap();
                let (identity, widget) = &before[key];
                prop_assert_eq!(&child.identity, identity);
                prop_assert_eq!(child.widget, Some(*widget));
                let key_index = key.as_str().strip_prefix("u:").unwrap();
                let expected_label = format!("updated-{key_index}");
                prop_assert_eq!(
                    child.previous.properties().get("label").map(String::as_str),
                    Some(expected_label.as_str())
                );

                adapter
                    .dispatch_action(&child.identity, Event::Press, 0)
                    .expect("live route must dispatch after reorder");
            }

            for (_, state, _) in &resources {
                prop_assert_eq!(state.get().expect("State must remain live"), 1);
            }
        }
    }

    #[test]
    fn same_identity_update_atomically_replaces_the_action_route() {
        use crate::app::{App, AppConfig};
        use crate::widgets::Button;
        use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
        use std::sync::Arc;

        let app = App::new(AppConfig::default()).unwrap();
        let context = app.context();
        let first_calls = Arc::new(AtomicUsize::new(0));
        let second_calls = Arc::new(AtomicUsize::new(0));
        let first = context
            .action({
                let calls = Arc::clone(&first_calls);
                move |_, ()| {
                    calls.fetch_add(1, AtomicOrdering::SeqCst);
                }
            })
            .unwrap();
        let second = context
            .action({
                let calls = Arc::clone(&second_calls);
                move |_, ()| {
                    calls.fetch_add(1, AtomicOrdering::SeqCst);
                }
            })
            .unwrap();

        let mut root = Button::new("run").on_press(first).node();
        let mut adapter = EngineAdapter::with_context(context);
        let (previous, mut mounted) = adapter.mount_root(&mut root).unwrap();
        let identity = mounted.identity.clone();
        adapter.dispatch_action(&identity, Event::Press, 0).unwrap();

        let mut updated = Button::new("run updated").on_press(second).node();
        adapter
            .update_root(&mut updated, &mut mounted, &previous)
            .unwrap();
        adapter.dispatch_action(&identity, Event::Press, 0).unwrap();

        assert_eq!(first_calls.load(AtomicOrdering::SeqCst), 1);
        assert_eq!(second_calls.load(AtomicOrdering::SeqCst), 1);
    }

    #[test]
    fn closure_routes_invoke_replace_without_growth_and_teardown_in_order() {
        use crate::app::{App, AppConfig};
        use crate::widgets::Button;
        use std::sync::{Arc, Mutex};

        let app = App::new(AppConfig::default()).unwrap();
        let context = app.context();
        let context_id = context.id();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let mut root = Button::new("run")
            .on_click({
                let calls = Arc::clone(&calls);
                move |event: EventContext| {
                    assert_eq!(event.app().id(), context_id);
                    calls.lock().unwrap().push(0);
                }
            })
            .node();
        let mut adapter = EngineAdapter::with_context(context.clone());
        let (mut previous, mut mounted) = adapter.mount_root(&mut root).unwrap();
        let identity = mounted.identity.clone();
        assert_eq!(context.resource_count(), 1);
        adapter.dispatch_action(&identity, Event::Press, 0).unwrap();

        for version in 1..=8 {
            let calls = Arc::clone(&calls);
            let mut updated = Button::new(format!("run {version}"))
                .on_click(move |app: AppContext| {
                    assert_eq!(app.id(), context_id);
                    calls.lock().unwrap().push(version);
                })
                .node();
            previous = adapter
                .update_root(&mut updated, &mut mounted, &previous)
                .unwrap();
            assert_eq!(context.resource_count(), 1);
            adapter.dispatch_action(&identity, Event::Press, 0).unwrap();
        }

        adapter.unmount_root(&mut root, &mut mounted);
        assert_eq!(context.resource_count(), 0);
        adapter.dispatch_action(&identity, Event::Press, 0).unwrap();
        assert_eq!(*calls.lock().unwrap(), (0..=8).collect::<Vec<_>>());
        assert_eq!(
            &adapter.teardown_steps[adapter.teardown_steps.len() - 3..],
            &[
                TeardownStep::RoutesDisabled,
                TeardownStep::GeneratedActionsReleased,
                TeardownStep::StoreRemoved,
            ]
        );
    }

    #[test]
    fn borrowed_action_survives_route_unmount_but_the_route_no_longer_hits() {
        use crate::app::{App, AppConfig};
        use crate::widgets::Button;
        use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
        use std::sync::Arc;

        let app = App::new(AppConfig::default()).unwrap();
        let context = app.context();
        let calls = Arc::new(AtomicUsize::new(0));
        let action = context
            .action({
                let calls = Arc::clone(&calls);
                move |_, ()| {
                    calls.fetch_add(1, AtomicOrdering::SeqCst);
                }
            })
            .unwrap();
        let mut root = Button::new("borrowed").on_press(action.clone()).node();
        let mut adapter = EngineAdapter::with_context(context.clone());
        let (_, mut mounted) = adapter.mount_root(&mut root).unwrap();
        let identity = mounted.identity.clone();
        adapter.dispatch_action(&identity, Event::Press, 0).unwrap();

        adapter.unmount_root(&mut root, &mut mounted);
        assert_eq!(context.resource_count(), 1);
        adapter.dispatch_action(&identity, Event::Press, 0).unwrap();
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 1);
        action.invoke(&context, ()).unwrap();
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 2);
    }

    #[test]
    fn typed_inputs_mount_the_four_native_widgets() {
        use crate::widgets::{Checkbox, NumberInput, Select, TextInput};

        let mut root = Node::container([
            TextInput::new("name").uncontrolled("Ada", None).node(),
            Checkbox::new(true).node(),
            NumberInput::new(25.0).range(0.0, 50.0).node(),
            Select::new(["red", "blue"], 1).node(),
        ]);
        let mut adapter = EngineAdapter::new();
        let (_, mounted) = adapter.mount_root(&mut root).unwrap();

        assert_eq!(adapter.retained_widget_count(), 5);
        let ids = mounted
            .children
            .iter()
            .map(|child| child.widget.unwrap())
            .collect::<Vec<_>>();
        let text = adapter.store.get(ids[0]).unwrap();
        assert_eq!(
            text.as_any()
                .downcast_ref::<NativeTextField>()
                .unwrap()
                .edit
                .text,
            "Ada"
        );
        assert!(
            adapter
                .store
                .get(ids[1])
                .unwrap()
                .as_any()
                .downcast_ref::<NativeCheckbox>()
                .unwrap()
                .checked
        );
        assert_eq!(
            adapter
                .store
                .get(ids[2])
                .unwrap()
                .as_any()
                .downcast_ref::<NativeSlider>()
                .unwrap()
                .value,
            25.0
        );
        assert_eq!(
            adapter
                .store
                .get(ids[3])
                .unwrap()
                .as_any()
                .downcast_ref::<NativeDropdown>()
                .unwrap()
                .selected,
            1
        );
    }

    // Feature: synui-api-abi-reset, Property 4: 受控输入的渲染—变更回路
    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 100,
            max_shrink_iters: 256,
            .. ProptestConfig::default()
        })]
        #[test]
        fn controlled_inputs_round_trip_typed_changes_before_rendering_back(
            kind in 0u8..4,
            text_initial in prop::collection::vec(0u8..=25, 0..=8),
            text_edit in prop::collection::vec(0u8..=25, 0..=8),
            text_render in prop::collection::vec(0u8..=25, 0..=8),
            bool_initial in any::<bool>(),
            bool_edit in any::<bool>(),
            bool_render in any::<bool>(),
            number_initial in 0u8..=100,
            number_edit in 0u8..=100,
            number_render in 0u8..=100,
            choice_initial in 0usize..4,
            choice_edit in 0usize..4,
            choice_render in 0usize..4,
        ) {
            use crate::app::{App, AppConfig};
            use crate::widgets::{Checkbox, NumberInput, Select, TextInput};

            let app = App::new(AppConfig::default()).expect("App creation must succeed");
            let context = app.context();
            let to_text = |values: &[u8]| {
                values
                    .iter()
                    .map(|value| char::from(b'a' + *value))
                    .collect::<String>()
            };

            match kind {
                0 => {
                    let initial = to_text(&text_initial);
                    let edited = to_text(&text_edit);
                    let rendered = to_text(&text_render);
                    let state = context.state(initial).unwrap();
                    let action = context
                        .action({
                            let state = state.clone();
                            move |_, value: String| state.set(value).unwrap()
                        })
                        .unwrap();
                    let mut root = TextInput::new("text")
                        .controlled(state.clone(), action.clone())
                        .node();
                    let mut adapter = EngineAdapter::with_context(context);
                    let (previous, mut mounted) = adapter.mount_root(&mut root).unwrap();
                    let widget = mounted.widget.unwrap();
                    adapter
                        .store
                        .get_mut(widget)
                        .unwrap()
                        .as_any_mut()
                        .downcast_mut::<NativeTextField>()
                        .unwrap()
                        .edit
                        .text = edited.clone();
                    adapter.poll_input_changes(&mut mounted).unwrap();
                    prop_assert_eq!(state.get().unwrap(), edited);

                    state.set(rendered.clone()).unwrap();
                    let mut next = TextInput::new("text")
                        .controlled(state.clone(), action)
                        .node();
                    adapter
                        .update_root(&mut next, &mut mounted, &previous)
                        .unwrap();
                    prop_assert_eq!(
                        adapter
                            .store
                            .get(widget)
                            .unwrap()
                            .as_any()
                            .downcast_ref::<NativeTextField>()
                            .unwrap()
                            .edit
                            .text
                            .as_str(),
                        rendered.as_str()
                    );
                }
                1 => {
                    let state = context.state(bool_initial).unwrap();
                    let action = context
                        .action({
                            let state = state.clone();
                            move |_, value: bool| state.set(value).unwrap()
                        })
                        .unwrap();
                    let mut root = Checkbox::new(bool_initial)
                        .controlled(state.clone(), action.clone())
                        .node();
                    let mut adapter = EngineAdapter::with_context(context);
                    let (previous, mut mounted) = adapter.mount_root(&mut root).unwrap();
                    let widget = mounted.widget.unwrap();
                    adapter
                        .store
                        .get_mut(widget)
                        .unwrap()
                        .as_any_mut()
                        .downcast_mut::<NativeCheckbox>()
                        .unwrap()
                        .checked = bool_edit;
                    adapter.poll_input_changes(&mut mounted).unwrap();
                    prop_assert_eq!(state.get().unwrap(), bool_edit);

                    state.set(bool_render).unwrap();
                    let mut next = Checkbox::new(bool_initial)
                        .controlled(state.clone(), action)
                        .node();
                    adapter
                        .update_root(&mut next, &mut mounted, &previous)
                        .unwrap();
                    prop_assert_eq!(
                        adapter
                            .store
                            .get(widget)
                            .unwrap()
                            .as_any()
                            .downcast_ref::<NativeCheckbox>()
                            .unwrap()
                            .checked,
                        bool_render
                    );
                }
                2 => {
                    let initial = f64::from(number_initial);
                    let edited = f64::from(number_edit);
                    let rendered = f64::from(number_render);
                    let state = context.state(initial).unwrap();
                    let action = context
                        .action({
                            let state = state.clone();
                            move |_, value: f64| state.set(value).unwrap()
                        })
                        .unwrap();
                    let mut root = NumberInput::new(initial)
                        .controlled(state.clone(), action.clone())
                        .node();
                    let mut adapter = EngineAdapter::with_context(context);
                    let (previous, mut mounted) = adapter.mount_root(&mut root).unwrap();
                    let widget = mounted.widget.unwrap();
                    adapter
                        .store
                        .get_mut(widget)
                        .unwrap()
                        .as_any_mut()
                        .downcast_mut::<NativeSlider>()
                        .unwrap()
                        .value = edited as f32;
                    adapter.poll_input_changes(&mut mounted).unwrap();
                    prop_assert_eq!(state.get().unwrap(), edited as f32 as f64);

                    state.set(rendered).unwrap();
                    let mut next = NumberInput::new(initial)
                        .controlled(state.clone(), action)
                        .node();
                    adapter
                        .update_root(&mut next, &mut mounted, &previous)
                        .unwrap();
                    prop_assert_eq!(
                        adapter
                            .store
                            .get(widget)
                            .unwrap()
                            .as_any()
                            .downcast_ref::<NativeSlider>()
                            .unwrap()
                            .value,
                        rendered as f32
                    );
                }
                _ => {
                    let options = ["red", "green", "blue", "yellow"];
                    let state = context
                        .state(ChoiceId::new(choice_initial))
                        .unwrap();
                    let action = context
                        .action({
                            let state = state.clone();
                            move |_, value: ChoiceId| state.set(value).unwrap()
                        })
                        .unwrap();
                    let mut root = Select::new(options, choice_initial)
                        .controlled(state.clone(), action.clone())
                        .node();
                    let mut adapter = EngineAdapter::with_context(context);
                    let (previous, mut mounted) = adapter.mount_root(&mut root).unwrap();
                    let widget = mounted.widget.unwrap();
                    let dropdown = adapter
                        .store
                        .get_mut(widget)
                        .unwrap()
                        .as_any_mut()
                        .downcast_mut::<NativeDropdown>()
                        .unwrap();
                    dropdown.selected = choice_edit;
                    dropdown.highlighted = choice_edit;
                    adapter.poll_input_changes(&mut mounted).unwrap();
                    prop_assert_eq!(state.get().unwrap(), ChoiceId::new(choice_edit));

                    state.set(ChoiceId::new(choice_render)).unwrap();
                    let mut next = Select::new(options, choice_initial)
                        .controlled(state.clone(), action)
                        .node();
                    adapter
                        .update_root(&mut next, &mut mounted, &previous)
                        .unwrap();
                    let dropdown = adapter
                        .store
                        .get(widget)
                        .unwrap()
                        .as_any()
                        .downcast_ref::<NativeDropdown>()
                        .unwrap();
                    prop_assert_eq!(dropdown.selected, choice_render);
                    prop_assert_eq!(dropdown.highlighted, choice_render);
                }
            }
        }
    }

    // Feature: synui-api-abi-reset, Property 5: 非受控输入的本地值保留
    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 100,
            max_shrink_iters: 256,
            .. ProptestConfig::default()
        })]
        #[test]
        fn uncontrolled_inputs_retain_or_replace_values_only_on_defined_operations(
            kind in 0u8..4,
            text_initial in 0u8..=25,
            bool_initial in any::<bool>(),
            number_initial in 0u8..=100,
            choice_initial in 0usize..4,
            operations in prop::collection::vec((0u8..4, 0u8..=100), 1..=32),
        ) {
            use crate::app::{App, AppConfig};
            use crate::widgets::{Checkbox, NumberInput, Select, TextInput};

            let app = App::new(AppConfig::default()).expect("App creation must succeed");
            let context = app.context();
            let options = ["red", "green", "blue", "yellow"];

            match kind {
                0 => {
                    let initial = format!("initial-{text_initial}");
                    let build = |key: u64| {
                        TextInput::new("text")
                            .uncontrolled(initial.clone(), None)
                            .node()
                            .key(Key::unsigned(key))
                    };
                    let mut epoch = 0;
                    let mut root = build(epoch);
                    let mut adapter = EngineAdapter::with_context(context);
                    let (mut previous, mut mounted) = adapter.mount_root(&mut root).unwrap();
                    let mut expected = initial.clone();

                    for (operation, value) in operations {
                        match operation {
                            0 => {
                                let edit = format!("edit-{value}");
                                let widget = mounted.widget.unwrap();
                                adapter
                                    .store
                                    .get_mut(widget)
                                    .unwrap()
                                    .as_any_mut()
                                    .downcast_mut::<NativeTextField>()
                                    .unwrap()
                                    .edit
                                    .text = edit.clone();
                                adapter.poll_input_changes(&mut mounted).unwrap();
                                expected = edit;
                            }
                            1 => {
                                let mut next = build(epoch);
                                previous = adapter
                                    .update_root(&mut next, &mut mounted, &previous)
                                    .unwrap();
                                root = next;
                            }
                            2 => {
                                epoch += 1;
                                let mut next = build(epoch);
                                previous = adapter
                                    .update_root(&mut next, &mut mounted, &previous)
                                    .unwrap();
                                root = next;
                                expected = format!("initial-{text_initial}");
                            }
                            _ => {
                                let value = format!("set-{value}");
                                adapter
                                    .set_value(&mut mounted, InputValue::Text(value.clone()))
                                    .unwrap();
                                expected = value;
                            }
                        }
                        let actual = adapter
                            .read_native_input(mounted.widget.unwrap(), &mounted.previous)
                            .unwrap();
                        prop_assert_eq!(actual, Some(InputValue::Text(expected.clone())));
                    }
                    adapter.unmount_root(&mut root, &mut mounted);
                }
                1 => {
                    let build = |key: u64| {
                        Checkbox::new(bool_initial)
                            .uncontrolled(bool_initial, None)
                            .node()
                            .key(Key::unsigned(key))
                    };
                    let mut epoch = 0;
                    let mut root = build(epoch);
                    let mut adapter = EngineAdapter::with_context(context);
                    let (mut previous, mut mounted) = adapter.mount_root(&mut root).unwrap();
                    let mut expected = bool_initial;

                    for (operation, value) in operations {
                        match operation {
                            0 => {
                                expected = value % 2 == 0;
                                let widget = mounted.widget.unwrap();
                                adapter
                                    .store
                                    .get_mut(widget)
                                    .unwrap()
                                    .as_any_mut()
                                    .downcast_mut::<NativeCheckbox>()
                                    .unwrap()
                                    .checked = expected;
                                adapter.poll_input_changes(&mut mounted).unwrap();
                            }
                            1 => {
                                let mut next = build(epoch);
                                previous = adapter
                                    .update_root(&mut next, &mut mounted, &previous)
                                    .unwrap();
                                root = next;
                            }
                            2 => {
                                epoch += 1;
                                let mut next = build(epoch);
                                previous = adapter
                                    .update_root(&mut next, &mut mounted, &previous)
                                    .unwrap();
                                root = next;
                                expected = bool_initial;
                            }
                            _ => {
                                expected = value % 2 == 0;
                                adapter
                                    .set_value(&mut mounted, InputValue::Bool(expected))
                                    .unwrap();
                            }
                        }
                        let actual = adapter
                            .read_native_input(mounted.widget.unwrap(), &mounted.previous)
                            .unwrap();
                        prop_assert_eq!(actual, Some(InputValue::Bool(expected)));
                    }
                    adapter.unmount_root(&mut root, &mut mounted);
                }
                2 => {
                    let initial = f64::from(number_initial);
                    let build = |key: u64| {
                        NumberInput::new(initial)
                            .uncontrolled(initial, None)
                            .node()
                            .key(Key::unsigned(key))
                    };
                    let mut epoch = 0;
                    let mut root = build(epoch);
                    let mut adapter = EngineAdapter::with_context(context);
                    let (mut previous, mut mounted) = adapter.mount_root(&mut root).unwrap();
                    let mut expected = initial;

                    for (operation, value) in operations {
                        match operation {
                            0 => {
                                expected = f64::from(value);
                                let widget = mounted.widget.unwrap();
                                adapter
                                    .store
                                    .get_mut(widget)
                                    .unwrap()
                                    .as_any_mut()
                                    .downcast_mut::<NativeSlider>()
                                    .unwrap()
                                    .value = expected as f32;
                                adapter.poll_input_changes(&mut mounted).unwrap();
                                expected = expected as f32 as f64;
                            }
                            1 => {
                                let mut next = build(epoch);
                                previous = adapter
                                    .update_root(&mut next, &mut mounted, &previous)
                                    .unwrap();
                                root = next;
                            }
                            2 => {
                                epoch += 1;
                                let mut next = build(epoch);
                                previous = adapter
                                    .update_root(&mut next, &mut mounted, &previous)
                                    .unwrap();
                                root = next;
                                expected = initial;
                            }
                            _ => {
                                expected = f64::from(value);
                                adapter
                                    .set_value(&mut mounted, InputValue::Number(expected))
                                    .unwrap();
                            }
                        }
                        let actual = adapter
                            .read_native_input(mounted.widget.unwrap(), &mounted.previous)
                            .unwrap();
                        prop_assert_eq!(actual, Some(InputValue::Number(expected)));
                    }
                    adapter.unmount_root(&mut root, &mut mounted);
                }
                _ => {
                    let build = |key: u64| {
                        Select::new(options, choice_initial)
                            .uncontrolled(ChoiceId::new(choice_initial), None)
                            .node()
                            .key(Key::unsigned(key))
                    };
                    let mut epoch = 0;
                    let mut root = build(epoch);
                    let mut adapter = EngineAdapter::with_context(context);
                    let (mut previous, mut mounted) = adapter.mount_root(&mut root).unwrap();
                    let mut expected = ChoiceId::new(choice_initial);

                    for (operation, value) in operations {
                        match operation {
                            0 => {
                                expected = ChoiceId::new(value as usize % options.len());
                                let widget = mounted.widget.unwrap();
                                let dropdown = adapter
                                    .store
                                    .get_mut(widget)
                                    .unwrap()
                                    .as_any_mut()
                                    .downcast_mut::<NativeDropdown>()
                                    .unwrap();
                                dropdown.selected = expected.index();
                                dropdown.highlighted = expected.index();
                                adapter.poll_input_changes(&mut mounted).unwrap();
                            }
                            1 => {
                                let mut next = build(epoch);
                                previous = adapter
                                    .update_root(&mut next, &mut mounted, &previous)
                                    .unwrap();
                                root = next;
                            }
                            2 => {
                                epoch += 1;
                                let mut next = build(epoch);
                                previous = adapter
                                    .update_root(&mut next, &mut mounted, &previous)
                                    .unwrap();
                                root = next;
                                expected = ChoiceId::new(choice_initial);
                            }
                            _ => {
                                expected = ChoiceId::new(value as usize % options.len());
                                adapter
                                    .set_value(&mut mounted, InputValue::Choice(expected))
                                    .unwrap();
                            }
                        }
                        let actual = adapter
                            .read_native_input(mounted.widget.unwrap(), &mounted.previous)
                            .unwrap();
                        prop_assert_eq!(actual, Some(InputValue::Choice(expected)));
                    }
                    adapter.unmount_root(&mut root, &mut mounted);
                }
            }
        }
    }

    fn assert_input_type_mismatch_is_atomic(
        adapter: &mut EngineAdapter,
        mounted: &mut Mounted,
        wrong: InputValue,
        expected: InputValue,
    ) {
        let widget_count = adapter.retained_widget_count();
        let widget = mounted.widget.unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            adapter.set_value(mounted, wrong)
        }));
        assert!(result.is_ok(), "type mismatch must not panic");
        assert!(matches!(
            result.unwrap(),
            Err(ElementError::Input(
                InputConfigError::UnsupportedValue { .. }
            ))
        ));
        assert_eq!(adapter.retained_widget_count(), widget_count);
        assert_eq!(
            adapter
                .read_native_input(widget, &mounted.previous)
                .unwrap(),
            Some(expected)
        );
    }

    // Feature: synui-api-abi-reset, Property 6: 输入类型不匹配安全失败
    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 100,
            max_shrink_iters: 256,
            .. ProptestConfig::default()
        })]
        #[test]
        fn input_type_mismatches_fail_without_panic_or_widget_mutation(
            kind in 0u8..4,
            value in 0u8..=100,
        ) {
            use crate::app::{App, AppConfig};
            use crate::widgets::{Checkbox, NumberInput, Select, TextInput};

            let app = App::new(AppConfig::default()).expect("App creation must succeed");
            let context = app.context();
            let options = ["red", "green", "blue", "yellow"];

            match kind {
                0 => {
                    let initial = format!("initial-{value}");
                    let mut root = TextInput::new("text")
                        .uncontrolled(initial.clone(), None)
                        .node();
                    let mut adapter = EngineAdapter::with_context(context);
                    let (_, mut mounted) = adapter.mount_root(&mut root).unwrap();
                    assert_input_type_mismatch_is_atomic(
                        &mut adapter,
                        &mut mounted,
                        InputValue::Bool(value % 2 == 0),
                        InputValue::Text(initial),
                    );
                    adapter.unmount_root(&mut root, &mut mounted);
                }
                1 => {
                    let initial = value % 2 == 0;
                    let mut root = Checkbox::new(initial).node();
                    let mut adapter = EngineAdapter::with_context(context);
                    let (_, mut mounted) = adapter.mount_root(&mut root).unwrap();
                    assert_input_type_mismatch_is_atomic(
                        &mut adapter,
                        &mut mounted,
                        InputValue::Text(format!("wrong-{value}")),
                        InputValue::Bool(initial),
                    );
                    adapter.unmount_root(&mut root, &mut mounted);
                }
                2 => {
                    let initial = f64::from(value);
                    let mut root = NumberInput::new(initial).node();
                    let mut adapter = EngineAdapter::with_context(context);
                    let (_, mut mounted) = adapter.mount_root(&mut root).unwrap();
                    assert_input_type_mismatch_is_atomic(
                        &mut adapter,
                        &mut mounted,
                        InputValue::Choice(ChoiceId::new(value as usize % options.len())),
                        InputValue::Number(initial),
                    );
                    adapter.unmount_root(&mut root, &mut mounted);
                }
                _ => {
                    let initial = ChoiceId::new(value as usize % options.len());
                    let mut root = Select::new(options, initial.index()).node();
                    let mut adapter = EngineAdapter::with_context(context);
                    let (_, mut mounted) = adapter.mount_root(&mut root).unwrap();
                    assert_input_type_mismatch_is_atomic(
                        &mut adapter,
                        &mut mounted,
                        InputValue::Number(f64::from(value)),
                        InputValue::Choice(initial),
                    );
                    adapter.unmount_root(&mut root, &mut mounted);
                }
            }
        }
    }

    #[test]
    fn controlled_native_edit_dispatches_typed_action_then_state_renders_back() {
        use crate::app::{App, AppConfig};
        use crate::widgets::TextInput;

        let app = App::new(AppConfig::default()).unwrap();
        let context = app.context();
        let state = context.state("before".to_owned()).unwrap();
        let action = context
            .action({
                let state = state.clone();
                move |_, value: String| state.set(value).unwrap()
            })
            .unwrap();
        let mut root = TextInput::new("value")
            .controlled(state.clone(), action.clone())
            .node();
        let mut adapter = EngineAdapter::with_context(context);
        let (previous, mut mounted) = adapter.mount_root(&mut root).unwrap();
        let widget = mounted.widget.unwrap();
        adapter
            .store
            .get_mut(widget)
            .unwrap()
            .as_any_mut()
            .downcast_mut::<NativeTextField>()
            .unwrap()
            .edit
            .text = "edited".to_owned();

        adapter.poll_input_changes(&mut mounted).unwrap();
        assert_eq!(state.get().unwrap(), "edited");

        state.set("rendered".to_owned()).unwrap();
        let mut next = TextInput::new("value").controlled(state, action).node();
        adapter
            .update_root(&mut next, &mut mounted, &previous)
            .unwrap();
        assert_eq!(
            adapter
                .store
                .get(widget)
                .unwrap()
                .as_any()
                .downcast_ref::<NativeTextField>()
                .unwrap()
                .edit
                .text,
            "rendered"
        );
    }

    #[test]
    fn uncontrolled_update_retains_edit_explicit_set_overwrites_and_new_identity_remounts() {
        use crate::widgets::Checkbox;

        let mut root = Checkbox::new(false).node().key(Key::new("a"));
        let mut adapter = EngineAdapter::new();
        let (previous, mut mounted) = adapter.mount_root(&mut root).unwrap();
        let original = mounted.widget.unwrap();
        adapter
            .store
            .get_mut(original)
            .unwrap()
            .as_any_mut()
            .downcast_mut::<NativeCheckbox>()
            .unwrap()
            .checked = true;
        adapter.poll_input_changes(&mut mounted).unwrap();

        let mut same_identity = Checkbox::new(false).node().key(Key::new("a"));
        let previous = adapter
            .update_root(&mut same_identity, &mut mounted, &previous)
            .unwrap();
        assert!(
            adapter
                .store
                .get(original)
                .unwrap()
                .as_any()
                .downcast_ref::<NativeCheckbox>()
                .unwrap()
                .checked
        );

        adapter
            .set_value(&mut mounted, InputValue::Bool(false))
            .unwrap();
        assert!(
            !adapter
                .store
                .get(original)
                .unwrap()
                .as_any()
                .downcast_ref::<NativeCheckbox>()
                .unwrap()
                .checked
        );

        let mut replacement = Checkbox::new(true).node().key(Key::new("b"));
        adapter
            .update_root(&mut replacement, &mut mounted, &previous)
            .unwrap();
        assert_ne!(mounted.widget, Some(original));
        assert!(
            adapter
                .store
                .get(mounted.widget.unwrap())
                .unwrap()
                .as_any()
                .downcast_ref::<NativeCheckbox>()
                .unwrap()
                .checked
        );
    }

    #[test]
    fn invalid_input_configs_fail_before_any_widget_is_retained() {
        use crate::widgets::{NumberInput, Select};

        let mut invalid_choice = Select::new(["only"], 2).node();
        let mut adapter = EngineAdapter::new();
        assert!(matches!(
            adapter.mount_root(&mut invalid_choice),
            Err(ElementError::Input(InputConfigError::InvalidChoice { .. }))
        ));
        assert_eq!(adapter.retained_widget_count(), 0);

        let mut out_of_range = NumberInput::new(11.0).range(0.0, 10.0).node();
        assert!(matches!(
            adapter.mount_root(&mut out_of_range),
            Err(ElementError::Input(InputConfigError::OutOfRange { .. }))
        ));
        assert_eq!(adapter.retained_widget_count(), 0);

        let config = InputConfig::uncontrolled("wrong".to_owned(), None);
        let mut unsupported = Node::from_spec(
            ElementSpec::custom("checkbox", []).with_text_input(String::new(), &config),
        );
        assert!(matches!(
            adapter.mount_root(&mut unsupported),
            Err(ElementError::Input(
                InputConfigError::UnsupportedValue { .. }
            ))
        ));
        assert_eq!(adapter.retained_widget_count(), 0);
    }

    #[test]
    fn failed_native_downcast_returns_a_defined_error_without_panicking() {
        use crate::widgets::Checkbox;

        let mut root = Checkbox::new(false).node();
        let mut adapter = EngineAdapter::new();
        let (_, mut mounted) = adapter.mount_root(&mut root).unwrap();
        let widget = mounted.widget.unwrap();
        adapter.store.replace(widget, Label::new("wrong native"));

        assert!(matches!(
            adapter.poll_input_changes(&mut mounted),
            Err(ElementError::NativeWidgetType {
                expected: "Checkbox"
            })
        ));
    }

    #[test]
    fn facade_scroll_and_virtual_list_mount_native_hosts() {
        use crate::widgets::{Scroll, VirtualList};
        use synui_core::Size;
        use synui_render::TextRenderer;
        use synui_widgets::{sync_post_layout, sync_pre_paint};

        let mut scroll_root = Scroll::new([Node::text("row a"), Node::text("row b")]).node();
        let mut adapter = EngineAdapter::new();
        let (_, scroll_mounted) = adapter.mount_root(&mut scroll_root).unwrap();
        let scroll_id = scroll_mounted.widget.unwrap();
        assert!(adapter
            .store
            .get(scroll_id)
            .unwrap()
            .as_any()
            .is::<NativeScrollView>());

        let mut list_root = VirtualList::new(10_000)
            .item_extent(28.0)
            .item(|index| format!("item {index}"))
            .node();
        let mut list_adapter = EngineAdapter::new();
        let (_, list_mounted) = list_adapter.mount_root(&mut list_root).unwrap();
        let list_id = list_mounted.widget.unwrap();
        assert!(list_adapter
            .store
            .get(list_id)
            .unwrap()
            .as_any()
            .is::<NativeListView>());

        list_adapter.store.set_viewport(Size::new(400.0, 280.0));
        let mut layout = LayoutEngine::new();
        let mut text = TextRenderer::new();
        layout.compute(
            &mut list_adapter.store,
            &mut text,
            list_id,
            Size::new(400.0, 280.0),
        );
        sync_post_layout(&mut list_adapter.store, &mut text);
        sync_pre_paint(&mut list_adapter.store, &mut text);

        let mounted = list_adapter.store.children_of(list_id).len();
        assert!(
            mounted > 0 && mounted <= 40,
            "facade VirtualList 10k mount bound exceeded: mounted={mounted}"
        );
    }

    #[test]
    fn facade_data_table_mounts_native_host_with_bounded_cells() {
        use crate::widgets::{DataTable, TableColumn};
        use synui_core::Size;
        use synui_render::TextRenderer;
        use synui_widgets::{sync_post_layout, sync_pre_paint};

        let mut root = DataTable::new()
            .columns([
                TableColumn::new("name", "Name", 120.0),
                TableColumn::new("score", "Score", 80.0),
            ])
            .rows(10_000)
            .freeze_leading(1)
            .cell(|row, col| format!("{row}:{col}"))
            .node();
        let mut adapter = EngineAdapter::new();
        let (_, mounted) = adapter.mount_root(&mut root).unwrap();
        let table_id = mounted.widget.unwrap();
        assert!(adapter
            .store
            .get(table_id)
            .unwrap()
            .as_any()
            .is::<NativeDataTable>());

        adapter.store.set_viewport(Size::new(480.0, 320.0));
        let mut layout = LayoutEngine::new();
        let mut text = TextRenderer::new();
        layout.compute(
            &mut adapter.store,
            &mut text,
            table_id,
            Size::new(480.0, 320.0),
        );
        sync_post_layout(&mut adapter.store, &mut text);
        sync_pre_paint(&mut adapter.store, &mut text);

        let mounted_cells = adapter.store.children_of(table_id).len();
        assert!(
            mounted_cells > 0 && mounted_cells <= 80,
            "facade DataTable 10k mount bound exceeded: mounted={mounted_cells}"
        );
        let props = adapter.store.effective_a11y(table_id).unwrap();
        assert_eq!(props.role, A11yRole::Grid);
        assert_eq!(props.row_count, Some(10_000));
        assert_eq!(props.col_count, Some(2));
    }

    #[test]
    fn facade_rich_document_mounts_native_richedit_host() {
        use crate::widgets::RichDocument;

        let doc = synui_core::BlockDoc::from_plain("hello rich");
        let mut root = RichDocument::new(doc).node();
        let mut adapter = EngineAdapter::new();
        let (_, mounted) = adapter.mount_root(&mut root).unwrap();
        let editor_id = mounted.widget.unwrap();
        assert!(adapter
            .store
            .get(editor_id)
            .unwrap()
            .as_any()
            .is::<NativeRichEdit>());
        let editor = adapter
            .store
            .get(editor_id)
            .unwrap()
            .as_any()
            .downcast_ref::<NativeRichEdit>()
            .unwrap();
        assert_eq!(editor.doc.plain_text(), "hello rich");
    }

    #[test]
    fn facade_label_mounts_native_label_with_a11y_name() {
        use crate::widgets::Label as FacadeLabel;

        let mut root = FacadeLabel::new("Hello Label")
            .a11y_name("Greeting")
            .node()
            .key(BusinessKey::new("label"));
        let mut adapter = EngineAdapter::new();
        let (_, mounted) = adapter.mount_root(&mut root).unwrap();
        let widget = mounted.widget.expect("label widget");
        let label = adapter
            .store
            .get(widget)
            .unwrap()
            .as_any()
            .downcast_ref::<Label>()
            .expect("native Label");
        assert_eq!(label.text, "Hello Label");
        let a11y = synui_core::Widget::a11y(label);
        assert_eq!(a11y.role, A11yRole::Label);
        assert_eq!(
            mounted.previous.accessibility.name.as_deref(),
            Some("Greeting")
        );
    }

    #[test]
    fn facade_switch_radio_and_linear_progress_mount_native_hosts() {
        use crate::widgets::{LinearProgress, Radio, Switch};

        let mut switch_root = Switch::new(true).node().key(BusinessKey::new("sw"));
        let mut adapter = EngineAdapter::new();
        let (_, mounted) = adapter.mount_root(&mut switch_root).unwrap();
        let switch_id = mounted.widget.expect("switch");
        assert!(
            adapter
                .store
                .get(switch_id)
                .unwrap()
                .as_any()
                .downcast_ref::<NativeSwitch>()
                .expect("native Switch")
                .on
        );

        let mut radio_root = Radio::new("Option A", false)
            .node()
            .key(BusinessKey::new("radio"));
        let mut adapter = EngineAdapter::new();
        let (_, mut mounted) = adapter.mount_root(&mut radio_root).unwrap();
        let radio_id = mounted.widget.expect("radio");
        {
            let radio = adapter
                .store
                .get_mut(radio_id)
                .unwrap()
                .as_any_mut()
                .downcast_mut::<NativeRadio>()
                .expect("native Radio");
            assert!(!radio.selected);
            radio.selected = true;
        }
        adapter.poll_input_changes(&mut mounted).unwrap();
        assert_eq!(
            mounted.observed_input.as_ref(),
            Some(&InputValue::Bool(true))
        );

        let mut progress_root = LinearProgress::new(0.4)
            .node()
            .key(BusinessKey::new("prog"));
        let mut adapter = EngineAdapter::new();
        let (previous, mut mounted) = adapter.mount_root(&mut progress_root).unwrap();
        let progress_id = mounted.widget.expect("progress");
        assert!(
            (adapter
                .store
                .get(progress_id)
                .unwrap()
                .as_any()
                .downcast_ref::<NativeLinearProgress>()
                .expect("native LinearProgress")
                .value
                - 0.4)
                .abs()
                < f32::EPSILON
        );
        let mut next = LinearProgress::new(0.75)
            .indeterminate(true)
            .node()
            .key(BusinessKey::new("prog"));
        let _ = adapter
            .update_root(&mut next, &mut mounted, &previous)
            .unwrap();
        let progress = adapter
            .store
            .get(progress_id)
            .unwrap()
            .as_any()
            .downcast_ref::<NativeLinearProgress>()
            .expect("native LinearProgress");
        assert!((progress.value - 0.75).abs() < f32::EPSILON);
        assert!(progress.indeterminate);
    }

    #[test]
    fn facade_text_area_mounts_and_polls_controlled_changes() {
        use crate::app::{App, AppConfig};
        use crate::widgets::TextArea;

        let app = App::new(AppConfig::default()).unwrap();
        let context = app.context();
        let state = context.state("before".to_owned()).unwrap();
        let action = context
            .action({
                let state = state.clone();
                move |_, value: String| state.set(value).unwrap()
            })
            .unwrap();
        let mut root = TextArea::new("Notes")
            .controlled(state.clone(), action.clone())
            .node()
            .key(BusinessKey::new("area"));
        let mut adapter = EngineAdapter::with_context(context);
        let (previous, mut mounted) = adapter.mount_root(&mut root).unwrap();
        let widget = mounted.widget.expect("text-area");
        {
            let area = adapter
                .store
                .get_mut(widget)
                .unwrap()
                .as_any_mut()
                .downcast_mut::<NativeTextArea>()
                .expect("native TextArea");
            assert_eq!(area.edit.text, "before");
            area.edit.text = "edited".to_owned();
            area.notify_text_changed();
        }
        adapter.poll_input_changes(&mut mounted).unwrap();
        assert_eq!(
            mounted.observed_input.as_ref(),
            Some(&InputValue::Text("edited".into()))
        );
        assert_eq!(state.get().unwrap(), "edited");

        let mut next = TextArea::new("Notes")
            .controlled(state, action)
            .node()
            .key(BusinessKey::new("area"));
        let _ = adapter
            .update_root(&mut next, &mut mounted, &previous)
            .unwrap();
        assert_eq!(
            adapter
                .store
                .get(widget)
                .unwrap()
                .as_any()
                .downcast_ref::<NativeTextArea>()
                .expect("native TextArea")
                .edit
                .text,
            "edited"
        );
        assert_eq!(
            mounted.previous.accessibility.name.as_deref(),
            Some("Notes")
        );
    }

    #[test]
    fn facade_text_input_password_masks_display_and_a11y() {
        use crate::app::{App, AppConfig};
        use crate::widgets::TextInput;

        let app = App::new(AppConfig::default()).unwrap();
        let context = app.context();
        let state = context.state("secret".to_owned()).unwrap();
        let action = context
            .action({
                let state = state.clone();
                move |_, value: String| state.set(value).unwrap()
            })
            .unwrap();

        let mut root = TextInput::new("Password")
            .password(true)
            .controlled(state, action)
            .node()
            .key(BusinessKey::new("password"));
        let mut adapter = EngineAdapter::with_context(context);
        let (_previous, mounted) = adapter.mount_root(&mut root).unwrap();
        let widget = mounted.widget.expect("password text-input");
        let field = adapter
            .store
            .get(widget)
            .unwrap()
            .as_any()
            .downcast_ref::<NativeTextField>()
            .expect("native TextField");
        assert!(field.password);
        assert_eq!(field.edit.text, "secret");
        let a11y = synui_core::Widget::a11y(field);
        assert_eq!(a11y.value.as_deref(), Some("••••••"));
    }

    #[test]
    fn facade_text_input_and_area_focus_protects_controlled_edit() {
        use crate::app::{App, AppConfig};
        use crate::widgets::{TextArea, TextInput};

        let app = App::new(AppConfig::default()).unwrap();
        let context = app.context();
        let state = context.state("before".to_owned()).unwrap();
        let action = context
            .action({
                let state = state.clone();
                move |_, value: String| state.set(value).unwrap()
            })
            .unwrap();

        let mut root = TextInput::new("field")
            .controlled(state.clone(), action.clone())
            .node()
            .key(BusinessKey::new("field"));
        let mut adapter = EngineAdapter::with_context(context.clone());
        let (previous, mut mounted) = adapter.mount_root(&mut root).unwrap();
        let widget = mounted.widget.expect("text-input");
        assert!(adapter.store.focus_manager_mut().focus(widget));
        {
            let field = adapter
                .store
                .get_mut(widget)
                .unwrap()
                .as_any_mut()
                .downcast_mut::<NativeTextField>()
                .unwrap();
            field.edit.text = "typing".to_owned();
            field.notify_text_changed();
        }
        let mut next = TextInput::new("field")
            .controlled(state.clone(), action.clone())
            .node()
            .key(BusinessKey::new("field"));
        let _ = adapter
            .update_root(&mut next, &mut mounted, &previous)
            .unwrap();
        assert_eq!(
            adapter
                .store
                .get(widget)
                .unwrap()
                .as_any()
                .downcast_ref::<NativeTextField>()
                .unwrap()
                .edit
                .text,
            "typing",
            "focused controlled TextInput must not clobber in-progress edit"
        );

        adapter.store.focus_manager_mut().clear_focus();
        state.set("synced".to_owned()).unwrap();
        let mut next = TextInput::new("field")
            .controlled(state.clone(), action.clone())
            .node()
            .key(BusinessKey::new("field"));
        let _ = adapter
            .update_root(&mut next, &mut mounted, &previous)
            .unwrap();
        assert_eq!(
            adapter
                .store
                .get(widget)
                .unwrap()
                .as_any()
                .downcast_ref::<NativeTextField>()
                .unwrap()
                .edit
                .text,
            "synced"
        );

        let mut area_root = TextArea::new("Notes")
            .controlled(state.clone(), action.clone())
            .node()
            .key(BusinessKey::new("area"));
        let mut adapter = EngineAdapter::with_context(context);
        let (previous, mut mounted) = adapter.mount_root(&mut area_root).unwrap();
        let widget = mounted.widget.expect("text-area");
        assert!(adapter.store.focus_manager_mut().focus(widget));
        {
            let area = adapter
                .store
                .get_mut(widget)
                .unwrap()
                .as_any_mut()
                .downcast_mut::<NativeTextArea>()
                .unwrap();
            area.edit.text = "area-typing".to_owned();
            area.notify_text_changed();
        }
        let mut next = TextArea::new("Notes")
            .controlled(state.clone(), action)
            .node()
            .key(BusinessKey::new("area"));
        let _ = adapter
            .update_root(&mut next, &mut mounted, &previous)
            .unwrap();
        assert_eq!(
            adapter
                .store
                .get(widget)
                .unwrap()
                .as_any()
                .downcast_ref::<NativeTextArea>()
                .unwrap()
                .edit
                .text,
            "area-typing",
            "focused controlled TextArea must not clobber in-progress edit"
        );
    }

    #[test]
    fn facade_card_list_item_and_circular_progress_mount_native_hosts() {
        use crate::widgets::{Card, CircularProgress, ListItem};

        let mut card_root = Card::new([Node::text("inside")])
            .elevation(2)
            .node()
            .key(BusinessKey::new("card"));
        let mut adapter = EngineAdapter::new();
        let (_, mounted) = adapter.mount_root(&mut card_root).unwrap();
        let card_id = mounted.widget.expect("card");
        let card = adapter
            .store
            .get(card_id)
            .unwrap()
            .as_any()
            .downcast_ref::<NativeCard>()
            .expect("native Card");
        assert_eq!(card.elevation, 2);
        assert_eq!(mounted.children.len(), 1);

        let mut item_root = ListItem::new("Row")
            .subtitle("Detail")
            .node()
            .key(BusinessKey::new("item"));
        let mut adapter = EngineAdapter::new();
        let (_, mounted) = adapter.mount_root(&mut item_root).unwrap();
        let item_id = mounted.widget.expect("list-item");
        let item = adapter
            .store
            .get(item_id)
            .unwrap()
            .as_any()
            .downcast_ref::<NativeListItem>()
            .expect("native ListItem");
        assert_eq!(item.title, "Row");
        assert_eq!(item.subtitle.as_deref(), Some("Detail"));

        let mut progress_root = CircularProgress::new(0.3)
            .indeterminate(false)
            .node()
            .key(BusinessKey::new("cprog"));
        let mut adapter = EngineAdapter::new();
        let (_, mounted) = adapter.mount_root(&mut progress_root).unwrap();
        let progress_id = mounted.widget.expect("circular-progress");
        let progress = adapter
            .store
            .get(progress_id)
            .unwrap()
            .as_any()
            .downcast_ref::<NativeCircularProgress>()
            .expect("native CircularProgress");
        assert!((progress.value - 0.3).abs() < f32::EPSILON);
        assert!(!progress.indeterminate);
    }

    #[test]
    fn facade_lottie_and_morphing_shape_mount_native_hosts() {
        use crate::widgets::{Lottie, MorphingShape};

        let pulse = include_str!("../../../examples/assets/lottie_pulse.json");
        let mut root = Lottie::new(pulse)
            .size(96.0, 96.0)
            .playing(false)
            .node()
            .key(BusinessKey::new("lottie"));
        let mut adapter = EngineAdapter::new();
        let (previous, mut mounted) = adapter.mount_root(&mut root).unwrap();
        let lottie_id = mounted.widget.expect("lottie");
        {
            let lottie = adapter
                .store
                .get(lottie_id)
                .unwrap()
                .as_any()
                .downcast_ref::<NativeLottie>()
                .expect("native Lottie");
            assert!(!lottie.playing());
            assert!((lottie.width() - 96.0).abs() < f64::EPSILON);
        }
        let mut next = Lottie::new(pulse)
            .size(96.0, 96.0)
            .playing(true)
            .node()
            .key(BusinessKey::new("lottie"));
        let _ = adapter
            .update_root(&mut next, &mut mounted, &previous)
            .unwrap();
        assert!(adapter
            .store
            .get(lottie_id)
            .unwrap()
            .as_any()
            .downcast_ref::<NativeLottie>()
            .unwrap()
            .playing());

        let mut bad = Lottie::new("{not-json").node().key(BusinessKey::new("bad"));
        let mut adapter = EngineAdapter::new();
        assert!(matches!(
            adapter.mount_root(&mut bad),
            Err(ElementError::InvalidProperty { name: "json", .. })
        ));

        let mut asset = Lottie::from_asset("pulse")
            .playing(false)
            .node()
            .key(BusinessKey::new("asset"));
        let mut adapter = EngineAdapter::new();
        let (_, mounted_asset) = adapter.mount_root(&mut asset).unwrap();
        let asset_id = mounted_asset.widget.expect("asset lottie");
        assert!(!adapter
            .store
            .get(asset_id)
            .unwrap()
            .as_any()
            .downcast_ref::<NativeLottie>()
            .unwrap()
            .playing());

        let mut morph_root = MorphingShape::new()
            .size(80.0)
            .playing(false)
            .speed(0.2)
            .node()
            .key(BusinessKey::new("morph"));
        let mut adapter = EngineAdapter::new();
        let (previous, mut mounted) = adapter.mount_root(&mut morph_root).unwrap();
        let morph_id = mounted.widget.expect("morph");
        {
            let morph = adapter
                .store
                .get(morph_id)
                .unwrap()
                .as_any()
                .downcast_ref::<NativeMorphingShape>()
                .expect("native MorphingShape");
            assert!(!morph.playing());
            assert!((morph.size() - 80.0).abs() < f64::EPSILON);
            assert!((morph.speed() - 0.2).abs() < f32::EPSILON);
        }
        let mut next = MorphingShape::new()
            .size(80.0)
            .playing(true)
            .speed(0.2)
            .node()
            .key(BusinessKey::new("morph"));
        let _ = adapter
            .update_root(&mut next, &mut mounted, &previous)
            .unwrap();
        assert!(adapter
            .store
            .get(morph_id)
            .unwrap()
            .as_any()
            .downcast_ref::<NativeMorphingShape>()
            .unwrap()
            .playing());
    }

    #[test]
    fn facade_label_truncates_to_paint_width_with_ellipsis() {
        use crate::widgets::Label as FacadeLabel;
        use synui_core::{Constraints, MeasureCtx, Size, Widget};
        use synui_render::TextRenderer;

        let mut root = FacadeLabel::new("WWWWWWWWWWWWWWWWWWWW")
            .truncate(true)
            .node()
            .key(BusinessKey::new("trunc-label"));
        let mut adapter = EngineAdapter::new();
        let (_, mounted) = adapter.mount_root(&mut root).unwrap();
        let widget = mounted.widget.expect("label widget");
        let mut text = TextRenderer::new();
        let label = adapter
            .store
            .get_mut(widget)
            .unwrap()
            .as_any_mut()
            .downcast_mut::<Label>()
            .expect("native Label");
        assert!(label.truncate);
        let mut ctx = MeasureCtx {
            text_measurer: Some(&mut text),
            max_width: Some(48.0),
        };
        let _ = Widget::measure(label, &mut ctx, Constraints::tight(Size::new(48.0, 40.0)));
        assert!(
            label.display_text().ends_with('…'),
            "truncate + narrow width must ellipsize, got {:?}",
            label.display_text()
        );
        assert!(label.display_text().chars().count() < label.text.chars().count());
    }

    #[test]
    fn facade_number_input_drag_grabs_pointer_and_updates_value() {
        use crate::app::AppConfig;
        use crate::widgets::NumberInput;
        use synui_core::{Event, Point, PointerButton, Rect};

        let app = crate::app::App::new(AppConfig::default()).unwrap();
        let context = app.context();
        let volume = context.state(40.0_f64).unwrap();
        let update = context
            .action({
                let volume = volume.clone();
                move |_, value: f64| volume.set(value).unwrap()
            })
            .unwrap();
        let mut root = NumberInput::new(40.0)
            .range(0.0, 100.0)
            .controlled(volume.clone(), update)
            .node()
            .key(BusinessKey::new("volume"));
        let mut adapter = EngineAdapter::with_context(context);
        let (_, mut mounted) = adapter.mount_root(&mut root).unwrap();
        let widget = mounted.widget.unwrap();
        adapter
            .store
            .get_mut(widget)
            .unwrap()
            .set_bounds(Rect::new(0.0, 0.0, 200.0, 40.0));
        adapter.store.set_root(widget);

        assert!(
            adapter
                .store
                .dispatch_event(&Event::PointerDown {
                    id: 0,
                    position: Point::new(100.0, 20.0),
                    button: PointerButton::Primary,
                    shift: false,
                    ctrl: false,
                })
                .handled
        );
        assert_eq!(adapter.store.pointer_grab(), Some(widget));
        assert!(
            adapter
                .store
                .dispatch_event(&Event::PointerMove {
                    id: 0,
                    position: Point::new(160.0, 8.0),
                })
                .handled
        );
        adapter.poll_input_changes(&mut mounted).unwrap();
        let value = volume.get().unwrap();
        assert!(
            value > 45.0,
            "drag should raise controlled volume, got {value}"
        );
        assert!(
            adapter
                .store
                .dispatch_event(&Event::PointerUp {
                    id: 0,
                    position: Point::new(160.0, 8.0),
                    button: PointerButton::Primary,
                })
                .handled
        );
        assert_eq!(adapter.store.pointer_grab(), None);
    }

    #[test]
    fn facade_data_table_applies_merge_spans_on_mount_and_update() {
        use crate::widgets::{DataTable, TableColumn};

        let mut root = DataTable::new()
            .columns([
                TableColumn::new("region", "Region", 100.0),
                TableColumn::new("code", "Code", 80.0),
            ])
            .rows(3)
            .span(0, 0, 2, 1)
            .cell(|row, col| format!("{row}:{col}"))
            .node()
            .key(BusinessKey::new("merge"));
        let mut adapter = EngineAdapter::new();
        let (previous, mut mounted) = adapter.mount_root(&mut root).unwrap();
        let widget = mounted.widget.unwrap();
        {
            let table = adapter
                .store
                .get(widget)
                .unwrap()
                .as_any()
                .downcast_ref::<NativeDataTable>()
                .unwrap();
            assert_eq!(table.spans().len(), 1);
            assert_eq!(table.spans()[0].row_span, 2);
            assert_eq!(table.spans()[0].col_span, 1);
        }

        let mut next = DataTable::new()
            .columns([
                TableColumn::new("region", "Region", 100.0),
                TableColumn::new("code", "Code", 80.0),
            ])
            .rows(3)
            .spans([(0, 0, 2, 1), (2, 0, 1, 2)])
            .cell(|row, col| format!("{row}:{col}"))
            .node()
            .key(BusinessKey::new("merge"));
        adapter
            .update_root(&mut next, &mut mounted, &previous)
            .unwrap();
        let table = adapter
            .store
            .get(widget)
            .unwrap()
            .as_any()
            .downcast_ref::<NativeDataTable>()
            .unwrap();
        assert_eq!(table.spans().len(), 2);
    }

    #[test]
    fn facade_menu_bar_and_context_menu_mount_and_dispatch_actions() {
        use crate::app::{App, AppConfig};
        use crate::widgets::{ContextMenu, Menu, MenuBar, MenuItem};
        use std::sync::{Arc, Mutex};

        let app = App::new(AppConfig::default()).unwrap();
        let context = app.context();
        let bar_seen = Arc::new(Mutex::new(None::<String>));
        let ctx_seen = Arc::new(Mutex::new(None::<String>));
        let on_bar = context
            .action({
                let bar_seen = Arc::clone(&bar_seen);
                move |_, id: String| {
                    *bar_seen.lock().unwrap() = Some(id);
                }
            })
            .unwrap();
        let on_ctx = context
            .action({
                let ctx_seen = Arc::clone(&ctx_seen);
                move |_, id: String| {
                    *ctx_seen.lock().unwrap() = Some(id);
                }
            })
            .unwrap();

        let mut root = Node::column([
            MenuBar::new([Menu::new(
                "File",
                [MenuItem::new("open", "Open"), MenuItem::new("quit", "Quit")],
            )])
            .on_action(on_bar)
            .node()
            .key(BusinessKey::new("menu-bar")),
            ContextMenu::new([
                MenuItem::new("info", "Info"),
                MenuItem::new("delete", "Delete"),
            ])
            .on_action(on_ctx)
            .show_at(12.0, 34.0)
            .node()
            .key(BusinessKey::new("context-menu")),
        ]);
        let mut adapter = EngineAdapter::with_context(context);
        let (_, mut mounted) = adapter.mount_root(&mut root).unwrap();
        assert_eq!(mounted.children.len(), 2);

        let bar_id = mounted.children[0].widget.unwrap();
        let ctx_id = mounted.children[1].widget.unwrap();
        assert!(adapter
            .store
            .get(bar_id)
            .unwrap()
            .as_any()
            .is::<NativeMenuBar>());
        assert!(adapter
            .store
            .get(ctx_id)
            .unwrap()
            .as_any()
            .is::<NativeContextMenu>());
        assert!(
            adapter
                .store
                .get(ctx_id)
                .unwrap()
                .as_any()
                .downcast_ref::<NativeContextMenu>()
                .unwrap()
                .open
        );

        adapter
            .store
            .get_mut(bar_id)
            .unwrap()
            .as_any_mut()
            .downcast_mut::<NativeMenuBar>()
            .unwrap()
            .last_action = Some(String::from("open"));
        adapter
            .store
            .get_mut(ctx_id)
            .unwrap()
            .as_any_mut()
            .downcast_mut::<NativeContextMenu>()
            .unwrap()
            .last_action = Some(String::from("info"));
        adapter.poll_input_changes(&mut mounted).unwrap();
        assert_eq!(bar_seen.lock().unwrap().as_deref(), Some("open"));
        assert_eq!(ctx_seen.lock().unwrap().as_deref(), Some("info"));
    }

    #[test]
    fn navigation_demo_menu_bar_opens_compact_popup_not_blank_columns() {
        use crate::widgets::{
            AppBar, BottomNavigation, Button, Dialog, Menu, MenuBar, MenuItem, NavigationDrawer,
        };
        use synui_core::{Event, Point, PointerButton, Size};
        use synui_layout::LayoutEngine;
        use synui_render::TextRenderer;
        use synui_widgets::{sync_post_layout, sync_pre_layout, MenuPanel, PopupHost};

        let mut root = Node::column([
            MenuBar::new([
                Menu::new(
                    "File",
                    [
                        MenuItem::new("home", "Home"),
                        MenuItem::new("settings", "Settings"),
                        MenuItem::new("quit", "Quit"),
                    ],
                ),
                Menu::new("Help", [MenuItem::new("about", "About")]),
            ])
            .node()
            .key(BusinessKey::new("menu-bar")),
            Node::text("status").key(BusinessKey::new("status")),
            AppBar::new("SynUI Navigation")
                .trailing([Button::new("Menu").node()])
                .node()
                .key(BusinessKey::new("app-bar")),
            NavigationDrawer::new([Button::new("List").node(), Button::new("Settings").node()])
                .open(false)
                .node()
                .key(BusinessKey::new("drawer")),
            Node::column([
                Node::text("Items"),
                Button::new("Item 1").node(),
                Button::new("Item 2").node(),
            ])
            .key(BusinessKey::new("body")),
            BottomNavigation::new(["Home", "Settings"])
                .selected(0)
                .node()
                .key(BusinessKey::new("bottom-nav")),
            Dialog::new(
                "About",
                [Node::text("about body"), Button::new("Close").node()],
            )
            .open(false)
            .node()
            .key(BusinessKey::new("dialog")),
        ]);
        let mut adapter = EngineAdapter::new();
        let (_, mounted) = adapter.mount_root(&mut root).unwrap();
        let bar_id = mounted.children[0].widget.unwrap();
        let body_id = mounted.children[4].widget.unwrap();

        adapter.store.set_viewport(Size::new(800.0, 600.0));
        let mut layout = LayoutEngine::new();
        let mut text = TextRenderer::new();
        let root_id = adapter.store.root().unwrap();
        // First layout builds the retained taffy graph (winit host path).
        sync_pre_layout(&mut adapter.store);
        layout.compute(
            &mut adapter.store,
            &mut text,
            root_id,
            Size::new(800.0, 600.0),
        );
        sync_post_layout(&mut adapter.store, &mut text);
        adapter.store.clear_layout_dirty();

        let bar_bounds = adapter.store.get(bar_id).unwrap().bounds();
        assert!(
            bar_bounds.height() > 0.0 && bar_bounds.height() < 48.0,
            "MenuBar should stay a thin strip, got {bar_bounds:?}"
        );
        let body_before = adapter.store.get(body_id).unwrap().bounds();
        assert!(
            body_before.height() > 40.0,
            "body should have content height before menu open, got {body_before:?}"
        );

        let click = Point::new(bar_bounds.x0 + 20.0, bar_bounds.y0 + 10.0);
        adapter.store.dispatch_event(&Event::PointerDown {
            id: 0,
            position: click,
            button: PointerButton::Primary,
            shift: false,
            ctrl: false,
        });
        // Same LayoutEngine instance — must not corrupt body via incremental sync
        // when the MenuBar overlay is inserted beside closed Dialog/Drawer.
        sync_pre_layout(&mut adapter.store);
        layout.compute(
            &mut adapter.store,
            &mut text,
            root_id,
            Size::new(800.0, 600.0),
        );
        sync_post_layout(&mut adapter.store, &mut text);

        let bar = adapter
            .store
            .get(bar_id)
            .unwrap()
            .as_any()
            .downcast_ref::<NativeMenuBar>()
            .unwrap();
        assert_eq!(bar.open_index, Some(0), "File menu should open");
        assert_eq!(bar.menus[0].items.len(), 3, "File should keep 3 items");

        let drawer_id = mounted.children[3].widget.unwrap();
        let drawer_open = adapter
            .store
            .get(drawer_id)
            .unwrap()
            .as_any()
            .downcast_ref::<synui_widgets::NavigationDrawer>()
            .unwrap()
            .open;
        assert!(
            !drawer_open,
            "MenuBar click must not open NavigationDrawer (blank 3-column look)"
        );

        let host = adapter
            .store
            .overlay_for_owner(bar_id)
            .expect("MenuBar must create an overlay popup");
        assert!(
            adapter.store.get(host).unwrap().as_any().is::<PopupHost>(),
            "overlay root should be PopupHost"
        );
        let host_bounds = adapter.store.get(host).unwrap().bounds();
        assert!(
            host_bounds.width() < 240.0 && host_bounds.height() < 160.0,
            "menu popup must stay compact, got {host_bounds:?}"
        );
        assert!(
            host_bounds.y1 < 200.0,
            "menu popup must stay near the MenuBar, got {host_bounds:?}"
        );

        let panel = adapter
            .store
            .children_of(host)
            .first()
            .copied()
            .expect("popup panel child");
        assert!(adapter.store.get(panel).unwrap().as_any().is::<MenuPanel>());
        let panel_bounds = adapter.store.get(panel).unwrap().bounds();
        assert!(
            panel_bounds.width() < 240.0 && panel_bounds.height() < 160.0,
            "MenuPanel must not expand into blank columns, got {panel_bounds:?}"
        );

        let body_after = adapter.store.get(body_id).unwrap().bounds();
        assert!(
            (body_after.height() - body_before.height()).abs() < 8.0,
            "opening MenuBar must not collapse body layout: before={body_before:?} after={body_after:?}"
        );
    }

    #[test]
    fn navigation_drawer_overlay_shows_buttons_and_scrim_dismisses() {
        use crate::widgets::{AppBar, Button, NavigationDrawer};
        use synui_core::{Event, Point, PointerButton, Size};
        use synui_layout::LayoutEngine;
        use synui_render::TextRenderer;
        use synui_widgets::{sync_post_layout, sync_pre_layout};

        let mut root = Node::column([
            AppBar::new("Nav")
                .trailing([Button::new("Drawer").node()])
                .node()
                .key(BusinessKey::new("app-bar")),
            NavigationDrawer::new([Button::new("List").node(), Button::new("Settings").node()])
                .open(true)
                .node()
                .key(BusinessKey::new("drawer")),
            Node::column([
                Node::text("Items"),
                Button::new("Item 1").node(),
                Button::new("Item 2").node(),
            ])
            .key(BusinessKey::new("body")),
        ]);
        let mut adapter = EngineAdapter::new();
        let (_, mounted) = adapter.mount_root(&mut root).unwrap();
        let drawer_id = mounted.children[1].widget.unwrap();
        let body_id = mounted.children[2].widget.unwrap();
        let list_id = mounted.children[1].children[0].widget.unwrap();

        adapter.store.set_viewport(Size::new(900.0, 600.0));
        let mut layout = LayoutEngine::new();
        let mut text = TextRenderer::new();
        let root_id = adapter.store.root().unwrap();
        sync_pre_layout(&mut adapter.store);
        layout.compute(
            &mut adapter.store,
            &mut text,
            root_id,
            Size::new(900.0, 600.0),
        );
        sync_post_layout(&mut adapter.store, &mut text);

        assert!(
            adapter.store.overlay_for_owner(drawer_id).is_some(),
            "open drawer must use modal overlay"
        );
        let rail = adapter.store.get(drawer_id).unwrap().bounds();
        assert!(
            rail.x0 < 8.0 && rail.width() <= 280.5,
            "left rail, got {rail:?}"
        );
        let list_b = adapter.store.get(list_id).unwrap().bounds();
        assert!(
            list_b.height() > 8.0 && list_b.width() > 8.0,
            "List button must be visible in the rail, got {list_b:?}"
        );
        let body_b = adapter.store.get(body_id).unwrap().bounds();
        assert!(
            body_b.height() > 40.0,
            "body must keep normal flow under the overlay, got {body_b:?}"
        );

        // Scrim click closes the drawer.
        assert!(
            adapter
                .store
                .dispatch_event(&Event::PointerDown {
                    id: 0,
                    position: Point::new(450.0, 300.0),
                    button: PointerButton::Primary,
                    shift: false,
                    ctrl: false,
                })
                .handled
        );
        sync_pre_layout(&mut adapter.store);
        let open = adapter
            .store
            .get(drawer_id)
            .unwrap()
            .as_any()
            .downcast_ref::<synui_widgets::NavigationDrawer>()
            .unwrap()
            .open;
        assert!(!open, "scrim click must close NavigationDrawer");
        assert!(adapter.store.overlay_for_owner(drawer_id).is_none());
    }

    #[test]
    fn node_styled_wraps_in_styled_host() {
        use crate::widgets::Styled;

        let wrapped = Node::column([Node::text("a"), Node::text("b")]).styled("padding:8;gap:4");
        assert_eq!(wrapped.kind(), "styled");
        let Node::Builtin(spec) = &wrapped else {
            panic!("expected builtin styled node");
        };
        assert_eq!(
            spec.properties().get("css").map(String::as_str),
            Some("padding:8;gap:4")
        );
        assert_eq!(spec.children().len(), 1);
        assert_eq!(spec.children()[0].kind(), "container");

        let col = Styled::column("padding:8;gap:12", [Node::text("x"), Node::text("y")]);
        let spec = col.describe(&mut BuildCx::new());
        assert_eq!(
            spec.properties().get("css").map(String::as_str),
            Some("flex-direction:column;padding:8;gap:12")
        );
        assert_eq!(spec.children().len(), 2);

        let row = Styled::row(
            "flex-direction:row;gap:4",
            [Node::text("l"), Node::text("r")],
        );
        let row_spec = row.describe(&mut BuildCx::new());
        assert_eq!(
            row_spec.properties().get("css").map(String::as_str),
            Some("flex-direction:row;gap:4"),
            "caller flex-direction must not be double-prefixed"
        );
    }
}
