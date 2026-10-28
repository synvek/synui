//! Design system presets (MD3, Fluent, Cupertino).

use crate::md3::ColorScheme;
use crate::shape::ShapeScheme;
use synui_core::ThemeMetrics;

/// Visual design language for SynUI widgets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DesignSystem {
    #[default]
    Md3,
    Fluent,
    Cupertino,
}

impl DesignSystem {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "md3" | "material" | "material3" => Some(Self::Md3),
            "fluent" | "winui" | "microsoft" => Some(Self::Fluent),
            "ios" | "cupertino" | "apple" => Some(Self::Cupertino),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Md3 => "Material 3",
            Self::Fluent => "Fluent",
            Self::Cupertino => "Cupertino",
        }
    }

    pub fn default_shape(self) -> ShapeScheme {
        match self {
            Self::Md3 => ShapeScheme::default(),
            Self::Fluent => ShapeScheme {
                extra_small: 2.0,
                small: 4.0,
                medium: 4.0,
                large: 4.0,
                extra_large: 8.0,
                full: 9999.0,
            },
            Self::Cupertino => ShapeScheme {
                extra_small: 6.0,
                small: 8.0,
                medium: 12.0,
                large: 14.0,
                extra_large: 20.0,
                full: 9999.0,
            },
        }
    }

    pub fn color_scheme(self, seed: peniko::Color, dark: bool) -> ColorScheme {
        match self {
            Self::Md3 => ColorScheme::from_seed(seed, dark),
            Self::Fluent => fluent_scheme(dark),
            Self::Cupertino => cupertino_scheme(dark),
        }
    }

    pub fn metrics(self, shape: &ShapeScheme) -> ThemeMetrics {
        let (ripple, elevation) = match self {
            Self::Md3 => (true, true),
            Self::Fluent | Self::Cupertino => (false, false),
        };
        ThemeMetrics {
            corner_button: match self {
                Self::Md3 => shape.large,
                Self::Fluent => shape.small,
                Self::Cupertino => 10.0,
            },
            corner_card: shape.card_radius(),
            corner_dialog: match self {
                Self::Md3 => shape.large,
                Self::Fluent => shape.medium,
                Self::Cupertino => shape.large,
            },
            corner_field: match self {
                Self::Md3 => shape.extra_small,
                Self::Fluent => shape.small,
                Self::Cupertino => 10.0,
            },
            corner_menu: match self {
                Self::Md3 => shape.extra_small,
                Self::Fluent => shape.small,
                Self::Cupertino => shape.medium,
            },
            corner_switch: match self {
                Self::Md3 => 10.0,
                Self::Fluent => 2.0,
                Self::Cupertino => 16.0,
            },
            ripple_enabled: ripple,
            elevation_enabled: elevation,
            bordered_surfaces: matches!(self, Self::Fluent),
        }
    }
}

fn fluent_scheme(dark: bool) -> ColorScheme {
    let primary = if dark {
        peniko::Color::from_rgb8(96, 205, 255)
    } else {
        peniko::Color::from_rgb8(0, 120, 212)
    };
    let on_primary = peniko::Color::from_rgb8(255, 255, 255);
    if dark {
        ColorScheme {
            primary,
            on_primary,
            secondary: peniko::Color::from_rgb8(200, 200, 200),
            on_secondary: peniko::Color::from_rgb8(0, 0, 0),
            surface: peniko::Color::from_rgb8(32, 32, 32),
            on_surface: peniko::Color::from_rgb8(255, 255, 255),
            surface_variant: peniko::Color::from_rgb8(45, 45, 45),
            on_surface_variant: peniko::Color::from_rgb8(200, 200, 200),
            surface_container: peniko::Color::from_rgb8(40, 40, 40),
            outline: peniko::Color::from_rgb8(117, 117, 117),
            error: peniko::Color::from_rgb8(255, 153, 164),
            on_error: peniko::Color::from_rgb8(0, 0, 0),
        }
    } else {
        ColorScheme {
            primary,
            on_primary,
            secondary: peniko::Color::from_rgb8(96, 96, 96),
            on_secondary: peniko::Color::from_rgb8(255, 255, 255),
            surface: peniko::Color::from_rgb8(250, 250, 250),
            on_surface: peniko::Color::from_rgb8(0, 0, 0),
            surface_variant: peniko::Color::from_rgb8(243, 243, 243),
            on_surface_variant: peniko::Color::from_rgb8(96, 96, 96),
            surface_container: peniko::Color::from_rgb8(255, 255, 255),
            outline: peniko::Color::from_rgb8(138, 138, 138),
            error: peniko::Color::from_rgb8(196, 43, 28),
            on_error: peniko::Color::from_rgb8(255, 255, 255),
        }
    }
}

fn cupertino_scheme(dark: bool) -> ColorScheme {
    let primary = if dark {
        peniko::Color::from_rgb8(10, 132, 255)
    } else {
        peniko::Color::from_rgb8(0, 122, 255)
    };
    let on_primary = peniko::Color::from_rgb8(255, 255, 255);
    if dark {
        ColorScheme {
            primary,
            on_primary,
            secondary: peniko::Color::from_rgb8(142, 142, 147),
            on_secondary: peniko::Color::from_rgb8(255, 255, 255),
            surface: peniko::Color::from_rgb8(28, 28, 30),
            on_surface: peniko::Color::from_rgb8(255, 255, 255),
            surface_variant: peniko::Color::from_rgb8(44, 44, 46),
            on_surface_variant: peniko::Color::from_rgb8(142, 142, 147),
            surface_container: peniko::Color::from_rgb8(44, 44, 46),
            outline: peniko::Color::from_rgb8(84, 84, 88),
            error: peniko::Color::from_rgb8(255, 69, 58),
            on_error: peniko::Color::from_rgb8(255, 255, 255),
        }
    } else {
        ColorScheme {
            primary,
            on_primary,
            secondary: peniko::Color::from_rgb8(142, 142, 147),
            on_secondary: peniko::Color::from_rgb8(255, 255, 255),
            surface: peniko::Color::from_rgb8(255, 255, 255),
            on_surface: peniko::Color::from_rgb8(0, 0, 0),
            surface_variant: peniko::Color::from_rgb8(242, 242, 247),
            on_surface_variant: peniko::Color::from_rgb8(60, 60, 67),
            surface_container: peniko::Color::from_rgb8(242, 242, 247),
            outline: peniko::Color::from_rgb8(198, 198, 200),
            error: peniko::Color::from_rgb8(255, 59, 48),
            on_error: peniko::Color::from_rgb8(255, 255, 255),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_design_system_aliases() {
        assert_eq!(DesignSystem::parse("fluent"), Some(DesignSystem::Fluent));
        assert_eq!(DesignSystem::parse("iOS"), Some(DesignSystem::Cupertino));
        assert_eq!(DesignSystem::parse("md3"), Some(DesignSystem::Md3));
    }

    #[test]
    fn fluent_disables_ripple_and_elevation() {
        let m = DesignSystem::Fluent.metrics(&ShapeScheme::default());
        assert!(!m.ripple_enabled);
        assert!(!m.elevation_enabled);
        assert!(m.bordered_surfaces);
    }
}
