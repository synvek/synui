//! Retained widget trait and supporting types.

use crate::event::{Event, EventCtx, EventResult};
use crate::geometry::{Constraints, Rect, Size};
use crate::IconPainter;
use crate::ImagePainter;
use crate::TextPainter;
use vello::Scene;

/// Opaque widget identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WidgetId(pub u64);

/// Accessibility properties exposed by widgets.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct A11yProps {
    pub role: A11yRole,
    pub label: Option<String>,
    pub value: Option<String>,
    /// UTF-8 byte selection into `value` when role is TextInput.
    pub text_selection: Option<std::ops::Range<usize>>,
    pub focusable: bool,
    pub actions: Vec<A11yAction>,
    pub disabled: bool,
    pub checked: Option<bool>,
    pub selected: Option<bool>,
    pub expanded: Option<bool>,
    pub read_only: bool,
    pub required: bool,
    pub invalid: bool,
    /// Logical row count for virtualized List/Grid. Not expanded into O(n) nodes.
    pub row_count: Option<usize>,
    /// Logical column count for Grid.
    pub col_count: Option<usize>,
    /// Mounted logical row index.
    pub row_index: Option<usize>,
    /// Mounted logical column index.
    pub col_index: Option<usize>,
}

/// Sparse declarative semantics layered over a widget's native accessibility defaults.
///
/// `None` means "preserve the native value"; an explicit value replaces it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct A11yOverride {
    pub role: Option<A11yRole>,
    pub label: Option<String>,
    pub value: Option<String>,
    pub focusable: Option<bool>,
    pub actions: Option<Vec<A11yAction>>,
    pub disabled: Option<bool>,
    pub checked: Option<bool>,
    pub selected: Option<bool>,
    pub expanded: Option<bool>,
    pub read_only: Option<bool>,
    pub required: Option<bool>,
    pub invalid: Option<bool>,
    pub row_count: Option<usize>,
    pub col_count: Option<usize>,
    pub row_index: Option<usize>,
    pub col_index: Option<usize>,
}

impl A11yOverride {
    pub fn merge(&self, mut native: A11yProps) -> A11yProps {
        if let Some(value) = self.role {
            native.role = value;
        }
        if let Some(value) = &self.label {
            native.label = Some(value.clone());
        }
        if let Some(value) = &self.value {
            native.value = Some(value.clone());
        }
        if let Some(value) = self.focusable {
            native.focusable = value;
        }
        if let Some(value) = &self.actions {
            native.actions = value.clone();
        }
        if let Some(value) = self.disabled {
            native.disabled = value;
        }
        if let Some(value) = self.checked {
            native.checked = Some(value);
        }
        if let Some(value) = self.selected {
            native.selected = Some(value);
        }
        if let Some(value) = self.expanded {
            native.expanded = Some(value);
        }
        if let Some(value) = self.read_only {
            native.read_only = value;
        }
        if let Some(value) = self.required {
            native.required = value;
        }
        if let Some(value) = self.invalid {
            native.invalid = value;
        }
        if let Some(value) = self.row_count {
            native.row_count = Some(value);
        }
        if let Some(value) = self.col_count {
            native.col_count = Some(value);
        }
        if let Some(value) = self.row_index {
            native.row_index = Some(value);
        }
        if let Some(value) = self.col_index {
            native.col_index = Some(value);
        }
        native
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum A11yRole {
    #[default]
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
    /// Virtualized data grid (DataTable).
    Grid,
    /// Grid row (mounted or logical proxy).
    Row,
    /// Grid cell.
    GridCell,
    Tab,
    Dialog,
    Status,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum A11yAction {
    Click,
    Focus,
    SetValue,
}

/// CSS grid track sizing (explicit templates only).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrackSize {
    Px(f32),
    Fr(f32),
    Auto,
}

/// CSS position for absolute/relative containing blocks (Phase 13+).
/// Sticky/Fixed added in Phase 19 (subset — not a full browser engine).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Position {
    #[default]
    Relative,
    Absolute,
    /// Vertical sticky within nearest clip ancestor (MVP: `top` inset only).
    Sticky,
    /// Viewport-fixed; does not scroll with ScrollView content.
    Fixed,
}

/// Layout style bridge to Taffy (subset used by widgets).
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutStyle {
    pub display: Display,
    pub flex_direction: FlexDirection,
    pub justify_content: JustifyContent,
    pub align_items: AlignItems,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub flex_basis: Dimension,
    pub width: Dimension,
    pub height: Dimension,
    pub min_width: Dimension,
    pub max_width: Dimension,
    pub min_height: Dimension,
    pub max_height: Dimension,
    /// `[top, right, bottom, left]`
    pub padding: [f32; 4],
    /// `[top, right, bottom, left]`
    pub margin: [f32; 4],
    /// Used for both axes when `row_gap` / `column_gap` are `None`.
    pub gap: f32,
    pub row_gap: Option<f32>,
    pub column_gap: Option<f32>,
    pub align_self: Option<AlignItems>,
    pub grid_template_columns: Vec<TrackSize>,
    pub grid_template_rows: Vec<TrackSize>,
    pub direction: LayoutDirection,
    pub position: Position,
    /// CSS inset `[top, right, bottom, left]` in px; `None` = auto.
    pub inset: [Option<f32>; 4],
    /// Paint/hit stacking order within a parent (Phase 19). `None` = document order (0).
    pub z_index: Option<i32>,
    /// When true, direct flex children keep content height (`flex-shrink: 0`) so a
    /// viewport can scroll instead of crushing content to fit.
    pub scroll_container: bool,
}

impl Default for LayoutStyle {
    fn default() -> Self {
        Self::base()
    }
}

impl LayoutStyle {
    /// Default for widget `layout_style()` tables (not `const` — grid track vecs).
    pub fn base() -> Self {
        Self {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::FlexStart,
            align_items: AlignItems::Stretch,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: Dimension::Auto,
            width: Dimension::Auto,
            height: Dimension::Auto,
            min_width: Dimension::Auto,
            max_width: Dimension::Auto,
            min_height: Dimension::Auto,
            max_height: Dimension::Auto,
            padding: [0.0; 4],
            margin: [0.0; 4],
            gap: 0.0,
            row_gap: None,
            column_gap: None,
            align_self: None,
            grid_template_columns: Vec::new(),
            grid_template_rows: Vec::new(),
            direction: LayoutDirection::Ltr,
            position: Position::Relative,
            inset: [None; 4],
            z_index: None,
            scroll_container: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Display {
    #[default]
    Flex,
    Grid,
    None,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FlexDirection {
    #[default]
    Row,
    Column,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum JustifyContent {
    #[default]
    FlexStart,
    Center,
    FlexEnd,
    SpaceBetween,
    SpaceAround,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AlignItems {
    #[default]
    Stretch,
    FlexStart,
    Center,
    FlexEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Dimension {
    #[default]
    Auto,
    Length(f32),
    Percent(f32),
    Fill,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LayoutDirection {
    #[default]
    Ltr,
    Rtl,
}

/// Paint context passed during rendering.
pub struct PaintCtx<'a> {
    pub bounds: Rect,
    pub theme: &'a ThemeRef,
    pub text: Option<&'a mut dyn TextPainter>,
    pub icons: Option<&'a mut dyn IconPainter>,
    pub images: Option<&'a mut dyn ImagePainter>,
    /// Host-preloaded Lottie JSON (`Window::with_lottie` → `LottieCache`).
    pub lotties: Option<&'a dyn crate::render::LottieJsonSource>,
    /// Phase 35: inherited text color from ancestor Styled cascade.
    pub text_color: Option<peniko::Color>,
    /// Phase 35: inherited font size from ancestor Styled cascade.
    pub font_size: Option<f32>,
}

/// Resolved theme metrics for widget painting.
#[derive(Debug, Clone, Copy)]
pub struct ThemeMetrics {
    pub corner_button: f64,
    pub corner_card: f64,
    pub corner_dialog: f64,
    pub corner_field: f64,
    pub corner_menu: f64,
    pub corner_switch: f64,
    pub ripple_enabled: bool,
    pub elevation_enabled: bool,
    /// Draw a subtle outline on elevated surfaces when elevation shadows are off.
    pub bordered_surfaces: bool,
}

impl Default for ThemeMetrics {
    fn default() -> Self {
        Self {
            corner_button: 16.0,
            corner_card: 12.0,
            corner_dialog: 16.0,
            corner_field: 4.0,
            corner_menu: 4.0,
            corner_switch: 10.0,
            ripple_enabled: true,
            elevation_enabled: true,
            bordered_surfaces: false,
        }
    }
}

/// Opaque theme reference (resolved by synui-theme).
#[derive(Debug, Clone)]
pub struct ThemeRef {
    pub surface: peniko::Color,
    pub on_surface: peniko::Color,
    pub primary: peniko::Color,
    pub on_primary: peniko::Color,
    pub outline: peniko::Color,
    pub secondary: peniko::Color,
    pub on_secondary: peniko::Color,
    pub surface_variant: peniko::Color,
    pub on_surface_variant: peniko::Color,
    pub surface_container: peniko::Color,
    pub error: peniko::Color,
    pub on_error: peniko::Color,
    pub is_dark: bool,
    pub metrics: ThemeMetrics,
    /// When true, widgets may use Cupertino morphology (Phase 9 Switch pilot).
    pub cupertino: bool,
}

impl ThemeRef {
    fn with_alpha(mut color: peniko::Color, alpha: f32) -> peniko::Color {
        color.components[3] = alpha.clamp(0.0, 1.0);
        color
    }

    /// Text selection highlight on field surfaces (TextField / TextArea / RichEdit).
    ///
    /// Dark surfaces need a stronger primary tint — fixed 28% alpha washes out on
    /// MD3/Fluent dark `surface` and becomes nearly invisible.
    pub fn selection_fill(&self) -> peniko::Color {
        Self::with_alpha(self.primary, if self.is_dark { 0.52 } else { 0.30 })
    }

    /// IME preedit / composition underline band (slightly stronger than selection).
    pub fn selection_ime_fill(&self) -> peniko::Color {
        Self::with_alpha(self.primary, if self.is_dark { 0.58 } else { 0.38 })
    }

    /// Progress / slider track behind the active indicator.
    ///
    /// Light mode previously used solid `outline`, which is close in value to a
    /// mid primary fill on a thin bar — the determinate segment disappears.
    /// A low-alpha primary rail keeps the solid primary fill readable in both modes.
    pub fn progress_track(&self) -> peniko::Color {
        if self.cupertino {
            return Self::with_alpha(self.outline, 0.28);
        }
        Self::with_alpha(self.primary, if self.is_dark { 0.30 } else { 0.20 })
    }

    /// Idle chrome border for text fields, lists, tables, and trees.
    ///
    /// Always-visible light outline so large empty surfaces read as controls;
    /// focus uses [`Self::primary`] (or a stronger tint) on top.
    pub fn field_border(&self) -> peniko::Color {
        Self::with_alpha(self.outline, if self.is_dark { 0.50 } else { 0.42 })
    }
}

impl Default for ThemeRef {
    fn default() -> Self {
        Self {
            surface: peniko::Color::from_rgb8(255, 251, 254),
            on_surface: peniko::Color::from_rgb8(28, 27, 31),
            primary: peniko::Color::from_rgb8(103, 80, 164),
            on_primary: peniko::Color::from_rgb8(255, 255, 255),
            outline: peniko::Color::from_rgb8(121, 116, 126),
            secondary: peniko::Color::from_rgb8(98, 91, 113),
            on_secondary: peniko::Color::from_rgb8(255, 255, 255),
            surface_variant: peniko::Color::from_rgb8(231, 224, 236),
            on_surface_variant: peniko::Color::from_rgb8(73, 69, 79),
            surface_container: peniko::Color::from_rgb8(243, 237, 247),
            error: peniko::Color::from_rgb8(179, 38, 30),
            on_error: peniko::Color::from_rgb8(255, 255, 255),
            is_dark: false,
            metrics: ThemeMetrics::default(),
            cupertino: false,
        }
    }
}

pub struct MeasureCtx<'a> {
    pub max_width: Option<f32>,
    pub text_measurer: Option<&'a mut dyn TextMeasurer>,
}

pub trait TextMeasurer {
    fn measure_text(&mut self, text: &str, font_size: f32, max_width: Option<f32>) -> Size;
}

pub struct LayoutCtx<'a> {
    pub widget_store: &'a WidgetStoreRef,
}

/// Lightweight reference to widget store for layout (avoids circular deps).
pub struct WidgetStoreRef;

pub struct A11yCtx;

/// Core retained widget trait.
pub trait Widget {
    fn id(&self) -> WidgetId;

    fn children(&self) -> &[WidgetId] {
        &[]
    }

    /// When true, the layout engine sizes this widget but does not assign child
    /// bounds — a host sync step places children (e.g. [`SplitPane`] primary/secondary).
    fn manages_child_layout(&self) -> bool {
        false
    }

    fn layout_style(&self) -> &LayoutStyle {
        use std::sync::OnceLock;
        static DEFAULT: OnceLock<LayoutStyle> = OnceLock::new();
        DEFAULT.get_or_init(LayoutStyle::base)
    }

    fn measure(&mut self, _ctx: &mut MeasureCtx, constraints: Constraints) -> Size {
        Size::new(
            constraints.max_width.min(100.0) as f64,
            constraints.max_height.min(32.0) as f64,
        )
    }

    fn layout(&mut self, _ctx: &mut LayoutCtx, bounds: Rect) {
        self.set_bounds(bounds);
    }

    fn bounds(&self) -> Rect;

    fn set_bounds(&mut self, bounds: Rect);

    fn paint(&self, ctx: &mut PaintCtx, scene: &mut Scene);

    /// Drawn after children, still inside [`clip_bounds`] when set (e.g. scrollbars).
    fn paint_overlay(&self, _ctx: &mut PaintCtx, _scene: &mut Scene) {}

    /// When `Some`, the store clips this widget's own paint **and** descendants
    /// to the rect (needed for scroll viewports that draw overflowing content).
    fn clip_bounds(&self) -> Option<Rect> {
        None
    }

    /// Phase 35: push inheritable visual tokens onto descendants during paint.
    /// Returns `(color, font_size)` when this host contributes to the cascade.
    fn paint_inherit(&self) -> Option<(Option<crate::ColorValue>, Option<f32>)> {
        None
    }

    fn event(&mut self, _ctx: &mut EventCtx, _event: &Event) -> EventResult {
        EventResult::IGNORED
    }

    fn a11y(&self) -> A11yProps {
        A11yProps::default()
    }

    fn is_disabled(&self) -> bool {
        false
    }

    fn is_focusable(&self) -> bool {
        false
    }

    fn hit_test(&self, point: crate::Point) -> bool {
        self.bounds().contains(point)
    }

    /// When true, hit-testing stops at this widget (e.g. scrollbar chrome over children).
    fn hit_test_prefer_self(&self, _point: crate::Point) -> bool {
        false
    }

    /// Optional hover tip text (engine widgets). Facade tips also use
    /// [`crate::WidgetStore::widget_tooltip`].
    fn tooltip(&self) -> Option<&str> {
        None
    }

    /// Platform cursor while the pointer is over this widget (or while it holds
    /// the pointer grab). `None` keeps the default arrow.
    fn pointer_cursor(&self) -> Option<PointerCursor> {
        None
    }
}

/// Mouse cursor shape requested by a widget (mapped to the platform cursor by the host).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PointerCursor {
    #[default]
    Default,
    Text,
    Pointer,
    /// Horizontal splitter / column resize.
    ColResize,
    /// Vertical splitter / row resize.
    RowResize,
    EwResize,
    NsResize,
    Grab,
    Grabbing,
}

/// Erased widget for storage.
pub trait ErasedWidget: Widget {
    fn as_any(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

impl<T: Widget + 'static> ErasedWidget for T {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
