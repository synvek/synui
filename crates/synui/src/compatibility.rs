//! Machine-checkable v0.1 to v0.2 compatibility metadata.

/// Source-level compatibility classification for a legacy facade entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompatibilityClassification {
    Preserve,
    DeprecatedWithReplacement,
    RemovedWithSteps,
}

/// Functional category used by the frozen v0.1 facade inventory.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacyFacadeCategory {
    AppStartup,
    State,
    Action,
    Binding,
    Identity,
    View,
    Style,
    Plugin,
    JsonView,
}

/// Compatibility boundary to which an entry belongs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompatibilitySurface {
    RustSource,
    ProductionAbi,
    ExperimentalPluginAbi,
    JsonViewCreation,
}

/// Allowed use of a non-production compatibility surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompatibilityPurpose {
    Experimental,
    Debugging,
    Preview,
    HotReload,
    Example,
    TestOnly,
}

/// One machine-checkable row in the complete v0.1 facade-entry inventory.
///
/// A row may represent a named entry family when v0.1 exposed a wildcard Engine
/// re-export. `entries` freezes every member or the exact wildcard boundary;
/// consumers must not infer additional v0.2 support from that old wildcard.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LegacyFacadeCompatibility {
    pub id: &'static str,
    pub category: LegacyFacadeCategory,
    pub entries: &'static [&'static str],
    pub classification: CompatibilityClassification,
    pub surface: CompatibilitySurface,
    pub replacement: Option<&'static str>,
    pub migration_steps: &'static str,
    pub feature_gate: Option<&'static str>,
    pub purposes: &'static [CompatibilityPurpose],
}

macro_rules! removed_entry {
    ($id:literal, $category:ident, [$($entry:literal),+ $(,)?], $replacement:literal, $steps:literal) => {
        LegacyFacadeCompatibility {
            id: $id,
            category: LegacyFacadeCategory::$category,
            entries: &[$($entry),+],
            classification: CompatibilityClassification::RemovedWithSteps,
            surface: CompatibilitySurface::RustSource,
            replacement: Some($replacement),
            migration_steps: $steps,
            feature_gate: Some("legacy"),
            purposes: &[],
        }
    };
}

macro_rules! deprecated_entry {
    ($id:literal, $category:ident, [$($entry:literal),+ $(,)?], $replacement:literal, $steps:literal) => {
        LegacyFacadeCompatibility {
            id: $id,
            category: LegacyFacadeCategory::$category,
            entries: &[$($entry),+],
            classification: CompatibilityClassification::DeprecatedWithReplacement,
            surface: CompatibilitySurface::RustSource,
            replacement: Some($replacement),
            migration_steps: $steps,
            feature_gate: Some("legacy"),
            purposes: &[],
        }
    };
}

/// Complete v0.1 facade inventory, grouped only where v0.1 itself exposed a
/// wildcard or one lifecycle mechanism shared a single migration.
///
/// This is the source of truth for the v0.2 migration guide. Removed entries
/// are absent from the default facade and are available, if at all, only behind
/// the transitional `legacy` feature.
pub const V01_FACADE_COMPATIBILITY: &[LegacyFacadeCompatibility] = &[
    removed_entry!(
        "app-bootstrap",
        AppStartup,
        ["synui::ViewFn", "synui::App<F>", "synui::run_app", "synui::run_app_signal"],
        "synui::app::App and synui::platform::{RunLoop, Window}",
        "Create App::new(AppConfig), create AppContext-owned resources, and return Node from App::run or RunLoop::run."
    ),
    removed_entry!(
        "app-engine-host",
        AppStartup,
        ["synui::Application", "synui::AppDriver", "synui::event_loop", "synui::RenderBackendKind"],
        "synui::app::App::run",
        "Move ordinary applications to App::run. Engine hosts must use an explicit integration crate; there is no direct curated-facade replacement for driver hooks."
    ),
    removed_entry!(
        "app-engine-hooks",
        AppStartup,
        ["App::with_frame_sync", "App::with_after_apply", "App::with_menu_handler", "App::with_back_handler", "App::engine", "App::engine_mut", "App::root_deps", "App::root_skip", "App::skipped_frames", "App::apply_only_frames"],
        "Element lifecycle, AppContext resources, and platform integration",
        "Move state synchronization to typed State/Action/InputConfig. Keep required Engine diagnostics in an explicit adapter; no one-to-one facade hook exists."
    ),
    removed_entry!(
        "global-signal",
        State,
        ["synui::Signal<T>", "synui::SignalId", "Signal::new", "Signal::get", "Signal::set", "Signal::set_noschedule"],
        "synui::app::{AppContext, State}",
        "Create state with AppContext::state and retain the returned State inside the owning Context."
    ),
    removed_entry!(
        "router-signal-state",
        State,
        ["synui::Route", "synui::Router<R>", "synui::take_pending_history_path"],
        "application-owned State and platform routing",
        "Store the current route in AppContext State and perform platform history integration outside the curated facade."
    ),
    removed_entry!(
        "global-actions",
        Action,
        ["synui::ActionId", "synui::register_action", "synui::register_action_alias", "synui::dispatch_action", "App::register_action", "View::on_action", "View::on_press"],
        "synui::app::Action and Element event handlers",
        "Create typed callbacks with AppContext::action or attach an Element closure; do not carry numeric or string action identifiers forward."
    ),
    deprecated_entry!(
        "scalar-bindings",
        Binding,
        ["synui::TextBinding", "synui::BoolBinding", "synui::NumberBinding"],
        "synui::widgets::InputConfig<T>",
        "Use TextInput, Checkbox, or NumberInput with InputConfig::controlled(State<T>, Action<T>) or InputConfig::uncontrolled."
    ),
    deprecated_entry!(
        "selection-binding",
        Binding,
        ["synui::SelectionBinding"],
        "synui::model::{SelectionChange, VirtualDataTableModel}",
        "Move table/list selection into an App-owned Model callback; Select choices use InputConfig<ChoiceId>."
    ),
    removed_entry!(
        "engine-bindings",
        Binding,
        ["synui::TextAreaBinding", "synui::RichBinding", "synui::TreeSelectionBinding", "synui::ColumnLayoutBinding", "synui::SplitRatioBinding", "synui::TabBinding", "synui::TableFilterBinding"],
        "AppContext State, Model callbacks, or an external Element",
        "Move values into State or Model callbacks. Components absent from v0.2 require an external Element; there is no lossless generic binding adapter."
    ),
    removed_entry!(
        "binding-registry",
        Binding,
        ["synui::register_*_binding", "synui::auto_pull_bindings*", "synui::auto_push_bindings*", "synui::binding_pull_stats", "synui::take_binding_pull_stats"],
        "typed InputConfig and Element lifecycle",
        "Delete global registration and WidgetStore synchronization; let controlled inputs dispatch typed Actions before the next render."
    ),
    removed_entry!(
        "legacy-forms",
        Binding,
        ["synui::FieldValidator", "synui::FormState"],
        "application validation over AppContext State",
        "Keep validation in application State or a component-local Model; no direct v0.2 form helper replacement is published."
    ),
    removed_entry!(
        "numeric-identity",
        Identity,
        ["synui::VIEW_KEY_BASE", "synui::reset_view_key_alloc", "synui::alloc_view_key", "synui::stable_styled_key", "View::with_key", "View::with_key_local", "View::keyed", "View::keyed_map", "View::rekey_from"],
        "Automatic Identity and synui::view::BusinessKey",
        "Remove numeric keys from static nodes. Attach a BusinessKey only to repeated siblings using a stable domain identifier."
    ),
    removed_entry!(
        "key-scope",
        Identity,
        ["synui::key_scope", "synui::mix_key", "synui::current_key_scope", "synui::resolve_key_local", "synui::reset_key_scope_tracking"],
        "Automatic Identity and synui::view::BusinessKey",
        "Delete key scopes and local numeric IDs. Duplicate sibling BusinessKeys now return ElementError::DuplicateBusinessKey."
    ),
    removed_entry!(
        "memo-section-scheduling",
        Identity,
        ["View::memo", "View::section", "View::section_fill", "synui::resolve_memo", "synui::memo_stats", "synui::take_memo_stats", "synui::force_rebuild_memos", "synui::clear_memo_cache", "synui::*section*", "synui::*keyed_cache*"],
        "automatic retained diffing inside EngineAdapter",
        "Delete public memo/section scheduling. Rebuild Nodes from State and allow retained identity/diff to preserve matching Elements."
    ),
    deprecated_entry!(
        "declarative-view",
        View,
        ["synui::View", "supported/conditional constructors in LEGACY_VIEW_COMPATIBILITY"],
        "synui::view::{Node, Element} and synui::widgets",
        "v0.3 removed adapt_legacy_view; rebuild with Node/Element / prelude widgets (see LEGACY_VIEW_COMPATIBILITY replacements)."
    ),
    removed_entry!(
        "unsupported-view-constructors",
        View,
        ["unsupported constructors in LEGACY_VIEW_COMPATIBILITY"],
        "external Element, VirtualDataTableModel, or platform integration",
        "Follow each LegacyViewCompatibility replacement/condition. Unsupported variants have no lossless adapter and must be rebuilt as v0.2 components."
    ),
    removed_entry!(
        "view-engine",
        View,
        ["synui::ViewEngine", "View::clone_structure"],
        "synui::view::{Element, Node}",
        "Use declarative Element lifecycle. Engine integration authors may opt into synui::advanced without exposing Engine storage to applications."
    ),
    deprecated_entry!(
        "fallible-view-style",
        Style,
        ["View::style", "View::try_style_css"],
        "synui::style::{StyleInput, StyleCache, StyleDiagnostic}",
        "Validate StyleInput and apply only successful ValidatedStyle values; inspect StyleApplyResult diagnostics."
    ),
    removed_entry!(
        "panic-and-class-style",
        Style,
        ["View::style_css", "View::class", "synui::StyleSheet", "synui::Theme", "synui::DesignSystem", "synui::design_system_from_config"],
        "synui::style::{StyleInput, StyleDiagnostic}",
        "Replace panic CSS parsing with StyleInput::Text. Theme/class/DesignSystem behavior has no direct curated-facade replacement and belongs in an explicit component or Engine integration."
    ),
    removed_entry!(
        "legacy-root-engine-reexports",
        View,
        ["synui_core::* via synui root", "synui_layout::* via synui root", "synui_render::* via synui root", "synui_theme::* via synui root", "synui_widgets::* via synui root", "synui_winit::* via synui root"],
        "synui curated modules or synui::advanced",
        "Import supported facade types from app/view/widgets/style/model/platform. Engine integrations must depend on the owning crate or explicit advanced boundary."
    ),
    LegacyFacadeCompatibility {
        id: "experimental-plugin-registry",
        category: LegacyFacadeCategory::Plugin,
        entries: &["synui::PluginIndexEntry", "synui::LabelFactory", "synui::ViewFactory", "synui::WidgetRegistry", "synui::WidgetPlugin", "synui::register_plugin", "synui::global_registry", "synui::plugin_styled_panel", "synui::plugin_view_page"],
        classification: CompatibilityClassification::Preserve,
        surface: CompatibilitySurface::ExperimentalPluginAbi,
        replacement: None,
        migration_steps: "Use only for experimental, preview, hot-reload, example, or test tooling; never treat it as a Production ABI contract.",
        feature_gate: Some("legacy + plugins"),
        purposes: &[CompatibilityPurpose::Experimental, CompatibilityPurpose::Preview, CompatibilityPurpose::HotReload, CompatibilityPurpose::Example, CompatibilityPurpose::TestOnly],
    },
    LegacyFacadeCompatibility {
        id: "experimental-plugin-loading",
        category: LegacyFacadeCategory::Plugin,
        entries: &["synui::load_plugin_index", "synui::load_plugins_from_index", "synui::load_plugins_from_index_opts", "native synui_plugin_* exports"],
        classification: CompatibilityClassification::Preserve,
        surface: CompatibilitySurface::ExperimentalPluginAbi,
        replacement: None,
        migration_steps: "Keep loading in development tooling. The versioned synui-abi header and SDKs intentionally exclude these symbols.",
        feature_gate: Some("legacy + plugins"),
        purposes: &[CompatibilityPurpose::Experimental, CompatibilityPurpose::Preview, CompatibilityPurpose::HotReload, CompatibilityPurpose::Example, CompatibilityPurpose::TestOnly],
    },
    LegacyFacadeCompatibility {
        id: "json-view-creation",
        category: LegacyFacadeCategory::JsonView,
        entries: &["synui::JsonPressFn", "synui::ViewJsonNode", "synui::parse_view_json", "synui::view_from_json"],
        classification: CompatibilityClassification::Preserve,
        surface: CompatibilitySurface::JsonViewCreation,
        replacement: None,
        migration_steps: "Restrict to debugging, preview, hot-reload, examples, and tests. Production applications and language SDKs must construct typed Elements through supported APIs.",
        feature_gate: Some("legacy"),
        purposes: &[CompatibilityPurpose::Debugging, CompatibilityPurpose::Preview, CompatibilityPurpose::HotReload, CompatibilityPurpose::Example, CompatibilityPurpose::TestOnly],
    },
];

/// Looks up one v0.1 facade entry family by stable manifest id.
pub fn facade_compatibility(id: &str) -> Option<&'static LegacyFacadeCompatibility> {
    V01_FACADE_COMPATIBILITY.iter().find(|entry| entry.id == id)
}

/// One published v0.2 compatibility boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReleaseBoundary {
    pub surface: CompatibilitySurface,
    pub stability: &'static str,
    pub supported_use: &'static str,
    pub production_contract: bool,
}

/// The four boundaries that must remain distinct throughout the v0.2 reset.
pub const RELEASE_BOUNDARIES: &[ReleaseBoundary] = &[
    ReleaseBoundary {
        surface: CompatibilitySurface::RustSource,
        stability: "intentional v0.2 source reset",
        supported_use: "curated Rust facade and explicitly labelled advanced integration",
        production_contract: true,
    },
    ReleaseBoundary {
        surface: CompatibilitySurface::ProductionAbi,
        stability: "versioned ABI v1.1",
        supported_use: "published C header and ABI-only language SDKs",
        production_contract: true,
    },
    ReleaseBoundary {
        surface: CompatibilitySurface::ExperimentalPluginAbi,
        stability: "experimental; no ABI stability promise",
        supported_use: "experimental, preview, hot-reload, example, and test tooling",
        production_contract: false,
    },
    ReleaseBoundary {
        surface: CompatibilitySurface::JsonViewCreation,
        stability: "non-production",
        supported_use: "debugging, preview, hot-reload, examples, and tests",
        production_contract: false,
    },
];

/// Explicitly deferred work outside the v0.2 release scope.
pub const V02_OUT_OF_SCOPE: &[&str] = &[
    "SynUI DSL",
    "DSL parser and formatter",
    "language server",
    "code generator",
    "live-preview tooling",
    "broad new component families",
];

/// Whether the private legacy View adapter accepts a variant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacyAdapterSupport {
    Supported,
    Conditional,
    Unsupported,
}

/// One machine-checkable compatibility row for a v0.1 `View` variant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LegacyViewCompatibility {
    pub variant: &'static str,
    pub classification: CompatibilityClassification,
    pub adapter_support: LegacyAdapterSupport,
    pub replacement: &'static str,
    pub conditions: &'static str,
    pub identity_policy: &'static str,
    pub binding_policy: &'static str,
    pub lifecycle_limit: &'static str,
    pub retirement_milestone: &'static str,
}

/// v0.3.0 Wave J: legacy View adapter and `feature = "legacy"` removed; table is migration documentation only.
pub const LEGACY_VIEW_ADAPTER_RETIREMENT: &str = "v0.3.0";

const IDENTITY: &str =
    "legacy u64 keys are ignored; v0.2 positional identity and BusinessKey rules apply";
const BINDING: &str =
    "snapshot only; legacy binding, section, memo, and signal registries are never read or written";
const LIFECYCLE: &str = "validate the complete tree before conversion; input snapshots are mount-only; unsupported updates must migrate to v0.2 Element APIs";

macro_rules! legacy_view {
    ($variant:literal, Supported, $replacement:literal, $conditions:literal) => {
        legacy_view!(@row $variant, Supported, DeprecatedWithReplacement, $replacement, $conditions)
    };
    ($variant:literal, Conditional, $replacement:literal, $conditions:literal) => {
        legacy_view!(@row $variant, Conditional, DeprecatedWithReplacement, $replacement, $conditions)
    };
    ($variant:literal, Unsupported, $replacement:literal, $conditions:literal) => {
        legacy_view!(@row $variant, Unsupported, RemovedWithSteps, $replacement, $conditions)
    };
    (@row $variant:literal, $support:ident, $classification:ident, $replacement:literal, $conditions:literal) => {
        LegacyViewCompatibility {
            variant: $variant,
            classification: CompatibilityClassification::$classification,
            adapter_support: LegacyAdapterSupport::$support,
            replacement: $replacement,
            conditions: $conditions,
            identity_policy: IDENTITY,
            binding_policy: BINDING,
            lifecycle_limit: LIFECYCLE,
            retirement_milestone: LEGACY_VIEW_ADAPTER_RETIREMENT,
        }
    };
}
/// Complete v0.1 declarative `View` variant manifest.
pub const LEGACY_VIEW_COMPATIBILITY: &[LegacyViewCompatibility] = &[
    legacy_view!("Label", Unsupported, "Node::text / widgets::Label", "adapter removed in v0.3; use Node::text or widgets::Label"),
    legacy_view!(
        "Button",
        Unsupported,
        "widgets::Button",
        "must be enabled and have no legacy closure or numeric ActionId"
    ),
    legacy_view!(
        "TextField",
        Unsupported,
        "widgets::TextInput",
        "enabled snapshot; subsequent value is native-owned"
    ),
    legacy_view!(
        "Icon",
        Unsupported,
        "widgets::Icon",
        "named glyph id; size optional"
    ),
    legacy_view!(
        "Checkbox",
        Unsupported,
        "widgets::Checkbox",
        "enabled snapshot; label becomes accessibility name"
    ),
    legacy_view!(
        "Switch",
        Unsupported,
        "widgets::Switch",
        "must not silently degrade to Checkbox; use widgets::Switch"
    ),
    legacy_view!(
        "Slider",
        Unsupported,
        "widgets::NumberInput",
        "enabled finite value and inclusive finite range"
    ),
    legacy_view!(
        "Image",
        Unsupported,
        "widgets::Image",
        "cache id + Window::with_image preload; fit/pinch remain host-owned"
    ),
    legacy_view!(
        "Card",
        Unsupported,
        "widgets::Card",
        "elevation + children Nodes"
    ),
    legacy_view!(
        "Dialog",
        Unsupported,
        "widgets::Dialog",
        "open State + on_dismiss Action; children application-composed"
    ),
    legacy_view!(
        "ListItem",
        Unsupported,
        "widgets::ListItem",
        "title/subtitle + optional trailing Nodes + Action<()>; not ActionId"
    ),
    legacy_view!(
        "AppBar",
        Unsupported,
        "widgets::AppBar",
        "title + optional trailing Nodes"
    ),
    legacy_view!(
        "Dropdown",
        Unsupported,
        "widgets::Select",
        "enabled non-empty choices and in-range selection"
    ),
    legacy_view!(
        "TabBar",
        Unsupported,
        "widgets::TabBar",
        "labels + selected index; pane content is application-composed"
    ),
    legacy_view!(
        "Radio",
        Unsupported,
        "widgets::Radio",
        "label + selected InputConfig<bool>; not a Switch"
    ),
    legacy_view!(
        "LinearProgress",
        Unsupported,
        "widgets::LinearProgress",
        "value 0..=1 or indeterminate; see also widgets::CircularProgress"
    ),
    legacy_view!(
        "MorphingShape",
        Unsupported,
        "widgets::MorphingShape",
        "prelude Element; size/playing/speed; continuous tick when playing"
    ),
    legacy_view!(
        "Lottie",
        Unsupported,
        "widgets::Lottie",
        "prelude Element; from_asset + Window::with_lottie, or from_json/new; playing/size"
    ),
    legacy_view!(
        "BottomNavigation",
        Unsupported,
        "widgets::BottomNavigation",
        "labels + selected index Action"
    ),
    legacy_view!(
        "NavigationDrawer",
        Unsupported,
        "widgets::NavigationDrawer",
        "open State + content Nodes; floating dock remains unsupported"
    ),
    legacy_view!(
        "BottomSheet",
        Unsupported,
        "widgets::BottomSheet",
        "open State + on_dismiss Action"
    ),
    legacy_view!(
        "SnackBar",
        Unsupported,
        "widgets::SnackBar",
        "visible State + message; auto-dismiss polls on_dismiss"
    ),
    legacy_view!(
        "TextArea",
        Unsupported,
        "widgets::TextArea",
        "InputConfig<String> multiline; label is a11y name only; TextAreaBinding removed"
    ),
    legacy_view!(
        "RichEdit",
        Unsupported,
        "widgets::RichDocument",
        "BlockDoc State/Action sync; no OT/HTML/nested tables"
    ),
    legacy_view!(
        "ListView",
        Unsupported,
        "widgets::VirtualList",
        "fixed item_extent or measure_heights; row labels via item(); select/activate Actions"
    ),
    legacy_view!(
        "Split",
        Unsupported,
        "widgets::SplitPane",
        "fixed two-pane nested split; floating dock remains unsupported"
    ),
    legacy_view!(
        "TreeView",
        Unsupported,
        "widgets::VirtualTree",
        "expand/select/activate Actions; item mount builder remains paint-path MVP"
    ),
    legacy_view!(
        "DataTable",
        Unsupported,
        "widgets::DataTable",
        "cell factory + select/sort/edit Actions; client substring filter; no server pivot"
    ),
    legacy_view!(
        "MenuBar",
        Unsupported,
        "widgets::MenuBar",
        "in-app overlay MenuBar + Action<String> item id; DesktopMenu/muda remains platform shell"
    ),
    legacy_view!(
        "ContextMenu",
        Unsupported,
        "widgets::ContextMenu",
        "in-app overlay ContextMenu + Action<String>; list secondary-click opens first mounted menu"
    ),
    legacy_view!(
        "Flex",
        Unsupported,
        "Node::container",
        "row/column only; finite non-negative gap and padding"
    ),
    legacy_view!(
        "Column",
        Unsupported,
        "Node::container",
        "finite non-negative gap and padding"
    ),
    legacy_view!(
        "Row",
        Unsupported,
        "Node::container",
        "finite non-negative gap and padding"
    ),
    legacy_view!(
        "Scroll",
        Unsupported,
        "widgets::Scroll",
        "full-mount children; sticky/fixed via widgets::Styled; transparent virt remains unsupported"
    ),
    legacy_view!(
        "VirtualScroll",
        Unsupported,
        "widgets::VirtualList",
        "legacy slot builder lifecycle is not adapted; use VirtualList for row virt"
    ),
    legacy_view!(
        "Styled",
        Unsupported,
        "style::StyleInput",
        "legacy Engine Style does not cross the facade boundary"
    ),
];

/// Looks up one legacy declarative variant by its stable manifest name.
pub fn legacy_view_compatibility(variant: &str) -> Option<&'static LegacyViewCompatibility> {
    LEGACY_VIEW_COMPATIBILITY
        .iter()
        .find(|entry| entry.variant == variant)
}

/// Why a legacy snapshot cannot enter the v0.2 Element lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum LegacyUnsupportedReason {
    #[error("the View variant is not in the adapter allowlist")]
    VariantNotSupported,
    #[error("numeric ActionId routing is not part of the v0.2 facade")]
    NumericActionId,
    #[error("legacy FnMut callbacks are not Send and cannot become Context-owned Actions")]
    NonSendCallback,
    #[error("the disabled state has no lossless mapping in this adapter")]
    DisabledState,
    #[error("a legacy value or range is invalid")]
    InvalidValue,
    #[error("legacy layout values are unsupported")]
    UnsupportedLayout,
}

/// Structured failure containing the child-index path to an unsupported legacy node.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[error("legacy View {variant} at child path {path:?} is unsupported: {reason}")]
pub struct LegacyViewError {
    pub path: Vec<usize>,
    pub variant: &'static str,
    pub reason: LegacyUnsupportedReason,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn facade_manifest_is_complete_and_machine_checkable() {
        assert!(!V01_FACADE_COMPATIBILITY.is_empty());
        let ids = V01_FACADE_COMPATIBILITY
            .iter()
            .map(|entry| entry.id)
            .collect::<HashSet<_>>();
        assert_eq!(ids.len(), V01_FACADE_COMPATIBILITY.len());
        let mut categories = Vec::new();
        for entry in V01_FACADE_COMPATIBILITY {
            assert!(
                !entry.entries.is_empty(),
                "{} has no legacy entries",
                entry.id
            );
            if entry.classification != CompatibilityClassification::Preserve {
                assert!(entry.replacement.is_some());
            }
            assert!(!entry.migration_steps.is_empty());
            if !categories.contains(&entry.category) {
                categories.push(entry.category);
            }
            match entry.classification {
                CompatibilityClassification::Preserve
                | CompatibilityClassification::DeprecatedWithReplacement
                | CompatibilityClassification::RemovedWithSteps => {}
            }
        }
        assert_eq!(categories.len(), 9);
        assert!(facade_compatibility("global-actions").is_some());
        assert!(facade_compatibility("does-not-exist").is_none());
    }

    #[test]
    fn release_boundaries_keep_production_and_experimental_surfaces_distinct() {
        assert_eq!(RELEASE_BOUNDARIES.len(), 4);
        let production = RELEASE_BOUNDARIES
            .iter()
            .filter(|boundary| boundary.production_contract)
            .count();
        assert_eq!(production, 2);
        assert!(RELEASE_BOUNDARIES.iter().any(|boundary| {
            boundary.surface == CompatibilitySurface::ExperimentalPluginAbi
                && !boundary.production_contract
        }));
        assert!(RELEASE_BOUNDARIES.iter().any(|boundary| {
            boundary.surface == CompatibilitySurface::JsonViewCreation
                && !boundary.production_contract
        }));
        assert!(!V02_FACADE_DENYLIST.is_empty());
        let denylist_symbols = V02_FACADE_DENYLIST
            .iter()
            .map(|entry| entry.symbol)
            .collect::<HashSet<_>>();
        assert_eq!(denylist_symbols.len(), V02_FACADE_DENYLIST.len());
        assert!(denylist_symbols.contains(&"WidgetStore"));
        assert!(denylist_symbols.contains(&"ViewEngine"));
        assert!(denylist_symbols.contains(&"plugin registry/loading symbols"));
        assert!(denylist_symbols.contains(&"JSON View creation symbols"));
        assert!(V02_FACADE_DENYLIST
            .iter()
            .all(|entry| !entry.boundary.is_empty()
                && !entry.replacement.is_empty()
                && !entry.reason.is_empty()));
    }

    #[test]
    fn every_legacy_view_row_has_lifecycle_and_migration_metadata() {
        assert_eq!(LEGACY_VIEW_ADAPTER_RETIREMENT, "v0.3.0");
        let mut variants = HashSet::new();
        for row in LEGACY_VIEW_COMPATIBILITY {
            assert!(variants.insert(row.variant));
            assert!(!row.replacement.is_empty());
            assert!(!row.conditions.is_empty());
            assert!(!row.identity_policy.is_empty());
            assert!(!row.binding_policy.is_empty());
            assert!(!row.lifecycle_limit.is_empty());
            assert_eq!(row.retirement_milestone, LEGACY_VIEW_ADAPTER_RETIREMENT);
            assert_eq!(row.adapter_support, LegacyAdapterSupport::Unsupported);
            assert_eq!(
                row.classification,
                CompatibilityClassification::RemovedWithSteps
            );
            assert_eq!(legacy_view_compatibility(row.variant), Some(row));
        }
    }
}

/// One explicit ordinary-facade denylist entry.
///
/// The list is an input to static migration checks. It does not remove or
/// restore any legacy implementation and does not describe Production ABI
/// exports.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FacadeDenylistEntry {
    pub symbol: &'static str,
    pub boundary: &'static str,
    pub replacement: &'static str,
    pub reason: &'static str,
}

/// Symbols that must remain outside the ordinary Prelude and migrated-example
/// proof boundary. Advanced adapters may depend on their owning Engine crate,
/// but ordinary applications must use the documented replacement.
pub const V02_FACADE_DENYLIST: &[FacadeDenylistEntry] = &[
    FacadeDenylistEntry {
        symbol: "WidgetStore",
        boundary: "advanced Engine integration",
        replacement: "synui::view::{Node, Element, ElementSpec}",
        reason: "Engine storage is not an application-owned facade contract.",
    },
    FacadeDenylistEntry {
        symbol: "ViewEngine",
        boundary: "retained Engine integration",
        replacement: "synui::platform::{Window, RunLoop} and Element lifecycle",
        reason: "ViewEngine downcast and retained storage are not Prelude APIs.",
    },
    FacadeDenylistEntry {
        symbol: "Engine layout/render types",
        boundary: "Engine/platform boundary",
        replacement: "synui::{view, widgets, style, model, platform}",
        reason: "synui-core/layout/render/theme/winit internals remain explicit boundaries.",
    },
    FacadeDenylistEntry {
        symbol: "numeric action identifiers",
        boundary: "removed legacy source",
        replacement: "synui::app::Action and synui::view::Event",
        reason: "Typed App-owned Actions replace numeric global routing.",
    },
    FacadeDenylistEntry {
        symbol: "global key/memo/section scheduling",
        boundary: "removed legacy identity/scheduling",
        replacement: "automatic identity and synui::view::BusinessKey",
        reason: "Global registries and numeric keys are not stable lifecycle semantics.",
    },
    FacadeDenylistEntry {
        symbol: "plugin registry/loading symbols",
        boundary: "ExperimentalPluginAbi",
        replacement: "typed facade components for production; explicit plugin tooling otherwise",
        reason: "Plugins are experimental/example/test-only and excluded from Prelude and Production ABI.",
    },
    FacadeDenylistEntry {
        symbol: "JSON View creation symbols",
        boundary: "JsonViewCreation",
        replacement: "typed synui::view::Element/ElementSpec",
        reason: "JSON construction is non-production debugging/preview/example tooling.",
    },
];
