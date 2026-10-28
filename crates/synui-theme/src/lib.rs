//! Material Design 3 theme tokens and dynamic color.

mod design_system;
mod md3;
mod ripple;
mod shape;
mod shape_morph;

pub use design_system::*;
pub use md3::*;
pub use ripple::*;
pub use shape::*;
pub use shape_morph::*;

use synui_core::ThemeRef;

/// Active theme state.
#[derive(Debug, Clone)]
pub struct Theme {
    pub design_system: DesignSystem,
    pub scheme: ColorScheme,
    pub shape: ShapeScheme,
    pub is_dark: bool,
    seed: peniko::Color,
}

impl Theme {
    pub fn new(design_system: DesignSystem, seed: peniko::Color, dark: bool) -> Self {
        let shape = design_system.default_shape();
        Self {
            design_system,
            scheme: design_system.color_scheme(seed, dark),
            shape,
            is_dark: dark,
            seed,
        }
    }

    pub fn from_seed(seed: peniko::Color, dark: bool) -> Self {
        Self::new(DesignSystem::Md3, seed, dark)
    }

    pub fn with_design_system(mut self, design_system: DesignSystem) -> Self {
        if self.design_system != design_system {
            self.design_system = design_system;
            self.shape = design_system.default_shape();
            self.scheme = design_system.color_scheme(self.seed, self.is_dark);
        }
        self
    }

    pub fn seed(&self) -> peniko::Color {
        self.seed
    }

    pub fn to_theme_ref(&self) -> ThemeRef {
        ThemeRef {
            surface: self.scheme.surface,
            on_surface: self.scheme.on_surface,
            primary: self.scheme.primary,
            on_primary: self.scheme.on_primary,
            outline: self.scheme.outline,
            secondary: self.scheme.secondary,
            on_secondary: self.scheme.on_secondary,
            surface_variant: self.scheme.surface_variant,
            on_surface_variant: self.scheme.on_surface_variant,
            surface_container: self.scheme.surface_container,
            error: self.scheme.error,
            on_error: self.scheme.on_error,
            is_dark: self.is_dark,
            metrics: self.design_system.metrics(&self.shape),
            cupertino: matches!(self.design_system, DesignSystem::Cupertino),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::from_seed(peniko::Color::from_rgb8(103, 80, 164), false)
    }
}

/// Resolve design system from env `SYNUI_DESIGN_SYSTEM` and/or `--design-system=`.
pub fn design_system_from_config() -> DesignSystem {
    for arg in std::env::args().skip(1) {
        if let Some(value) = arg.strip_prefix("--design-system=") {
            if let Some(ds) = DesignSystem::parse(value) {
                return ds;
            }
        }
    }
    if let Ok(value) = std::env::var("SYNUI_DESIGN_SYSTEM") {
        if let Some(ds) = DesignSystem::parse(&value) {
            return ds;
        }
    }
    DesignSystem::default()
}
