//! Platform abstraction and lifecycle hooks.

mod lifecycle;

pub use lifecycle::*;

#[cfg(feature = "android")]
pub use lifecycle::android::AndroidLifecycle;
#[cfg(feature = "ios")]
pub use lifecycle::ios::IosLifecycle;
#[cfg(feature = "web")]
pub use lifecycle::web::{set_dom_a11y_click_handler, DomA11yNode, DomA11yTree, WebLifecycle};
