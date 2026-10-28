//! Simplified CSS-subset style: layout (Taffy) + visual (paint).
//!
//! Phase 40 cascade v2: specificity, `!important`, `:hover`/`:focus`/`:disabled`,
//! and `@media (prefers-color-scheme)`. Still **not** a browser: no id selectors,
//! no `:nth-child` / `:not()`, no width media / container queries.

use crate::widget::{
    AlignItems, Dimension, Display, FlexDirection, JustifyContent, LayoutStyle, TrackSize,
};
use peniko::Color;
use std::hash::{Hash, Hasher};

/// Theme color token resolved at paint time against [`crate::ThemeRef`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeToken {
    Primary,
    OnPrimary,
    Secondary,
    OnSecondary,
    Surface,
    OnSurface,
    SurfaceVariant,
    OnSurfaceVariant,
    SurfaceContainer,
    Outline,
    Error,
    OnError,
}

/// Solid color or theme token.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColorValue {
    Rgba(Color),
    Token(ThemeToken),
}

impl ColorValue {
    pub fn resolve(self, theme: &crate::ThemeRef) -> Color {
        match self {
            ColorValue::Rgba(c) => c,
            ColorValue::Token(ThemeToken::Primary) => theme.primary,
            ColorValue::Token(ThemeToken::OnPrimary) => theme.on_primary,
            ColorValue::Token(ThemeToken::Secondary) => theme.secondary,
            ColorValue::Token(ThemeToken::OnSecondary) => theme.on_secondary,
            ColorValue::Token(ThemeToken::Surface) => theme.surface,
            ColorValue::Token(ThemeToken::OnSurface) => theme.on_surface,
            ColorValue::Token(ThemeToken::SurfaceVariant) => theme.surface_variant,
            ColorValue::Token(ThemeToken::OnSurfaceVariant) => theme.on_surface_variant,
            ColorValue::Token(ThemeToken::SurfaceContainer) => theme.surface_container,
            ColorValue::Token(ThemeToken::Outline) => theme.outline,
            ColorValue::Token(ThemeToken::Error) => theme.error,
            ColorValue::Token(ThemeToken::OnError) => theme.on_error,
        }
    }
}

/// Paint-only style; unset fields leave theme / widget defaults.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VisualStyle {
    pub background: Option<ColorValue>,
    pub border_radius: Option<f32>,
    pub opacity: Option<f32>,
    pub border_width: Option<f32>,
    pub border_color: Option<ColorValue>,
    /// Per-side borders (top / right / bottom / left). When set, they override the
    /// uniform [`Self::border_width`] / [`Self::border_color`] for that edge.
    pub border_top: Option<BorderSide>,
    pub border_right: Option<BorderSide>,
    pub border_bottom: Option<BorderSide>,
    pub border_left: Option<BorderSide>,
    /// Text / foreground color (Phase 35 inherit).
    pub color: Option<ColorValue>,
    /// Optional font size in px (Phase 35 inherit).
    pub font_size: Option<f32>,
}

/// One CSS border edge (`border-right: 1px solid #ccc`).
#[derive(Debug, Clone, PartialEq)]
pub struct BorderSide {
    pub width: f32,
    pub color: ColorValue,
}

impl VisualStyle {
    /// True when any uniform or per-side border is declared.
    pub fn has_border(&self) -> bool {
        self.border_width.is_some_and(|w| w > 0.0)
            || self.border_top.is_some()
            || self.border_right.is_some()
            || self.border_bottom.is_some()
            || self.border_left.is_some()
    }

    /// Resolved (width, color) for one edge. `side`: 0=top 1=right 2=bottom 3=left.
    pub fn border_side(&self, side: usize) -> Option<(f32, &ColorValue)> {
        let per = match side {
            0 => self.border_top.as_ref(),
            1 => self.border_right.as_ref(),
            2 => self.border_bottom.as_ref(),
            3 => self.border_left.as_ref(),
            _ => None,
        };
        if let Some(s) = per {
            if s.width > 0.0 {
                return Some((s.width, &s.color));
            }
            return None;
        }
        let w = self.border_width.filter(|w| *w > 0.0)?;
        let c = self.border_color.as_ref()?;
        Some((w, c))
    }

    /// True when at least one **per-side** border is set (not only uniform).
    pub fn has_per_side_border(&self) -> bool {
        self.border_top.is_some()
            || self.border_right.is_some()
            || self.border_bottom.is_some()
            || self.border_left.is_some()
    }
}

/// Aggregate style applied via `View::style` / `StyledBox`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Style {
    pub layout: LayoutStyle,
    pub visual: VisualStyle,
    /// Bitmask of visual fields declared with `!important` (Phase 40).
    pub important_visual: u16,
    /// Bitmask of layout fields declared with `!important` (Phase 40).
    pub important_layout: u32,
}

/// Resolved style after inheritance + sheet + local (Phase 35/40).
pub type ComputedStyle = Style;

/// Visual `!important` field bits.
pub mod vis_imp {
    pub const BACKGROUND: u16 = 1 << 0;
    pub const BORDER_RADIUS: u16 = 1 << 1;
    pub const OPACITY: u16 = 1 << 2;
    pub const BORDER_WIDTH: u16 = 1 << 3;
    pub const BORDER_COLOR: u16 = 1 << 4;
    pub const COLOR: u16 = 1 << 5;
    pub const FONT_SIZE: u16 = 1 << 6;
    pub const BORDER_TOP: u16 = 1 << 7;
    pub const BORDER_RIGHT: u16 = 1 << 8;
    pub const BORDER_BOTTOM: u16 = 1 << 9;
    pub const BORDER_LEFT: u16 = 1 << 10;
}

/// Layout `!important` field bits (subset of layout props).
pub mod lay_imp {
    pub const DISPLAY: u32 = 1 << 0;
    pub const FLEX_DIRECTION: u32 = 1 << 1;
    pub const JUSTIFY: u32 = 1 << 2;
    pub const ALIGN_ITEMS: u32 = 1 << 3;
    pub const FLEX_GROW: u32 = 1 << 4;
    pub const FLEX_SHRINK: u32 = 1 << 5;
    pub const WIDTH: u32 = 1 << 6;
    pub const HEIGHT: u32 = 1 << 7;
    pub const PADDING: u32 = 1 << 8;
    pub const MARGIN: u32 = 1 << 9;
    pub const GAP: u32 = 1 << 10;
    pub const POSITION: u32 = 1 << 11;
    pub const INSET: u32 = 1 << 12;
    pub const Z_INDEX: u32 = 1 << 13;
    pub const MIN_WIDTH: u32 = 1 << 14;
    pub const MIN_HEIGHT: u32 = 1 << 15;
    pub const MAX_WIDTH: u32 = 1 << 16;
    pub const MAX_HEIGHT: u32 = 1 << 17;
    pub const ROW_GAP: u32 = 1 << 18;
    pub const COLUMN_GAP: u32 = 1 << 19;
    pub const ALIGN_SELF: u32 = 1 << 20;
    pub const FLEX_BASIS: u32 = 1 << 21;
    pub const GRID_COLS: u32 = 1 << 22;
    pub const GRID_ROWS: u32 = 1 << 23;
}

/// Pseudo-class for stylesheet matching (Phase 40).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PseudoClass {
    #[default]
    None,
    Hover,
    Focus,
    Disabled,
}

/// Runtime interaction state for cascade resolve (Phase 40).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PseudoState {
    pub hover: bool,
    pub focus: bool,
    pub disabled: bool,
}

impl PseudoState {
    pub const fn matches(self, want: PseudoClass) -> bool {
        match want {
            PseudoClass::None => true,
            PseudoClass::Hover => self.hover,
            PseudoClass::Focus => self.focus,
            PseudoClass::Disabled => self.disabled,
        }
    }
}

/// Color-scheme media context (Phase 40 narrow `@media`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrefersColorScheme {
    #[default]
    Light,
    Dark,
}

impl PrefersColorScheme {
    pub fn from_dark(is_dark: bool) -> Self {
        if is_dark {
            Self::Dark
        } else {
            Self::Light
        }
    }
}

/// Optional prefers-color-scheme gate on a rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MediaScheme {
    #[default]
    Any,
    Light,
    Dark,
}

impl MediaScheme {
    pub fn matches(self, scheme: PrefersColorScheme) -> bool {
        match self {
            Self::Any => true,
            Self::Light => scheme == PrefersColorScheme::Light,
            Self::Dark => scheme == PrefersColorScheme::Dark,
        }
    }
}

/// Class selector: AND of classes + optional pseudo + optional ancestor class (descendant).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selector {
    /// Classes that must all be present on the element (`.a.b` ->?`["a","b"]`).
    pub classes: Vec<String>,
    /// If set, at least one ancestor must carry this class (`.a .b` ->?ancestor `a`, self `b`).
    pub ancestor_class: Option<String>,
    pub pseudo: PseudoClass,
}

impl Selector {
    pub fn class(name: impl Into<String>) -> Self {
        Self {
            classes: vec![name.into()],
            ancestor_class: None,
            pseudo: PseudoClass::None,
        }
    }

    /// Specificity `(0, class_count, 0)` ->?Phase 40: no id / type selectors.
    pub fn specificity(&self) -> u32 {
        let mut n = self.classes.len() as u32;
        if self.ancestor_class.is_some() {
            n = n.saturating_add(1);
        }
        if self.pseudo != PseudoClass::None {
            n = n.saturating_add(1);
        }
        n
    }

    pub fn matches(&self, classes: &[String], ancestors: &[String], pseudo: PseudoState) -> bool {
        if !pseudo.matches(self.pseudo) {
            return false;
        }
        if !self.classes.iter().all(|c| classes.iter().any(|x| x == c)) {
            return false;
        }
        if let Some(ref anc) = self.ancestor_class {
            if !ancestors.iter().any(|a| a == anc) {
                return false;
            }
        }
        true
    }
}

/// One stylesheet rule (Phase 40).
#[derive(Debug, Clone, PartialEq)]
pub struct StyleRule {
    pub selector: Selector,
    pub style: Style,
    pub source_order: u32,
    pub media: MediaScheme,
}

impl Style {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stable fingerprint for declarative `Styled` key derivation (Phase 25).
    pub fn fingerprint(&self) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        hash_layout(&self.layout, &mut h);
        hash_visual(&self.visual, &mut h);
        h.finish()
    }

    /// True when layout fields differ (triggers relayout).
    pub fn layout_eq(&self, other: &Style) -> bool {
        self.layout == other.layout
    }

    pub fn padding_all(mut self, v: f32) -> Self {
        self.layout.padding = [v; 4];
        self
    }

    pub fn padding(mut self, trbl: [f32; 4]) -> Self {
        self.layout.padding = trbl;
        self
    }

    pub fn margin_all(mut self, v: f32) -> Self {
        self.layout.margin = [v; 4];
        self
    }

    pub fn margin(mut self, trbl: [f32; 4]) -> Self {
        self.layout.margin = trbl;
        self
    }

    pub fn gap(mut self, v: f32) -> Self {
        self.layout.gap = v;
        self
    }

    pub fn width(mut self, d: Dimension) -> Self {
        self.layout.width = d;
        self
    }

    pub fn height(mut self, d: Dimension) -> Self {
        self.layout.height = d;
        self
    }

    pub fn flex_grow(mut self, v: f32) -> Self {
        self.layout.flex_grow = v;
        self
    }

    pub fn flex_shrink(mut self, v: f32) -> Self {
        self.layout.flex_shrink = v;
        self
    }

    pub fn flex_direction(mut self, d: FlexDirection) -> Self {
        self.layout.flex_direction = d;
        self
    }

    pub fn justify_content(mut self, j: JustifyContent) -> Self {
        self.layout.justify_content = j;
        self
    }

    pub fn align_items(mut self, a: AlignItems) -> Self {
        self.layout.align_items = a;
        self
    }

    pub fn background(mut self, c: ColorValue) -> Self {
        self.visual.background = Some(c);
        self
    }

    pub fn border_radius(mut self, r: f32) -> Self {
        self.visual.border_radius = Some(r);
        self
    }

    pub fn opacity(mut self, o: f32) -> Self {
        self.visual.opacity = Some(o);
        self
    }

    pub fn color(mut self, c: ColorValue) -> Self {
        self.visual.color = Some(c);
        self
    }

    pub fn font_size(mut self, px: f32) -> Self {
        self.visual.font_size = Some(px);
        self
    }

    /// Parse a CSS-subset declaration list. Unknown properties hard-fail.
    ///
    /// Supports trailing `!important` per declaration (Phase 40).
    /// Example: `padding:16;gap:8;align-items:center;background:#3366ff`
    pub fn from_css(input: &str) -> Result<Self, StyleParseError> {
        let mut style = Style::default();
        for raw in input.split(';') {
            let decl = raw.trim();
            if decl.is_empty() {
                continue;
            }
            let (prop, value) = decl
                .split_once(':')
                .ok_or_else(|| StyleParseError::InvalidDeclaration(decl.to_string()))?;
            let prop = prop.trim().to_ascii_lowercase();
            let (value, important) = split_important(value.trim());
            apply_css_prop(&mut style, &prop, value, important)?;
        }
        if style.layout.display == Display::Grid
            && style.layout.grid_template_columns.is_empty()
            && style.layout.grid_template_rows.is_empty()
        {
            return Err(StyleParseError::GridRequiresTemplate);
        }
        Ok(style)
    }

    /// Merge layout fields onto an existing widget layout (visual stays separate).
    pub fn merge_layout_into(&self, target: &mut LayoutStyle) {
        *target = self.layout.clone();
    }

    /// Inherit narrow visual tokens from `parent`, then overlay `sheet`, then `local`.
    ///
    /// Precedence (Phase 40): important layer beats normal; within a layer
    /// **local > sheet > inherited**. Layout does not inherit.
    pub fn cascade(parent: &Style, sheet: &Style, local: &Style) -> ComputedStyle {
        let mut out = Style::default();
        out.layout = LayoutStyle::default();
        merge_layout_level(
            &mut out.layout,
            &sheet.layout,
            sheet.important_layout,
            false,
        );
        merge_layout_level(
            &mut out.layout,
            &local.layout,
            local.important_layout,
            false,
        );
        merge_layout_level(&mut out.layout, &sheet.layout, sheet.important_layout, true);
        merge_layout_level(&mut out.layout, &local.layout, local.important_layout, true);

        out.visual = inherit_visual(&parent.visual);
        overlay_visual_level(
            &mut out.visual,
            &sheet.visual,
            sheet.important_visual,
            false,
        );
        overlay_visual_level(
            &mut out.visual,
            &local.visual,
            local.important_visual,
            false,
        );
        overlay_visual_level(&mut out.visual, &sheet.visual, sheet.important_visual, true);
        overlay_visual_level(&mut out.visual, &local.visual, local.important_visual, true);
        out
    }
}

/// Inherit only the Phase 35 narrow set (color, font_size, opacity).
pub fn inherit_visual(parent: &VisualStyle) -> VisualStyle {
    VisualStyle {
        background: None,
        border_radius: None,
        opacity: parent.opacity,
        border_width: None,
        border_color: None,
        border_top: None,
        border_right: None,
        border_bottom: None,
        border_left: None,
        color: parent.color,
        font_size: parent.font_size,
    }
}

fn split_important(value: &str) -> (&str, bool) {
    let v = value.trim();
    if let Some(rest) = v.strip_suffix("!important") {
        (rest.trim_end(), true)
    } else {
        (v, false)
    }
}

fn overlay_visual_level(dst: &mut VisualStyle, src: &VisualStyle, imp: u16, want_important: bool) {
    let take = |bit: u16| ((imp & bit) != 0) == want_important;
    if src.background.is_some() && take(vis_imp::BACKGROUND) {
        dst.background = src.background;
    }
    if src.border_radius.is_some() && take(vis_imp::BORDER_RADIUS) {
        dst.border_radius = src.border_radius;
    }
    if src.opacity.is_some() && take(vis_imp::OPACITY) {
        dst.opacity = src.opacity;
    }
    if src.border_width.is_some() && take(vis_imp::BORDER_WIDTH) {
        dst.border_width = src.border_width;
    }
    if src.border_color.is_some() && take(vis_imp::BORDER_COLOR) {
        dst.border_color = src.border_color;
    }
    if src.border_top.is_some() && take(vis_imp::BORDER_TOP) {
        dst.border_top = src.border_top.clone();
    }
    if src.border_right.is_some() && take(vis_imp::BORDER_RIGHT) {
        dst.border_right = src.border_right.clone();
    }
    if src.border_bottom.is_some() && take(vis_imp::BORDER_BOTTOM) {
        dst.border_bottom = src.border_bottom.clone();
    }
    if src.border_left.is_some() && take(vis_imp::BORDER_LEFT) {
        dst.border_left = src.border_left.clone();
    }
    if src.color.is_some() && take(vis_imp::COLOR) {
        dst.color = src.color;
    }
    if src.font_size.is_some() && take(vis_imp::FONT_SIZE) {
        dst.font_size = src.font_size;
    }
}

fn merge_layout_level(dst: &mut LayoutStyle, src: &LayoutStyle, imp: u32, want_important: bool) {
    let d = LayoutStyle::default();
    let take = |bit: u32| ((imp & bit) != 0) == want_important;
    // For normal layer: apply non-default fields that are NOT important.
    // For important layer: apply fields marked important (even if equal to default).
    if want_important {
        if take(lay_imp::DISPLAY) {
            dst.display = src.display;
        }
        if take(lay_imp::FLEX_DIRECTION) {
            dst.flex_direction = src.flex_direction;
        }
        if take(lay_imp::JUSTIFY) {
            dst.justify_content = src.justify_content;
        }
        if take(lay_imp::ALIGN_ITEMS) {
            dst.align_items = src.align_items;
        }
        if take(lay_imp::FLEX_GROW) {
            dst.flex_grow = src.flex_grow;
        }
        if take(lay_imp::FLEX_SHRINK) {
            dst.flex_shrink = src.flex_shrink;
        }
        if take(lay_imp::FLEX_BASIS) {
            dst.flex_basis = src.flex_basis.clone();
        }
        if take(lay_imp::WIDTH) {
            dst.width = src.width.clone();
        }
        if take(lay_imp::HEIGHT) {
            dst.height = src.height.clone();
        }
        if take(lay_imp::MIN_WIDTH) {
            dst.min_width = src.min_width.clone();
        }
        if take(lay_imp::MIN_HEIGHT) {
            dst.min_height = src.min_height.clone();
        }
        if take(lay_imp::MAX_WIDTH) {
            dst.max_width = src.max_width.clone();
        }
        if take(lay_imp::MAX_HEIGHT) {
            dst.max_height = src.max_height.clone();
        }
        if take(lay_imp::PADDING) {
            dst.padding = src.padding;
        }
        if take(lay_imp::MARGIN) {
            dst.margin = src.margin;
        }
        if take(lay_imp::GAP) {
            dst.gap = src.gap;
        }
        if take(lay_imp::ROW_GAP) {
            dst.row_gap = src.row_gap;
        }
        if take(lay_imp::COLUMN_GAP) {
            dst.column_gap = src.column_gap;
        }
        if take(lay_imp::ALIGN_SELF) {
            dst.align_self = src.align_self;
        }
        if take(lay_imp::POSITION) {
            dst.position = src.position;
        }
        if take(lay_imp::INSET) {
            dst.inset = src.inset;
        }
        if take(lay_imp::Z_INDEX) {
            dst.z_index = src.z_index;
        }
        if take(lay_imp::GRID_COLS) {
            dst.grid_template_columns = src.grid_template_columns.clone();
        }
        if take(lay_imp::GRID_ROWS) {
            dst.grid_template_rows = src.grid_template_rows.clone();
        }
        return;
    }
    // Normal: only non-default, non-important fields (legacy merge_layout_override).
    if src == &d {
        return;
    }
    if src.display != d.display && take(lay_imp::DISPLAY) {
        dst.display = src.display;
    }
    if src.flex_direction != d.flex_direction && take(lay_imp::FLEX_DIRECTION) {
        dst.flex_direction = src.flex_direction;
    }
    if src.justify_content != d.justify_content && take(lay_imp::JUSTIFY) {
        dst.justify_content = src.justify_content;
    }
    if src.align_items != d.align_items && take(lay_imp::ALIGN_ITEMS) {
        dst.align_items = src.align_items;
    }
    if src.flex_grow != d.flex_grow && take(lay_imp::FLEX_GROW) {
        dst.flex_grow = src.flex_grow;
    }
    if src.flex_shrink != d.flex_shrink && take(lay_imp::FLEX_SHRINK) {
        dst.flex_shrink = src.flex_shrink;
    }
    if src.flex_basis != d.flex_basis && take(lay_imp::FLEX_BASIS) {
        dst.flex_basis = src.flex_basis.clone();
    }
    if src.width != d.width && take(lay_imp::WIDTH) {
        dst.width = src.width.clone();
    }
    if src.height != d.height && take(lay_imp::HEIGHT) {
        dst.height = src.height.clone();
    }
    if src.min_width != d.min_width && take(lay_imp::MIN_WIDTH) {
        dst.min_width = src.min_width.clone();
    }
    if src.min_height != d.min_height && take(lay_imp::MIN_HEIGHT) {
        dst.min_height = src.min_height.clone();
    }
    if src.max_width != d.max_width && take(lay_imp::MAX_WIDTH) {
        dst.max_width = src.max_width.clone();
    }
    if src.max_height != d.max_height && take(lay_imp::MAX_HEIGHT) {
        dst.max_height = src.max_height.clone();
    }
    if src.padding != d.padding && take(lay_imp::PADDING) {
        dst.padding = src.padding;
    }
    if src.margin != d.margin && take(lay_imp::MARGIN) {
        dst.margin = src.margin;
    }
    if src.gap != d.gap && take(lay_imp::GAP) {
        dst.gap = src.gap;
    }
    if src.row_gap != d.row_gap && take(lay_imp::ROW_GAP) {
        dst.row_gap = src.row_gap;
    }
    if src.column_gap != d.column_gap && take(lay_imp::COLUMN_GAP) {
        dst.column_gap = src.column_gap;
    }
    if src.align_self != d.align_self && take(lay_imp::ALIGN_SELF) {
        dst.align_self = src.align_self;
    }
    if src.position != d.position && take(lay_imp::POSITION) {
        dst.position = src.position;
    }
    if src.inset != d.inset && take(lay_imp::INSET) {
        dst.inset = src.inset;
    }
    if src.z_index != d.z_index && take(lay_imp::Z_INDEX) {
        dst.z_index = src.z_index;
    }
    if src.grid_template_columns != d.grid_template_columns && take(lay_imp::GRID_COLS) {
        dst.grid_template_columns = src.grid_template_columns.clone();
    }
    if src.grid_template_rows != d.grid_template_rows && take(lay_imp::GRID_ROWS) {
        dst.grid_template_rows = src.grid_template_rows.clone();
    }
}

/// Class-based stylesheet (Phase 40): specificity, `!important`, pseudos, narrow `@media`.
#[derive(Debug, Clone, Default)]
pub struct StyleSheet {
    rules: Vec<StyleRule>,
    next_order: u32,
}

impl StyleSheet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn rule(mut self, class: impl Into<String>, style: Style) -> Self {
        let order = self.next_order;
        self.next_order = self.next_order.saturating_add(1);
        self.rules.push(StyleRule {
            selector: Selector::class(class),
            style,
            source_order: order,
            media: MediaScheme::Any,
        });
        self
    }

    pub fn push_rule(&mut self, rule: StyleRule) {
        let order = rule.source_order;
        self.rules.push(rule);
        self.next_order = self.next_order.max(order.saturating_add(1));
    }

    /// Parse a minimal sheet: `.name { ->?}`, `.a.b:hover { ->?}`, `.a .b { ->?}`,
    /// and `@media (prefers-color-scheme: dark|light) { ->?}` (Phase 40).
    pub fn from_css(input: &str) -> Result<Self, StyleParseError> {
        let mut sheet = StyleSheet::new();
        parse_sheet_rules(input, MediaScheme::Any, &mut sheet)?;
        Ok(sheet)
    }

    /// Merge matching rules (no ancestors / pseudo / light scheme).
    pub fn resolve(&self, classes: &[String]) -> Style {
        self.resolve_with(
            classes,
            &[],
            PseudoState::default(),
            PrefersColorScheme::Light,
        )
    }

    /// Phase 40: resolve with ancestor class path, pseudo state, and color scheme.
    pub fn resolve_with(
        &self,
        classes: &[String],
        ancestors: &[String],
        pseudo: PseudoState,
        scheme: PrefersColorScheme,
    ) -> Style {
        let mut matched: Vec<&StyleRule> = self
            .rules
            .iter()
            .filter(|r| r.media.matches(scheme) && r.selector.matches(classes, ancestors, pseudo))
            .collect();
        matched.sort_by(|a, b| {
            a.selector
                .specificity()
                .cmp(&b.selector.specificity())
                .then(a.source_order.cmp(&b.source_order))
        });

        let mut out = Style::default();
        for r in &matched {
            merge_layout_level(
                &mut out.layout,
                &r.style.layout,
                r.style.important_layout,
                false,
            );
            overlay_visual_level(
                &mut out.visual,
                &r.style.visual,
                r.style.important_visual,
                false,
            );
        }
        for r in &matched {
            merge_layout_level(
                &mut out.layout,
                &r.style.layout,
                r.style.important_layout,
                true,
            );
            overlay_visual_level(
                &mut out.visual,
                &r.style.visual,
                r.style.important_visual,
                true,
            );
        }
        out
    }

    /// True when any rule uses a pseudo-class (hosts may need hover/focus recompute).
    pub fn has_pseudo_rules(&self) -> bool {
        self.rules
            .iter()
            .any(|r| r.selector.pseudo != PseudoClass::None)
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    pub fn rules(&self) -> &[StyleRule] {
        &self.rules
    }
}

fn parse_sheet_rules(
    input: &str,
    media: MediaScheme,
    sheet: &mut StyleSheet,
) -> Result<(), StyleParseError> {
    let mut rest = input.trim();
    while !rest.is_empty() {
        if rest.starts_with('@') {
            let (inner_media, body, after) = parse_media_block(rest)?;
            parse_sheet_rules(body, inner_media, sheet)?;
            rest = after.trim_start();
            continue;
        }
        let Some(dot) = rest.find('.') else {
            if rest.chars().all(|c| c.is_whitespace()) {
                break;
            }
            return Err(StyleParseError::InvalidDeclaration(rest.to_string()));
        };
        rest = rest[dot..].trim_start();
        let brace = rest
            .find('{')
            .ok_or_else(|| StyleParseError::InvalidDeclaration(rest.to_string()))?;
        let selector_str = rest[..brace].trim();
        let after_brace = &rest[brace + 1..];
        let close = after_brace
            .find('}')
            .ok_or_else(|| StyleParseError::InvalidDeclaration(after_brace.to_string()))?;
        let body = after_brace[..close].trim();
        let style = Style::from_css(body)?;
        let selector = parse_selector(selector_str)?;
        let order = sheet.next_order;
        sheet.next_order = sheet.next_order.saturating_add(1);
        sheet.rules.push(StyleRule {
            selector,
            style,
            source_order: order,
            media,
        });
        rest = after_brace[close + 1..].trim_start();
    }
    Ok(())
}

fn parse_media_block(input: &str) -> Result<(MediaScheme, &str, &str), StyleParseError> {
    let rest = input.trim_start();
    let Some(after_at) = rest.strip_prefix("@media") else {
        return Err(StyleParseError::InvalidDeclaration(rest.to_string()));
    };
    let after_at = after_at.trim_start();
    let brace = after_at
        .find('{')
        .ok_or_else(|| StyleParseError::InvalidDeclaration(after_at.to_string()))?;
    let query = after_at[..brace].trim().to_ascii_lowercase();
    let media = if query.contains("prefers-color-scheme") && query.contains("dark") {
        MediaScheme::Dark
    } else if query.contains("prefers-color-scheme") && query.contains("light") {
        MediaScheme::Light
    } else {
        return Err(StyleParseError::UnsupportedMedia(query));
    };
    let body_start = &after_at[brace + 1..];
    let mut depth = 1usize;
    let mut idx = 0usize;
    let bytes = body_start.as_bytes();
    while idx < bytes.len() {
        match bytes[idx] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    let body = body_start[..idx].trim();
                    let after = &body_start[idx + 1..];
                    return Ok((media, body, after));
                }
            }
            _ => {}
        }
        idx += 1;
    }
    Err(StyleParseError::InvalidDeclaration(body_start.to_string()))
}

fn parse_selector(input: &str) -> Result<Selector, StyleParseError> {
    let s = input.trim();
    if s.is_empty() {
        return Err(StyleParseError::InvalidDeclaration(s.to_string()));
    }
    // Descendant: `.a .b` or `.a.b .c` ->?single space combinator only.
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() > 2 {
        return Err(StyleParseError::InvalidDeclaration(s.to_string()));
    }
    if parts.len() == 2 {
        let anc = parse_compound_selector(parts[0])?;
        if anc.pseudo != PseudoClass::None || anc.ancestor_class.is_some() || anc.classes.len() != 1
        {
            // Ancestor side: single class only (Phase 40).
            return Err(StyleParseError::InvalidDeclaration(s.to_string()));
        }
        let mut child = parse_compound_selector(parts[1])?;
        child.ancestor_class = Some(anc.classes[0].clone());
        return Ok(child);
    }
    parse_compound_selector(parts[0])
}

fn parse_compound_selector(input: &str) -> Result<Selector, StyleParseError> {
    let mut s = input.trim();
    let mut pseudo = PseudoClass::None;
    if let Some((head, tail)) = s.rsplit_once(':') {
        // Avoid treating `::` or urls ->?only trailing single pseudo.
        let p = tail.trim().to_ascii_lowercase();
        pseudo = match p.as_str() {
            "hover" => PseudoClass::Hover,
            "focus" => PseudoClass::Focus,
            "disabled" => PseudoClass::Disabled,
            _ => {
                return Err(StyleParseError::InvalidDeclaration(input.to_string()));
            }
        };
        s = head.trim();
    }
    if !s.starts_with('.') {
        return Err(StyleParseError::InvalidDeclaration(input.to_string()));
    }
    let mut classes = Vec::new();
    for part in s.split('.').filter(|p| !p.is_empty()) {
        if !part
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(StyleParseError::InvalidDeclaration(input.to_string()));
        }
        classes.push(part.to_string());
    }
    if classes.is_empty() {
        return Err(StyleParseError::InvalidDeclaration(input.to_string()));
    }
    Ok(Selector {
        classes,
        ancestor_class: None,
        pseudo,
    })
}

fn hash_f32(h: &mut impl Hasher, v: f32) {
    v.to_bits().hash(h);
}

fn hash_dimension(h: &mut impl Hasher, d: &Dimension) {
    match d {
        Dimension::Auto => 0u8.hash(h),
        Dimension::Length(v) => {
            1u8.hash(h);
            hash_f32(h, *v);
        }
        Dimension::Percent(v) => {
            2u8.hash(h);
            hash_f32(h, *v);
        }
        Dimension::Fill => 3u8.hash(h),
    }
}

fn hash_track(h: &mut impl Hasher, t: &TrackSize) {
    match t {
        TrackSize::Px(v) => {
            0u8.hash(h);
            hash_f32(h, *v);
        }
        TrackSize::Fr(v) => {
            1u8.hash(h);
            hash_f32(h, *v);
        }
        TrackSize::Auto => 2u8.hash(h),
    }
}

fn hash_color_value(h: &mut impl Hasher, c: &ColorValue) {
    match c {
        ColorValue::Rgba(color) => {
            0u8.hash(h);
            let [r, g, b, a] = color.to_rgba8().to_u8_array();
            r.hash(h);
            g.hash(h);
            b.hash(h);
            a.hash(h);
        }
        ColorValue::Token(t) => {
            1u8.hash(h);
            (*t as u8).hash(h);
        }
    }
}

fn hash_layout(layout: &LayoutStyle, h: &mut impl Hasher) {
    (layout.display as u8).hash(h);
    (layout.flex_direction as u8).hash(h);
    (layout.justify_content as u8).hash(h);
    (layout.align_items as u8).hash(h);
    hash_f32(h, layout.flex_grow);
    hash_f32(h, layout.flex_shrink);
    hash_dimension(h, &layout.flex_basis);
    hash_dimension(h, &layout.width);
    hash_dimension(h, &layout.height);
    hash_dimension(h, &layout.min_width);
    hash_dimension(h, &layout.max_width);
    hash_dimension(h, &layout.min_height);
    hash_dimension(h, &layout.max_height);
    for v in layout.padding {
        hash_f32(h, v);
    }
    for v in layout.margin {
        hash_f32(h, v);
    }
    hash_f32(h, layout.gap);
    match layout.row_gap {
        Some(v) => {
            1u8.hash(h);
            hash_f32(h, v);
        }
        None => 0u8.hash(h),
    }
    match layout.column_gap {
        Some(v) => {
            1u8.hash(h);
            hash_f32(h, v);
        }
        None => 0u8.hash(h),
    }
    match layout.align_self {
        Some(a) => {
            1u8.hash(h);
            (a as u8).hash(h);
        }
        None => 0u8.hash(h),
    }
    layout.grid_template_columns.len().hash(h);
    for t in &layout.grid_template_columns {
        hash_track(h, t);
    }
    layout.grid_template_rows.len().hash(h);
    for t in &layout.grid_template_rows {
        hash_track(h, t);
    }
    (layout.direction as u8).hash(h);
    (layout.position as u8).hash(h);
    for inset in layout.inset {
        match inset {
            Some(v) => {
                1u8.hash(h);
                hash_f32(h, v);
            }
            None => 0u8.hash(h),
        }
    }
    layout.z_index.hash(h);
    layout.scroll_container.hash(h);
}

fn hash_visual(visual: &VisualStyle, h: &mut impl Hasher) {
    match &visual.background {
        Some(c) => {
            1u8.hash(h);
            hash_color_value(h, c);
        }
        None => 0u8.hash(h),
    }
    match visual.border_radius {
        Some(v) => {
            1u8.hash(h);
            hash_f32(h, v);
        }
        None => 0u8.hash(h),
    }
    match visual.opacity {
        Some(v) => {
            1u8.hash(h);
            hash_f32(h, v);
        }
        None => 0u8.hash(h),
    }
    match visual.border_width {
        Some(v) => {
            1u8.hash(h);
            hash_f32(h, v);
        }
        None => 0u8.hash(h),
    }
    match &visual.border_color {
        Some(c) => {
            1u8.hash(h);
            hash_color_value(h, c);
        }
        None => 0u8.hash(h),
    }
    for side in [
        &visual.border_top,
        &visual.border_right,
        &visual.border_bottom,
        &visual.border_left,
    ] {
        match side {
            Some(s) => {
                1u8.hash(h);
                hash_f32(h, s.width);
                hash_color_value(h, &s.color);
            }
            None => 0u8.hash(h),
        }
    }
    match &visual.color {
        Some(c) => {
            1u8.hash(h);
            hash_color_value(h, c);
        }
        None => 0u8.hash(h),
    }
    match visual.font_size {
        Some(v) => {
            1u8.hash(h);
            hash_f32(h, v);
        }
        None => 0u8.hash(h),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StyleParseError {
    #[error("invalid declaration (expected property:value): {0}")]
    InvalidDeclaration(String),
    #[error("unknown property: {0}")]
    UnknownProperty(String),
    #[error("invalid value for {property}: {value}")]
    InvalidValue { property: String, value: String },
    #[error("display:grid requires grid-template-columns or grid-template-rows")]
    GridRequiresTemplate,
    #[error("unsupported @media query (Phase 40: prefers-color-scheme only): {0}")]
    UnsupportedMedia(String),
}

fn apply_css_prop(
    style: &mut Style,
    prop: &str,
    value: &str,
    important: bool,
) -> Result<(), StyleParseError> {
    let invalid = || StyleParseError::InvalidValue {
        property: prop.to_string(),
        value: value.to_string(),
    };
    let mark_v = |s: &mut Style, bit: u16| {
        if important {
            s.important_visual |= bit;
        }
    };
    let mark_l = |s: &mut Style, bit: u32| {
        if important {
            s.important_layout |= bit;
        }
    };
    match prop {
        "display" => {
            style.layout.display = parse_display(value).ok_or_else(invalid)?;
            mark_l(style, lay_imp::DISPLAY);
        }
        "flex-direction" => {
            style.layout.flex_direction = parse_flex_direction(value).ok_or_else(invalid)?;
            mark_l(style, lay_imp::FLEX_DIRECTION);
        }
        "justify-content" => {
            style.layout.justify_content = parse_justify(value).ok_or_else(invalid)?;
            mark_l(style, lay_imp::JUSTIFY);
        }
        "align-items" => {
            style.layout.align_items = parse_align(value).ok_or_else(invalid)?;
            mark_l(style, lay_imp::ALIGN_ITEMS);
        }
        "align-self" => {
            style.layout.align_self = Some(parse_align(value).ok_or_else(invalid)?);
            mark_l(style, lay_imp::ALIGN_SELF);
        }
        "flex-grow" => {
            style.layout.flex_grow = parse_f32(value).ok_or_else(invalid)?;
            mark_l(style, lay_imp::FLEX_GROW);
        }
        "flex-shrink" => {
            style.layout.flex_shrink = parse_f32(value).ok_or_else(invalid)?;
            mark_l(style, lay_imp::FLEX_SHRINK);
        }
        "flex-basis" => {
            style.layout.flex_basis = parse_dimension(value).ok_or_else(invalid)?;
            mark_l(style, lay_imp::FLEX_BASIS);
        }
        "width" => {
            style.layout.width = parse_dimension(value).ok_or_else(invalid)?;
            mark_l(style, lay_imp::WIDTH);
        }
        "height" => {
            style.layout.height = parse_dimension(value).ok_or_else(invalid)?;
            mark_l(style, lay_imp::HEIGHT);
        }
        "min-width" => {
            style.layout.min_width = parse_dimension(value).ok_or_else(invalid)?;
            mark_l(style, lay_imp::MIN_WIDTH);
        }
        "min-height" => {
            style.layout.min_height = parse_dimension(value).ok_or_else(invalid)?;
            mark_l(style, lay_imp::MIN_HEIGHT);
        }
        "max-width" => {
            style.layout.max_width = parse_dimension(value).ok_or_else(invalid)?;
            mark_l(style, lay_imp::MAX_WIDTH);
        }
        "max-height" => {
            style.layout.max_height = parse_dimension(value).ok_or_else(invalid)?;
            mark_l(style, lay_imp::MAX_HEIGHT);
        }
        "padding" => {
            style.layout.padding = parse_trbl(value).ok_or_else(invalid)?;
            mark_l(style, lay_imp::PADDING);
        }
        "margin" => {
            style.layout.margin = parse_trbl(value).ok_or_else(invalid)?;
            mark_l(style, lay_imp::MARGIN);
        }
        "gap" => {
            style.layout.gap = parse_length_px(value).ok_or_else(invalid)?;
            mark_l(style, lay_imp::GAP);
        }
        "row-gap" => {
            style.layout.row_gap = Some(parse_length_px(value).ok_or_else(invalid)?);
            mark_l(style, lay_imp::ROW_GAP);
        }
        "column-gap" => {
            style.layout.column_gap = Some(parse_length_px(value).ok_or_else(invalid)?);
            mark_l(style, lay_imp::COLUMN_GAP);
        }
        "position" => {
            style.layout.position = parse_position(value).ok_or_else(invalid)?;
            mark_l(style, lay_imp::POSITION);
        }
        "top" => {
            style.layout.inset[0] = Some(parse_length_px(value).ok_or_else(invalid)?);
            mark_l(style, lay_imp::INSET);
        }
        "right" => {
            style.layout.inset[1] = Some(parse_length_px(value).ok_or_else(invalid)?);
            mark_l(style, lay_imp::INSET);
        }
        "bottom" => {
            style.layout.inset[2] = Some(parse_length_px(value).ok_or_else(invalid)?);
            mark_l(style, lay_imp::INSET);
        }
        "left" => {
            style.layout.inset[3] = Some(parse_length_px(value).ok_or_else(invalid)?);
            mark_l(style, lay_imp::INSET);
        }
        "z-index" => {
            style.layout.z_index = parse_z_index(value).ok_or_else(invalid)?;
            mark_l(style, lay_imp::Z_INDEX);
        }
        "grid-template-columns" => {
            style.layout.grid_template_columns = parse_track_list(value).ok_or_else(invalid)?;
            mark_l(style, lay_imp::GRID_COLS);
        }
        "grid-template-rows" => {
            style.layout.grid_template_rows = parse_track_list(value).ok_or_else(invalid)?;
            mark_l(style, lay_imp::GRID_ROWS);
        }
        "background" | "background-color" => {
            style.visual.background = Some(parse_color(value).ok_or_else(invalid)?);
            mark_v(style, vis_imp::BACKGROUND);
        }
        "border-radius" => {
            style.visual.border_radius = Some(parse_length_px(value).ok_or_else(invalid)?);
            mark_v(style, vis_imp::BORDER_RADIUS);
        }
        "opacity" => {
            style.visual.opacity = Some(parse_f32(value).ok_or_else(invalid)?);
            mark_v(style, vis_imp::OPACITY);
        }
        "border-width" => {
            style.visual.border_width = Some(parse_length_px(value).ok_or_else(invalid)?);
            mark_v(style, vis_imp::BORDER_WIDTH);
        }
        "border-color" => {
            style.visual.border_color = Some(parse_color(value).ok_or_else(invalid)?);
            mark_v(style, vis_imp::BORDER_COLOR);
        }
        "border" => {
            let (w, c) = parse_border(value).ok_or_else(invalid)?;
            style.visual.border_width = Some(w);
            style.visual.border_color = Some(c.clone());
            // Shorthand also clears per-side so uniform stroke wins unless re-set.
            style.visual.border_top = None;
            style.visual.border_right = None;
            style.visual.border_bottom = None;
            style.visual.border_left = None;
            mark_v(
                style,
                vis_imp::BORDER_WIDTH
                    | vis_imp::BORDER_COLOR
                    | vis_imp::BORDER_TOP
                    | vis_imp::BORDER_RIGHT
                    | vis_imp::BORDER_BOTTOM
                    | vis_imp::BORDER_LEFT,
            );
        }
        "border-top" => {
            let (w, c) = parse_border(value).ok_or_else(invalid)?;
            style.visual.border_top = Some(BorderSide { width: w, color: c });
            mark_v(style, vis_imp::BORDER_TOP);
        }
        "border-right" => {
            let (w, c) = parse_border(value).ok_or_else(invalid)?;
            style.visual.border_right = Some(BorderSide { width: w, color: c });
            mark_v(style, vis_imp::BORDER_RIGHT);
        }
        "border-bottom" => {
            let (w, c) = parse_border(value).ok_or_else(invalid)?;
            style.visual.border_bottom = Some(BorderSide { width: w, color: c });
            mark_v(style, vis_imp::BORDER_BOTTOM);
        }
        "border-left" => {
            let (w, c) = parse_border(value).ok_or_else(invalid)?;
            style.visual.border_left = Some(BorderSide { width: w, color: c });
            mark_v(style, vis_imp::BORDER_LEFT);
        }
        "color" => {
            style.visual.color = Some(parse_color(value).ok_or_else(invalid)?);
            mark_v(style, vis_imp::COLOR);
        }
        "font-size" => {
            style.visual.font_size = Some(parse_length_px(value).ok_or_else(invalid)?);
            mark_v(style, vis_imp::FONT_SIZE);
        }
        _ => return Err(StyleParseError::UnknownProperty(prop.to_string())),
    }
    Ok(())
}

fn parse_f32(s: &str) -> Option<f32> {
    s.trim().parse().ok()
}

fn parse_length_px(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some(rest) = s.strip_suffix("px") {
        return rest.trim().parse().ok();
    }
    s.parse().ok()
}

fn parse_trbl(s: &str) -> Option<[f32; 4]> {
    let parts: Vec<f32> = s
        .split_whitespace()
        .map(parse_length_px)
        .collect::<Option<Vec<_>>>()?;
    match parts.as_slice() {
        [a] => Some([*a, *a, *a, *a]),
        [v, h] => Some([*v, *h, *v, *h]),
        [t, h, b] => Some([*t, *h, *b, *h]),
        [t, r, b, l] => Some([*t, *r, *b, *l]),
        _ => None,
    }
}

fn parse_dimension(s: &str) -> Option<Dimension> {
    let s = s.trim().to_ascii_lowercase();
    match s.as_str() {
        "auto" => Some(Dimension::Auto),
        "fill" | "100%" => Some(Dimension::Fill),
        _ if s.ends_with('%') => {
            let n: f32 = s.trim_end_matches('%').trim().parse().ok()?;
            Some(Dimension::Percent(n))
        }
        _ => parse_length_px(&s).map(Dimension::Length),
    }
}

fn parse_display(s: &str) -> Option<Display> {
    match s.trim().to_ascii_lowercase().as_str() {
        "flex" => Some(Display::Flex),
        "none" => Some(Display::None),
        "grid" => Some(Display::Grid),
        _ => None,
    }
}

fn parse_position(s: &str) -> Option<crate::Position> {
    match s.trim().to_ascii_lowercase().as_str() {
        "relative" => Some(crate::Position::Relative),
        "absolute" => Some(crate::Position::Absolute),
        "static" => Some(crate::Position::Relative),
        "sticky" => Some(crate::Position::Sticky),
        "fixed" => Some(crate::Position::Fixed),
        _ => None,
    }
}

fn parse_z_index(s: &str) -> Option<Option<i32>> {
    let v = s.trim();
    if v.eq_ignore_ascii_case("auto") {
        Some(None)
    } else {
        Some(Some(v.parse().ok()?))
    }
}

fn parse_track_list(s: &str) -> Option<Vec<TrackSize>> {
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }
    parts.into_iter().map(parse_track_size).collect()
}

fn parse_track_size(s: &str) -> Option<TrackSize> {
    let s = s.trim().to_ascii_lowercase();
    if s == "auto" {
        return Some(TrackSize::Auto);
    }
    if let Some(rest) = s.strip_suffix("fr") {
        let n: f32 = rest.trim().parse().ok()?;
        return Some(TrackSize::Fr(n));
    }
    parse_length_px(&s).map(TrackSize::Px)
}

fn parse_flex_direction(s: &str) -> Option<FlexDirection> {
    match s.trim().to_ascii_lowercase().as_str() {
        "row" => Some(FlexDirection::Row),
        "column" => Some(FlexDirection::Column),
        _ => None,
    }
}

fn parse_justify(s: &str) -> Option<JustifyContent> {
    match s.trim().to_ascii_lowercase().as_str() {
        "flex-start" | "start" => Some(JustifyContent::FlexStart),
        "center" => Some(JustifyContent::Center),
        "flex-end" | "end" => Some(JustifyContent::FlexEnd),
        "space-between" => Some(JustifyContent::SpaceBetween),
        "space-around" => Some(JustifyContent::SpaceAround),
        _ => None,
    }
}

fn parse_align(s: &str) -> Option<AlignItems> {
    match s.trim().to_ascii_lowercase().as_str() {
        "stretch" => Some(AlignItems::Stretch),
        "flex-start" | "start" => Some(AlignItems::FlexStart),
        "center" => Some(AlignItems::Center),
        "flex-end" | "end" => Some(AlignItems::FlexEnd),
        _ => None,
    }
}

fn parse_color(s: &str) -> Option<ColorValue> {
    let s = s.trim();
    let lower = s.to_ascii_lowercase();
    if let Some(token) = parse_theme_token(&lower) {
        return Some(ColorValue::Token(token));
    }
    if let Some(hex) = s.strip_prefix('#') {
        return parse_hex(hex).map(ColorValue::Rgba);
    }
    None
}

fn parse_theme_token(s: &str) -> Option<ThemeToken> {
    match s {
        "primary" => Some(ThemeToken::Primary),
        "on-primary" | "on_primary" => Some(ThemeToken::OnPrimary),
        "secondary" => Some(ThemeToken::Secondary),
        "on-secondary" | "on_secondary" => Some(ThemeToken::OnSecondary),
        "surface" => Some(ThemeToken::Surface),
        "on-surface" | "on_surface" => Some(ThemeToken::OnSurface),
        "surface-variant" | "surface_variant" => Some(ThemeToken::SurfaceVariant),
        "on-surface-variant" | "on_surface_variant" => Some(ThemeToken::OnSurfaceVariant),
        "surface-container" | "surface_container" => Some(ThemeToken::SurfaceContainer),
        "outline" => Some(ThemeToken::Outline),
        "error" => Some(ThemeToken::Error),
        "on-error" | "on_error" => Some(ThemeToken::OnError),
        _ => None,
    }
}

fn parse_hex(hex: &str) -> Option<Color> {
    let hex = hex.trim();
    match hex.len() {
        3 => {
            let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
            Some(Color::from_rgb8(r, g, b))
        }
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some(Color::from_rgb8(r, g, b))
        }
        8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
            Some(Color::from_rgba8(r, g, b, a))
        }
        _ => None,
    }
}

fn parse_border(s: &str) -> Option<(f32, ColorValue)> {
    let parts: Vec<&str> = s.split_whitespace().collect();
    match parts.as_slice() {
        [w, c] => Some((parse_length_px(w)?, parse_color(c)?)),
        [w, _style, c] => Some((parse_length_px(w)?, parse_color(c)?)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_css_padding_gap_align() {
        let s = Style::from_css("padding:16;gap:8;align-items:center").unwrap();
        assert_eq!(s.layout.padding, [16.0; 4]);
        assert_eq!(s.layout.gap, 8.0);
        assert_eq!(s.layout.align_items, AlignItems::Center);
    }

    #[test]
    fn from_css_unknown_property_fails() {
        let err = Style::from_css("padding:8;foo:1").unwrap_err();
        assert!(matches!(err, StyleParseError::UnknownProperty(_)));
    }

    #[test]
    fn from_css_trbl_and_percent() {
        let s = Style::from_css("margin:4 8;width:50%;min-width:100px").unwrap();
        assert_eq!(s.layout.margin, [4.0, 8.0, 4.0, 8.0]);
        assert_eq!(s.layout.width, Dimension::Percent(50.0));
        assert_eq!(s.layout.min_width, Dimension::Length(100.0));
    }

    #[test]
    fn from_css_visual_hex_and_token() {
        let s = Style::from_css("background:#3366ff;border-radius:12;opacity:0.5").unwrap();
        assert!(matches!(s.visual.background, Some(ColorValue::Rgba(_))));
        assert_eq!(s.visual.border_radius, Some(12.0));
        assert_eq!(s.visual.opacity, Some(0.5));
        let t = Style::from_css("background:primary").unwrap();
        assert_eq!(
            t.visual.background,
            Some(ColorValue::Token(ThemeToken::Primary))
        );
    }

    #[test]
    fn from_css_border_right_only() {
        let s = Style::from_css("border-right:1px solid #888888").unwrap();
        let side = s.visual.border_right.as_ref().expect("right");
        assert_eq!(side.width, 1.0);
        assert!(s.visual.border_left.is_none());
        assert!(s.visual.border_top.is_none());
        assert!(s.visual.border_bottom.is_none());
        assert!(s.visual.has_per_side_border());
        assert_eq!(
            s.visual.border_side(1).map(|(w, _)| w),
            Some(1.0)
        );
        assert!(s.visual.border_side(3).is_none());
    }

    #[test]
    fn from_css_grid_requires_template() {
        let err = Style::from_css("display:grid").unwrap_err();
        assert!(matches!(err, StyleParseError::GridRequiresTemplate));
    }

    #[test]
    fn from_css_grid_tracks() {
        let s = Style::from_css(
            "display:grid;grid-template-columns:1fr 1fr 100px;grid-template-rows:auto 40",
        )
        .unwrap();
        assert_eq!(s.layout.display, Display::Grid);
        assert_eq!(
            s.layout.grid_template_columns,
            vec![TrackSize::Fr(1.0), TrackSize::Fr(1.0), TrackSize::Px(100.0)]
        );
        assert_eq!(
            s.layout.grid_template_rows,
            vec![TrackSize::Auto, TrackSize::Px(40.0)]
        );
    }

    #[test]
    fn from_css_position_absolute_inset() {
        let s =
            Style::from_css("position:absolute;top:8;right:4;bottom:2;left:12;width:40;height:20")
                .unwrap();
        assert_eq!(s.layout.position, crate::Position::Absolute);
        assert_eq!(
            s.layout.inset,
            [Some(8.0), Some(4.0), Some(2.0), Some(12.0)]
        );
    }

    #[test]
    fn from_css_sticky_and_fixed_and_z() {
        let s = Style::from_css("position:sticky;top:8;z-index:3").unwrap();
        assert_eq!(s.layout.position, crate::Position::Sticky);
        assert_eq!(s.layout.inset[0], Some(8.0));
        assert_eq!(s.layout.z_index, Some(3));
        let f = Style::from_css("position:fixed;top:10;left:12;z-index:auto").unwrap();
        assert_eq!(f.layout.position, crate::Position::Fixed);
        assert_eq!(f.layout.z_index, None);
    }

    #[test]
    fn cascade_inherits_color_font_opacity() {
        let parent = Style::from_css("color:primary;font-size:18;opacity:0.8").unwrap();
        let child_local = Style::default();
        let computed = Style::cascade(&parent, &Style::default(), &child_local);
        assert_eq!(
            computed.visual.color,
            Some(ColorValue::Token(ThemeToken::Primary))
        );
        assert_eq!(computed.visual.font_size, Some(18.0));
        assert_eq!(computed.visual.opacity, Some(0.8));
        // Non-inherited fields stay unset.
        assert!(computed.visual.background.is_none());
    }

    #[test]
    fn cascade_local_wins_over_sheet_and_inherit() {
        let parent = Style::from_css("color:primary").unwrap();
        let sheet = Style::from_css("color:secondary;padding:8").unwrap();
        let local = Style::from_css("color:#112233").unwrap();
        let computed = Style::cascade(&parent, &sheet, &local);
        assert!(matches!(computed.visual.color, Some(ColorValue::Rgba(_))));
        assert_eq!(computed.layout.padding, [8.0; 4]);
    }

    #[test]
    fn stylesheet_class_rules_source_order() {
        let sheet = StyleSheet::from_css(
            ".card { padding:12;background:surface } .title { color:primary;font-size:20 }",
        )
        .unwrap();
        let s = sheet.resolve(&["card".into(), "title".into()]);
        assert_eq!(s.layout.padding, [12.0; 4]);
        assert_eq!(s.visual.color, Some(ColorValue::Token(ThemeToken::Primary)));
        assert_eq!(s.visual.font_size, Some(20.0));
        assert!(s.visual.background.is_some());
    }

    #[test]
    fn specificity_multi_class_beats_single() {
        let sheet = StyleSheet::from_css(".a { color:primary } .a.b { color:secondary }").unwrap();
        let s = sheet.resolve(&["a".into(), "b".into()]);
        assert_eq!(
            s.visual.color,
            Some(ColorValue::Token(ThemeToken::Secondary))
        );
    }

    #[test]
    fn important_beats_higher_specificity_normal() {
        let sheet =
            StyleSheet::from_css(".a.b { color:secondary } .a { color:primary !important }")
                .unwrap();
        let s = sheet.resolve(&["a".into(), "b".into()]);
        assert_eq!(s.visual.color, Some(ColorValue::Token(ThemeToken::Primary)));
    }

    #[test]
    fn local_important_beats_sheet_important() {
        let sheet = Style::from_css("color:primary !important").unwrap();
        let local = Style::from_css("color:#112233 !important").unwrap();
        let computed = Style::cascade(&Style::default(), &sheet, &local);
        assert!(matches!(computed.visual.color, Some(ColorValue::Rgba(_))));
    }

    #[test]
    fn later_same_specificity_wins() {
        let sheet =
            StyleSheet::from_css(".x { background:primary } .x { background:secondary }").unwrap();
        let s = sheet.resolve(&["x".into()]);
        assert_eq!(
            s.visual.background,
            Some(ColorValue::Token(ThemeToken::Secondary))
        );
    }

    #[test]
    fn media_prefers_color_scheme_dark() {
        let sheet = StyleSheet::from_css(
            ".card { color:primary } @media (prefers-color-scheme: dark) { .card { color:secondary } }",
        )
        .unwrap();
        let light = sheet.resolve_with(
            &["card".into()],
            &[],
            PseudoState::default(),
            PrefersColorScheme::Light,
        );
        let dark = sheet.resolve_with(
            &["card".into()],
            &[],
            PseudoState::default(),
            PrefersColorScheme::Dark,
        );
        assert_eq!(
            light.visual.color,
            Some(ColorValue::Token(ThemeToken::Primary))
        );
        assert_eq!(
            dark.visual.color,
            Some(ColorValue::Token(ThemeToken::Secondary))
        );
    }

    #[test]
    fn pseudo_hover_matches_only_when_hovered() {
        let sheet =
            StyleSheet::from_css(".btn { background:surface } .btn:hover { background:primary }")
                .unwrap();
        let idle = sheet.resolve_with(
            &["btn".into()],
            &[],
            PseudoState::default(),
            PrefersColorScheme::Light,
        );
        let hover = sheet.resolve_with(
            &["btn".into()],
            &[],
            PseudoState {
                hover: true,
                ..PseudoState::default()
            },
            PrefersColorScheme::Light,
        );
        assert_eq!(
            idle.visual.background,
            Some(ColorValue::Token(ThemeToken::Surface))
        );
        assert_eq!(
            hover.visual.background,
            Some(ColorValue::Token(ThemeToken::Primary))
        );
    }

    #[test]
    fn descendant_selector_needs_ancestor_class() {
        let sheet = StyleSheet::from_css(".wrap .item { color:primary }").unwrap();
        let miss = sheet.resolve_with(
            &["item".into()],
            &[],
            PseudoState::default(),
            PrefersColorScheme::Light,
        );
        let hit = sheet.resolve_with(
            &["item".into()],
            &["wrap".into()],
            PseudoState::default(),
            PrefersColorScheme::Light,
        );
        assert!(miss.visual.color.is_none());
        assert_eq!(
            hit.visual.color,
            Some(ColorValue::Token(ThemeToken::Primary))
        );
    }
}
