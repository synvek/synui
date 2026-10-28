//! Unified, non-panicking style input and diagnostics.

use std::panic::{catch_unwind, AssertUnwindSafe};

use synui_core::{Dimension, StyleParseError, TrackSize};

/// Stable facade style value. Its Engine representation is intentionally private.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Style(synui_core::Style);

impl Style {
    /// Creates an empty style.
    pub fn new() -> Self {
        Self(synui_core::Style::new())
    }

    /// Sets padding on all four sides in logical pixels.
    pub fn padding_all(mut self, value: f32) -> Self {
        self.0 = self.0.padding_all(value);
        self
    }

    /// Sets the flex gap in logical pixels.
    pub fn gap(mut self, value: f32) -> Self {
        self.0 = self.0.gap(value);
        self
    }

    /// Sets visual opacity.
    pub fn opacity(mut self, value: f32) -> Self {
        self.0 = self.0.opacity(value);
        self
    }

    pub(crate) fn from_engine(style: synui_core::Style) -> Self {
        Self(style)
    }

    pub(crate) fn into_engine(self) -> synui_core::Style {
        self.0
    }
}

/// Optional source location supplied with a runtime style declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceSpan {
    /// Zero-based byte offset where the input originated.
    pub start: usize,
    /// Exclusive zero-based byte offset where the input ended.
    pub end: usize,
}

/// Severity of a style diagnostic.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StyleSeverity {
    /// The declaration was rejected and the prior style was retained.
    Error,
}

/// Structured, non-panicking explanation of an invalid style input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StyleDiagnostic {
    /// Stable category for programmatic diagnostic handling.
    pub code: &'static str,
    /// Human-readable description of the failure.
    pub message: String,
    /// The invalid property or token.
    pub property_or_token: String,
    /// Original source position when the caller supplied one.
    pub source: Option<SourceSpan>,
    /// Diagnostic severity.
    pub severity: StyleSeverity,
}

impl StyleDiagnostic {
    fn invalid_value(
        property_or_token: impl Into<String>,
        message: impl Into<String>,
        source: Option<SourceSpan>,
    ) -> Self {
        Self {
            code: "invalid-value",
            message: message.into(),
            property_or_token: property_or_token.into(),
            source,
            severity: StyleSeverity::Error,
        }
    }
}

/// A style previously checked through the same parser and semantic rules as text input.
#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedStyle(Style);

impl ValidatedStyle {
    /// Validates text now and returns a reusable style input on success.
    pub fn from_text(text: impl AsRef<str>) -> Result<Self, StyleDiagnostic> {
        parse_text(text.as_ref(), None).map(Self)
    }

    /// Validates text now while retaining its original source position on failure.
    pub fn from_text_at(
        text: impl AsRef<str>,
        source: SourceSpan,
    ) -> Result<Self, StyleDiagnostic> {
        parse_text(text.as_ref(), Some(source)).map(Self)
    }

    /// Builds a macro-style input.
    ///
    /// The input is checked again by [`validate`] so direct construction cannot
    /// bypass the facade's semantic rules. The [`style!`] macro also performs
    /// its literal checks while compiling.
    #[doc(hidden)]
    pub fn from_style(style: Style) -> Self {
        Self(style)
    }
}

/// One supported source for a style declaration.
#[derive(Clone, Debug, PartialEq)]
pub enum StyleInput {
    /// A Rust builder style.
    Rust(Style),
    /// A style created by the facade macro or explicit validation API.
    Macro(ValidatedStyle),
    /// Runtime text parsed with the same syntax and semantic validator.
    Text {
        text: String,
        source: Option<SourceSpan>,
    },
}

impl StyleInput {
    /// Builds runtime text input without source-location metadata.
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text {
            text: text.into(),
            source: None,
        }
    }

    /// Builds runtime text input with source-location metadata.
    pub fn text_at(text: impl Into<String>, source: SourceSpan) -> Self {
        Self::Text {
            text: text.into(),
            source: Some(source),
        }
    }
}

/// Outcome of applying one input to an Element's style cache.
#[derive(Clone, Debug, PartialEq)]
pub struct StyleApplyResult {
    /// Whether a complete new style was applied.
    pub applied: bool,
    /// Rejection diagnostic; `None` for a successful application.
    pub diagnostic: Option<StyleDiagnostic>,
}

impl StyleApplyResult {
    /// Returns true for a handled style failure that retained the cached style.
    pub fn is_diagnostic_success(&self) -> bool {
        !self.applied && self.diagnostic.is_some()
    }
}

/// Last-successful-style cache owned by an Element host.
///
/// Each mounted Element host owns one cache. A host must retain this cache for
/// the Element's lifetime; sharing a cache between Elements intentionally shares
/// their last-successful style and is therefore not the normal integration path.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StyleCache {
    last_successful: Option<Style>,
}

impl StyleCache {
    /// Returns the last fully validated style, if one was applied.
    pub fn last_successful(&self) -> Option<&Style> {
        self.last_successful.as_ref()
    }

    /// Applies a replacement during an identity-preserving Element update.
    /// Invalid input retains the previous successful style and returns a
    /// structured diagnostic.
    pub fn update(&mut self, input: StyleInput) -> StyleApplyResult {
        self.apply(input)
    }

    /// Releases the host's cached style during Element unmount.
    pub fn unmount(&mut self) {
        self.last_successful = None;
    }

    /// Validates and atomically applies `input`.
    ///
    /// Invalid input is a diagnostic success rather than an error or panic: this
    /// cache keeps the preceding complete style unchanged and no partial style is
    /// observable by the Element host.
    pub fn apply(&mut self, input: StyleInput) -> StyleApplyResult {
        match validate(input) {
            Ok(style) => {
                self.last_successful = Some(style);
                StyleApplyResult {
                    applied: true,
                    diagnostic: None,
                }
            }
            Err(diagnostic) => StyleApplyResult {
                applied: false,
                diagnostic: Some(diagnostic),
            },
        }
    }
}

/// Canonical normalized style AST used by every facade input path.
///
/// Text is parsed into this AST, while Rust builders and macro output construct
/// the same normalized representation directly. Semantic checks are always run
/// after construction, before a cache can be changed.
#[derive(Clone, Debug, PartialEq)]
struct StyleAst(Style);

impl StyleAst {
    fn from_rust(style: Style) -> Self {
        Self(style)
    }

    fn from_text(text: &str, source: Option<SourceSpan>) -> Result<Self, StyleDiagnostic> {
        match catch_unwind(AssertUnwindSafe(|| synui_core::Style::from_css(text))) {
            Ok(Ok(style)) => Ok(Self(Style::from_engine(style))),
            Ok(Err(error)) => Err(diagnostic_for(error, source)),
            Err(_) => Err(StyleDiagnostic {
                code: "parser-panic",
                message: "style parser rejected the input without completing".into(),
                property_or_token: text.into(),
                source,
                severity: StyleSeverity::Error,
            }),
        }
    }

    fn validate(self, source: Option<SourceSpan>) -> Result<Style, StyleDiagnostic> {
        validate_semantics(&self.0, source)?;
        Ok(self.0)
    }
}

/// Validates any supported input with the facade's shared AST syntax and semantic rules.
pub fn validate(input: StyleInput) -> Result<Style, StyleDiagnostic> {
    let (ast, source) = match input {
        StyleInput::Rust(style) => (StyleAst::from_rust(style), None),
        StyleInput::Macro(style) => (StyleAst::from_rust(style.0), None),
        StyleInput::Text { text, source } => {
            let ast = StyleAst::from_text(&text, source.clone())?;
            (ast, source)
        }
    };
    ast.validate(source)
}

fn parse_text(text: &str, source: Option<SourceSpan>) -> Result<Style, StyleDiagnostic> {
    StyleAst::from_text(text, source.clone())?.validate(source)
}

fn validate_semantics(style: &Style, source: Option<SourceSpan>) -> Result<(), StyleDiagnostic> {
    let layout = &style.0.layout;
    validate_finite("flex-grow", [layout.flex_grow], source.clone())?;
    validate_finite("flex-shrink", [layout.flex_shrink], source.clone())?;
    validate_dimension("flex-basis", layout.flex_basis, source.clone())?;
    validate_dimension("width", layout.width, source.clone())?;
    validate_dimension("height", layout.height, source.clone())?;
    validate_dimension("min-width", layout.min_width, source.clone())?;
    validate_dimension("max-width", layout.max_width, source.clone())?;
    validate_dimension("min-height", layout.min_height, source.clone())?;
    validate_dimension("max-height", layout.max_height, source.clone())?;
    validate_finite("padding", layout.padding, source.clone())?;
    validate_non_negative("padding", layout.padding, source.clone())?;
    validate_finite("margin", layout.margin, source.clone())?;
    validate_finite("gap", [layout.gap], source.clone())?;
    validate_non_negative("gap", [layout.gap], source.clone())?;
    validate_optional_finite("row-gap", layout.row_gap, source.clone())?;
    validate_optional_non_negative("row-gap", layout.row_gap, source.clone())?;
    validate_optional_finite("column-gap", layout.column_gap, source.clone())?;
    validate_optional_non_negative("column-gap", layout.column_gap, source.clone())?;
    for inset in layout.inset {
        validate_optional_finite("inset", inset, source.clone())?;
    }
    for track in layout
        .grid_template_columns
        .iter()
        .chain(layout.grid_template_rows.iter())
    {
        match track {
            TrackSize::Px(value) => validate_finite("grid-template", [*value], source.clone())?,
            TrackSize::Fr(value) => validate_finite("grid-template", [*value], source.clone())?,
            TrackSize::Auto => {}
        }
    }

    let visual = &style.0.visual;
    validate_optional_finite("border-radius", visual.border_radius, source.clone())?;
    validate_optional_non_negative("border-radius", visual.border_radius, source.clone())?;
    validate_optional_finite("opacity", visual.opacity, source.clone())?;
    if let Some(opacity) = visual.opacity {
        if !(0.0..=1.0).contains(&opacity) {
            return Err(StyleDiagnostic::invalid_value(
                "opacity",
                "opacity must be a finite value from 0.0 through 1.0",
                source,
            ));
        }
    }
    validate_optional_finite("border-width", visual.border_width, source.clone())?;
    validate_optional_non_negative("border-width", visual.border_width, source.clone())?;
    for (name, side) in [
        ("border-top", &visual.border_top),
        ("border-right", &visual.border_right),
        ("border-bottom", &visual.border_bottom),
        ("border-left", &visual.border_left),
    ] {
        if let Some(side) = side {
            validate_optional_finite(name, Some(side.width), source.clone())?;
            validate_optional_non_negative(name, Some(side.width), source.clone())?;
        }
    }
    validate_optional_finite("font-size", visual.font_size, source.clone())?;
    validate_optional_non_negative("font-size", visual.font_size, source)?;
    Ok(())
}

fn validate_dimension(
    property: &str,
    dimension: Dimension,
    source: Option<SourceSpan>,
) -> Result<(), StyleDiagnostic> {
    match dimension {
        Dimension::Length(value) | Dimension::Percent(value) => {
            validate_finite(property, [value], source)
        }
        Dimension::Auto | Dimension::Fill => Ok(()),
    }
}

fn validate_optional_finite(
    property: &str,
    value: Option<f32>,
    source: Option<SourceSpan>,
) -> Result<(), StyleDiagnostic> {
    value.map_or(Ok(()), |value| validate_finite(property, [value], source))
}

fn validate_finite(
    property: &str,
    values: impl IntoIterator<Item = f32>,
    source: Option<SourceSpan>,
) -> Result<(), StyleDiagnostic> {
    if values.into_iter().all(f32::is_finite) {
        Ok(())
    } else {
        Err(StyleDiagnostic::invalid_value(
            property,
            format!("{property} must be finite"),
            source,
        ))
    }
}

fn validate_optional_non_negative(
    property: &str,
    value: Option<f32>,
    source: Option<SourceSpan>,
) -> Result<(), StyleDiagnostic> {
    value.map_or(Ok(()), |value| {
        validate_non_negative(property, [value], source)
    })
}

fn validate_non_negative(
    property: &str,
    values: impl IntoIterator<Item = f32>,
    source: Option<SourceSpan>,
) -> Result<(), StyleDiagnostic> {
    if values.into_iter().all(|value| value >= 0.0) {
        Ok(())
    } else {
        Err(StyleDiagnostic::invalid_value(
            property,
            format!("{property} cannot be negative"),
            source,
        ))
    }
}

fn diagnostic_for(error: StyleParseError, source: Option<SourceSpan>) -> StyleDiagnostic {
    let (code, property_or_token) = match &error {
        StyleParseError::InvalidDeclaration(token) => ("invalid-declaration", token.clone()),
        StyleParseError::UnknownProperty(property) => ("unknown-property", property.clone()),
        StyleParseError::InvalidValue { property, .. } => ("invalid-value", property.clone()),
        StyleParseError::GridRequiresTemplate => ("grid-requires-template", "display".into()),
        StyleParseError::UnsupportedMedia(token) => ("unsupported-media", token.clone()),
    };
    StyleDiagnostic {
        code,
        message: error.to_string(),
        property_or_token,
        source,
        severity: StyleSeverity::Error,
    }
}

/// Creates a compile-time validated facade style.
///
/// Literal values and property names are checked by the compiler before a
/// [`ValidatedStyle`] is constructed. This catches semantic errors such as an
/// out-of-range opacity even when the declaration grammar itself is valid.
/// Runtime text should use [`StyleInput::text`] instead.
///
/// ```compile_fail
/// use synui::style;
/// let _ = style!(opacity: 2.0);
/// ```
#[macro_export]
macro_rules! style {
    () => { $crate::style::ValidatedStyle::from_style($crate::style::Style::new()) };
    ($($property:ident : $value:expr);+ $(;)?) => {{
        let style = $crate::style::Style::new();
        $(let style = $crate::__synui_style_declaration!(style, $property: $value);)+
        $crate::style::ValidatedStyle::from_style(style)
    }};
    ($($invalid:tt)+) => {
        compile_error!("StyleDiagnostic { code: invalid-declaration, property_or_token: style! }")
    };
}

/// Implementation detail used by [`style!`].
#[doc(hidden)]
#[macro_export]
macro_rules! __synui_style_declaration {
    ($style:ident, padding: $value:expr) => {{
        const _: () = {
            let value = $value as f32;
            if !value.is_finite() || value < 0.0 {
                panic!("StyleDiagnostic {{ code: invalid-value, property_or_token: padding }}");
            }
        };
        $style.padding_all($value as f32)
    }};
    ($style:ident, gap: $value:expr) => {{
        const _: () = {
            let value = $value as f32;
            if !value.is_finite() || value < 0.0 {
                panic!("StyleDiagnostic {{ code: invalid-value, property_or_token: gap }}");
            }
        };
        $style.gap($value as f32)
    }};
    ($style:ident, opacity: $value:expr) => {{
        const _: () = {
            let value = $value as f32;
            if !value.is_finite() || value < 0.0 || value > 1.0 {
                panic!("StyleDiagnostic {{ code: invalid-value, property_or_token: opacity }}");
            }
        };
        $style.opacity($value as f32)
    }};
    ($style:ident, $property:ident: $value:expr) => {
        compile_error!(concat!(
            "StyleDiagnostic { code: unknown-property, property_or_token: ",
            stringify!($property),
            " }"
        ))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_text_preserves_the_last_successful_style_and_source() {
        let mut cache = StyleCache::default();
        assert!(cache.apply(StyleInput::text("padding:12;gap:4")).applied);
        let initial = cache.last_successful().cloned();
        let source = SourceSpan { start: 20, end: 35 };
        let result = cache.apply(StyleInput::text_at("not-a-property:1", source.clone()));

        assert!(result.is_diagnostic_success());
        let diagnostic = result.diagnostic.unwrap();
        assert_eq!(diagnostic.code, "unknown-property");
        assert_eq!(diagnostic.property_or_token, "not-a-property");
        assert_eq!(diagnostic.source, Some(source));
        assert_eq!(cache.last_successful(), initial.as_ref());
    }

    #[test]
    fn semantic_failures_share_the_ast_validator_and_do_not_commit() {
        let invalid_inputs = [
            StyleInput::Rust(Style::new().gap(-1.0)),
            StyleInput::Macro(ValidatedStyle::from_style(Style::new().opacity(2.0))),
            StyleInput::text("padding:-1"),
        ];

        for input in invalid_inputs {
            let mut cache = StyleCache::default();
            assert!(cache.apply(StyleInput::text("gap:4")).applied);
            let initial = cache.last_successful().cloned();
            let result = cache.apply(input);

            assert!(result.is_diagnostic_success());
            assert_eq!(result.diagnostic.unwrap().code, "invalid-value");
            assert_eq!(cache.last_successful(), initial.as_ref());
        }
    }

    #[test]
    fn rust_macro_and_text_inputs_share_a_single_apply_contract() {
        let mut cache = StyleCache::default();
        assert!(cache.apply(StyleInput::Rust(Style::new().gap(3.0))).applied);
        assert!(
            cache
                .apply(StyleInput::Macro(
                    crate::style!(padding: 8; gap: 2; opacity: 0.5)
                ))
                .applied
        );
        assert!(cache.apply(StyleInput::text("opacity:0.5")).applied);
        assert!(cache.last_successful().is_some());
    }
}
