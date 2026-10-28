//! SynUI v0.2 curated Rust facade.
//!
//! Supported application code imports [`prelude`] or the documented [`app`],
//! [`view`], [`widgets`], [`mod@style`], [`model`], and [`platform`] modules.
//! Engine details are not re-exported from this crate root.
//!
//! # API stability
//!
//! The seven documented facade modules are the **stable public API**. The
//! explicit [`advanced`] module is an **advanced Engine integration** boundary:
//! it is intentionally opt-in and excluded from [`prelude`]. The complete v0.1
//! source inventory, migration steps, non-production purpose labels, and four
//! release boundaries are exposed by [`compatibility`]; the human-readable
//! guide is `docs/migration-v0.2.md` in the workspace.
//!
//! # Legacy API boundary
//!
//! As of v0.3 (Phase 51 Wave J), the `legacy` feature and v0.1 root re-exports
//! (`View`, `Signal`, bindings, `run_app`, …) are **removed**. Phase 52 deleted
//! the in-tree gold archive. Ordinary evidence is `examples/*` + facade gates.
//! Migration metadata remains in [`compatibility`].
//!
//! ```compile_fail
//! use synui::prelude::*;
//! let _ = synui::key_scope;
//! ```
//!
//! ```compile_fail
//! use synui::prelude::*;
//! let _ = synui::section;
//! let _ = synui::section_fill;
//! ```
//!
//! ```compile_fail
//! use synui::prelude::*;
//! let _ = synui::memo::schedule;
//! ```
//!
//! ```compile_fail
//! use synui::prelude::*;
//! let _ = synui::ActionId::default();
//! ```
//!
//! ```compile_fail
//! use synui::prelude::*;
//! let _ = synui::plugins::Plugin;
//! ```
//!
//! ```compile_fail
//! use synui::prelude::*;
//! let _ = synui::view_from_json;
//! ```
//!
//! ```compile_fail
//! use synui::prelude::*;
//! let _: WidgetStore;
//! ```

/// **Stable migration metadata:** machine-checkable v0.1 `View` compatibility policy.
pub mod compatibility;
mod error;
mod runtime;

/// **Stable public API:** App-context creation, resource ownership, and errors.
pub mod app;
/// **Stable public API:** App-owned model contracts.
pub mod model;
/// **Stable public API:** platform-facing scheduling services.
pub mod platform;
/// **Stable public API:** non-panicking style input and diagnostic APIs.
pub mod style;
/// **Stable public API:** declarative node construction and event-handler contracts.
pub mod view;
/// **Stable public API:** built-in widget descriptors.
pub mod widgets;

/// **Advanced Engine integration.** Explicit opt-in Engine types for adapters.
///
/// These types are intentionally excluded from [`prelude`]. Their storage and
/// lifecycle are Engine concerns, not part of ordinary application code.
pub mod advanced {
    /// **Advanced Engine integration:** Engine widget identifier.
    pub use synui_core::WidgetId;
    /// **Advanced Engine integration:** widget-tree storage for Engine adapters.
    pub use synui_core::WidgetStore;
}

/// **Stable public API:** common v0.2 application imports.
///
/// This intentionally excludes every [`advanced`] Engine type.
pub mod prelude {
    pub use crate::app::{
        Action, App, AppConfig, AppContext, AppError, ContextId, LifecycleState, State,
    };
    pub use crate::model::{
        CellCoordinate, CellRange, CellRequest, CellValue, Model, ModelHandle, PrefetchBand,
        SelectionChange, SortDescriptor, SortDirection, TableDimensions, VirtualDataTableAdapter,
        VirtualDataTableModel, VirtualModelError,
    };
    pub use crate::platform::{PlatformError, RunLoop, UiPost, UiPostStatus, UiPostToken, Window};
    pub use crate::style;
    pub use crate::style::{
        SourceSpan, Style, StyleApplyResult, StyleCache, StyleDiagnostic, StyleInput,
        StyleSeverity, ValidatedStyle,
    };
    pub use crate::view::{
        Accessibility, AccessibilityAction, AccessibilityActionRequest, AccessibilityRole,
        AccessibilityRoleError, AccessibilityState, BuildCx, BusinessKey, ContainerLayout, Element,
        ElementError, ElementKind, ElementSpec, Event, EventContext, EventHandler, EventSlot,
        IdentityPath, Key, Mounted, Node,
    };
    pub use crate::widgets::{
        AppBar, BottomNavigation, BottomSheet, Button, Card, Checkbox, ChoiceId, CircularProgress,
        ContextMenu, DataTable, Dialog, Icon, Image, InputConfig, InputConfigError, InputKind,
        InputValue, Label, LinearProgress, ListItem, Lottie, Menu, MenuBar, MenuItem,
        MorphingShape, NavigationDrawer, NumberInput, Radio, RichDocument, Scroll, Select,
        SnackBar, SplitDirection, SplitPane, Styled, Switch, TabBar, TableColumn, TextArea,
        TextInput, TreeNode, VirtualList, VirtualTree,
    };
    pub use synui_core::{Block, BlockDoc, TextSpan};
}

/// One canonical v0.2 facade export used by API documentation and migration
/// checks. This index describes application-level names only; Engine and
/// experimental symbols are recorded separately in [`FACADE_DENYLIST`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FacadeExport {
    /// Stable module path consumers should import.
    pub canonical_path: &'static str,
    /// Public type or constructor name at that path.
    pub type_name: &'static str,
    /// Whether the item is part of [`prelude`].
    pub prelude: bool,
    /// Public stability label.
    pub stability: &'static str,
    /// Advanced/legacy replacement or boundary note.
    pub advanced_replacement: &'static str,
    /// Legacy mapping id, when the name replaces a v0.1 family.
    pub legacy_mapping: Option<&'static str>,
}

/// Canonical stable export index. Keep this list aligned with `prelude` and
/// `docs/api-reference-v0.2.md`; it intentionally contains no Engine storage,
/// plugin, JSON View, numeric scheduling, or Production ABI implementation.
pub const CURATED_EXPORT_INDEX: &[FacadeExport] = &[
    FacadeExport {
        canonical_path: "synui::app::App",
        type_name: "App",
        prelude: true,
        stability: "stable",
        advanced_replacement: "App-owned runtime entry point",
        legacy_mapping: Some("app-bootstrap"),
    },
    FacadeExport {
        canonical_path: "synui::app::AppContext",
        type_name: "AppContext",
        prelude: true,
        stability: "stable",
        advanced_replacement: "App-owned resource context",
        legacy_mapping: Some("global-signal"),
    },
    FacadeExport {
        canonical_path: "synui::app::State",
        type_name: "State",
        prelude: true,
        stability: "stable",
        advanced_replacement: "Context-owned state",
        legacy_mapping: Some("global-signal"),
    },
    FacadeExport {
        canonical_path: "synui::app::Action",
        type_name: "Action",
        prelude: true,
        stability: "stable",
        advanced_replacement: "Typed event callback",
        legacy_mapping: Some("global-actions"),
    },
    FacadeExport {
        canonical_path: "synui::view::Node",
        type_name: "Node",
        prelude: true,
        stability: "stable",
        advanced_replacement: "Open Element composition",
        legacy_mapping: Some("declarative-view"),
    },
    FacadeExport {
        canonical_path: "synui::view::Element",
        type_name: "Element",
        prelude: true,
        stability: "stable",
        advanced_replacement: "External component lifecycle",
        legacy_mapping: Some("declarative-view"),
    },
    FacadeExport {
        canonical_path: "synui::view::ElementSpec",
        type_name: "ElementSpec",
        prelude: true,
        stability: "stable",
        advanced_replacement: "External Element descriptor",
        legacy_mapping: Some("unsupported-view-constructors"),
    },
    FacadeExport {
        canonical_path: "synui::view::BusinessKey",
        type_name: "BusinessKey",
        prelude: true,
        stability: "stable",
        advanced_replacement: "Automatic identity for repeated siblings",
        legacy_mapping: Some("numeric-identity"),
    },
    FacadeExport {
        canonical_path: "synui::widgets::Button",
        type_name: "Button",
        prelude: true,
        stability: "stable",
        advanced_replacement: "Typed press Action",
        legacy_mapping: Some("global-actions"),
    },
    FacadeExport {
        canonical_path: "synui::widgets::TextInput",
        type_name: "TextInput",
        prelude: true,
        stability: "stable",
        advanced_replacement: "InputConfig<String>",
        legacy_mapping: Some("scalar-bindings"),
    },
    FacadeExport {
        canonical_path: "synui::widgets::TextArea",
        type_name: "TextArea",
        prelude: true,
        stability: "stable",
        advanced_replacement: "InputConfig<String> multiline; label is a11y name only",
        legacy_mapping: Some("unsupported-view-constructors"),
    },
    FacadeExport {
        canonical_path: "synui::widgets::Checkbox",
        type_name: "Checkbox",
        prelude: true,
        stability: "stable",
        advanced_replacement: "InputConfig<bool>",
        legacy_mapping: Some("scalar-bindings"),
    },
    FacadeExport {
        canonical_path: "synui::widgets::Switch",
        type_name: "Switch",
        prelude: true,
        stability: "stable",
        advanced_replacement: "InputConfig<bool> (native Switch, not Checkbox)",
        legacy_mapping: Some("unsupported-view-constructors"),
    },
    FacadeExport {
        canonical_path: "synui::widgets::Radio",
        type_name: "Radio",
        prelude: true,
        stability: "stable",
        advanced_replacement: "InputConfig<bool> + label",
        legacy_mapping: Some("unsupported-view-constructors"),
    },
    FacadeExport {
        canonical_path: "synui::widgets::NumberInput",
        type_name: "NumberInput",
        prelude: true,
        stability: "stable",
        advanced_replacement: "InputConfig<f64>",
        legacy_mapping: Some("scalar-bindings"),
    },
    FacadeExport {
        canonical_path: "synui::widgets::Select",
        type_name: "Select",
        prelude: true,
        stability: "stable",
        advanced_replacement: "InputConfig<ChoiceId>",
        legacy_mapping: Some("selection-binding"),
    },
    FacadeExport {
        canonical_path: "synui::widgets::LinearProgress",
        type_name: "LinearProgress",
        prelude: true,
        stability: "stable",
        advanced_replacement: "value 0..=1 + optional indeterminate",
        legacy_mapping: Some("unsupported-view-constructors"),
    },
    FacadeExport {
        canonical_path: "synui::widgets::CircularProgress",
        type_name: "CircularProgress",
        prelude: true,
        stability: "stable",
        advanced_replacement: "value 0..=1 determinate arc + optional indeterminate rotating arc",
        legacy_mapping: Some("unsupported-view-constructors"),
    },
    FacadeExport {
        canonical_path: "synui::widgets::Lottie",
        type_name: "Lottie",
        prelude: true,
        stability: "stable",
        advanced_replacement:
            "from_asset(id) with Window::with_lottie, or from_json/new; playing/size",
        legacy_mapping: Some("unsupported-view-constructors"),
    },
    FacadeExport {
        canonical_path: "synui::widgets::MorphingShape",
        type_name: "MorphingShape",
        prelude: true,
        stability: "stable",
        advanced_replacement: "size/playing/speed morph host",
        legacy_mapping: Some("unsupported-view-constructors"),
    },
    FacadeExport {
        canonical_path: "synui::widgets::Card",
        type_name: "Card",
        prelude: true,
        stability: "stable",
        advanced_replacement: "elevation + children Nodes",
        legacy_mapping: Some("unsupported-view-constructors"),
    },
    FacadeExport {
        canonical_path: "synui::widgets::ListItem",
        type_name: "ListItem",
        prelude: true,
        stability: "stable",
        advanced_replacement: "title/subtitle + optional trailing + Action<()>",
        legacy_mapping: Some("unsupported-view-constructors"),
    },
    FacadeExport {
        canonical_path: "synui::widgets::DataTable",
        type_name: "DataTable",
        prelude: true,
        stability: "stable",
        advanced_replacement: "VirtualDataTableModel",
        legacy_mapping: Some("unsupported-view-constructors"),
    },
    FacadeExport {
        canonical_path: "synui::widgets::TableColumn",
        type_name: "TableColumn",
        prelude: true,
        stability: "stable",
        advanced_replacement: "DataTable column metadata",
        legacy_mapping: Some("unsupported-view-constructors"),
    },
    FacadeExport {
        canonical_path: "synui::widgets::VirtualTree",
        type_name: "VirtualTree",
        prelude: true,
        stability: "stable",
        advanced_replacement: "TreeView virtual mount host",
        legacy_mapping: Some("unsupported-view-constructors"),
    },
    FacadeExport {
        canonical_path: "synui::widgets::TreeNode",
        type_name: "TreeNode",
        prelude: true,
        stability: "stable",
        advanced_replacement: "VirtualTree node model",
        legacy_mapping: Some("unsupported-view-constructors"),
    },
    FacadeExport {
        canonical_path: "synui::widgets::SplitPane",
        type_name: "SplitPane",
        prelude: true,
        stability: "stable",
        advanced_replacement: "fixed nested SplitPane host",
        legacy_mapping: Some("unsupported-view-constructors"),
    },
    FacadeExport {
        canonical_path: "synui::widgets::SplitDirection",
        type_name: "SplitDirection",
        prelude: true,
        stability: "stable",
        advanced_replacement: "SplitPane orientation",
        legacy_mapping: Some("unsupported-view-constructors"),
    },
    FacadeExport {
        canonical_path: "synui::widgets::TabBar",
        type_name: "TabBar",
        prelude: true,
        stability: "stable",
        advanced_replacement: "TabBar selection Action",
        legacy_mapping: Some("unsupported-view-constructors"),
    },
    FacadeExport {
        canonical_path: "synui::widgets::RichDocument",
        type_name: "RichDocument",
        prelude: true,
        stability: "stable",
        advanced_replacement: "RichEdit BlockDoc host",
        legacy_mapping: Some("unsupported-view-constructors"),
    },
    FacadeExport {
        canonical_path: "synui::widgets::AppBar",
        type_name: "AppBar",
        prelude: true,
        stability: "stable",
        advanced_replacement: "AppBar chrome host",
        legacy_mapping: Some("unsupported-view-constructors"),
    },
    FacadeExport {
        canonical_path: "synui::widgets::NavigationDrawer",
        type_name: "NavigationDrawer",
        prelude: true,
        stability: "stable",
        advanced_replacement: "NavigationDrawer host",
        legacy_mapping: Some("unsupported-view-constructors"),
    },
    FacadeExport {
        canonical_path: "synui::widgets::BottomNavigation",
        type_name: "BottomNavigation",
        prelude: true,
        stability: "stable",
        advanced_replacement: "BottomNavigation host",
        legacy_mapping: Some("unsupported-view-constructors"),
    },
    FacadeExport {
        canonical_path: "synui::widgets::Dialog",
        type_name: "Dialog",
        prelude: true,
        stability: "stable",
        advanced_replacement: "Dialog overlay host",
        legacy_mapping: Some("unsupported-view-constructors"),
    },
    FacadeExport {
        canonical_path: "synui::widgets::BottomSheet",
        type_name: "BottomSheet",
        prelude: true,
        stability: "stable",
        advanced_replacement: "BottomSheet overlay host",
        legacy_mapping: Some("unsupported-view-constructors"),
    },
    FacadeExport {
        canonical_path: "synui::widgets::SnackBar",
        type_name: "SnackBar",
        prelude: true,
        stability: "stable",
        advanced_replacement: "SnackBar overlay host",
        legacy_mapping: Some("unsupported-view-constructors"),
    },
    FacadeExport {
        canonical_path: "synui::widgets::Icon",
        type_name: "Icon",
        prelude: true,
        stability: "stable",
        advanced_replacement: "Icon glyph host",
        legacy_mapping: Some("unsupported-view-constructors"),
    },
    FacadeExport {
        canonical_path: "synui::widgets::Label",
        type_name: "Label",
        prelude: true,
        stability: "stable",
        advanced_replacement: "native Label with optional truncate/a11y name",
        legacy_mapping: Some("unsupported-view-constructors"),
    },
    FacadeExport {
        canonical_path: "synui::widgets::Image",
        type_name: "Image",
        prelude: true,
        stability: "stable",
        advanced_replacement: "Image cache-id host",
        legacy_mapping: Some("unsupported-view-constructors"),
    },
    FacadeExport {
        canonical_path: "synui::widgets::Scroll",
        type_name: "Scroll",
        prelude: true,
        stability: "stable",
        advanced_replacement: "ScrollView host with sticky child metadata",
        legacy_mapping: Some("unsupported-view-constructors"),
    },
    FacadeExport {
        canonical_path: "synui::widgets::VirtualList",
        type_name: "VirtualList",
        prelude: true,
        stability: "stable",
        advanced_replacement: "ListView virtual mount host",
        legacy_mapping: Some("unsupported-view-constructors"),
    },
    FacadeExport {
        canonical_path: "synui::widgets::MenuItem",
        type_name: "MenuItem",
        prelude: true,
        stability: "stable",
        advanced_replacement: "in-app menu item model",
        legacy_mapping: Some("unsupported-view-constructors"),
    },
    FacadeExport {
        canonical_path: "synui::widgets::Menu",
        type_name: "Menu",
        prelude: true,
        stability: "stable",
        advanced_replacement: "in-app MenuBar menu model",
        legacy_mapping: Some("unsupported-view-constructors"),
    },
    FacadeExport {
        canonical_path: "synui::widgets::MenuBar",
        type_name: "MenuBar",
        prelude: true,
        stability: "stable",
        advanced_replacement: "in-app MenuBar host (not DesktopMenu/muda)",
        legacy_mapping: Some("unsupported-view-constructors"),
    },
    FacadeExport {
        canonical_path: "synui::widgets::ContextMenu",
        type_name: "ContextMenu",
        prelude: true,
        stability: "stable",
        advanced_replacement: "in-app ContextMenu overlay host",
        legacy_mapping: Some("unsupported-view-constructors"),
    },
    FacadeExport {
        canonical_path: "synui::widgets::Styled",
        type_name: "Styled",
        prelude: true,
        stability: "stable",
        advanced_replacement: "CSS-subset sticky/fixed host",
        legacy_mapping: Some("fallible-view-style"),
    },
    FacadeExport {
        canonical_path: "synui::model::VirtualDataTableModel",
        type_name: "VirtualDataTableModel",
        prelude: true,
        stability: "stable",
        advanced_replacement: "App-owned bounded data model",
        legacy_mapping: Some("selection-binding"),
    },
    FacadeExport {
        canonical_path: "synui::model::CellRange",
        type_name: "CellRange",
        prelude: true,
        stability: "stable",
        advanced_replacement: "Half-open bounded viewport",
        legacy_mapping: Some("unsupported-view-constructors"),
    },
    FacadeExport {
        canonical_path: "synui::style::Style",
        type_name: "Style",
        prelude: true,
        stability: "stable",
        advanced_replacement: "Non-panicking style builder",
        legacy_mapping: Some("fallible-view-style"),
    },
    FacadeExport {
        canonical_path: "synui::style::StyleInput",
        type_name: "StyleInput",
        prelude: true,
        stability: "stable",
        advanced_replacement: "Validated runtime style input",
        legacy_mapping: Some("fallible-view-style"),
    },
    FacadeExport {
        canonical_path: "synui::style::StyleDiagnostic",
        type_name: "StyleDiagnostic",
        prelude: true,
        stability: "stable",
        advanced_replacement: "Structured invalid-style evidence",
        legacy_mapping: Some("panic-and-class-style"),
    },
    FacadeExport {
        canonical_path: "synui::platform::Window",
        type_name: "Window",
        prelude: true,
        stability: "stable",
        advanced_replacement: "Native host configuration",
        legacy_mapping: Some("app-bootstrap"),
    },
    FacadeExport {
        canonical_path: "synui::platform::RunLoop",
        type_name: "RunLoop",
        prelude: true,
        stability: "stable",
        advanced_replacement: "Native host event loop",
        legacy_mapping: Some("app-bootstrap"),
    },
];

/// Public alias for the compatibility module's machine-checkable denylist.
pub const FACADE_DENYLIST: &[compatibility::FacadeDenylistEntry] =
    compatibility::V02_FACADE_DENYLIST;
