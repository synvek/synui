//! MD3 color scheme generation from seed color.

use palette::{Hsv, IntoColor, Srgb};

#[derive(Debug, Clone)]
pub struct ColorScheme {
    pub primary: peniko::Color,
    pub on_primary: peniko::Color,
    pub secondary: peniko::Color,
    pub on_secondary: peniko::Color,
    pub surface: peniko::Color,
    pub on_surface: peniko::Color,
    pub surface_variant: peniko::Color,
    pub on_surface_variant: peniko::Color,
    pub surface_container: peniko::Color,
    pub outline: peniko::Color,
    pub error: peniko::Color,
    pub on_error: peniko::Color,
}

impl ColorScheme {
    pub fn from_seed(seed: peniko::Color, dark: bool) -> Self {
        let components = seed.components;
        let rgb = Srgb::new(components[0], components[1], components[2]);
        let hsv: Hsv = rgb.into_color();
        let (surface, on_surface, primary, on_primary) = if dark {
            (
                peniko::Color::from_rgb8(28, 27, 31),
                peniko::Color::from_rgb8(230, 225, 229),
                seed,
                peniko::Color::from_rgb8(255, 255, 255),
            )
        } else {
            (
                peniko::Color::from_rgb8(255, 251, 254),
                peniko::Color::from_rgb8(28, 27, 31),
                seed,
                peniko::Color::from_rgb8(255, 255, 255),
            )
        };
        let secondary = shift_hue(hsv, 60.0, dark);
        let _ = hsv;
        Self {
            primary,
            on_primary,
            secondary,
            on_secondary: if dark {
                peniko::Color::from_rgb8(204, 194, 220)
            } else {
                peniko::Color::from_rgb8(74, 68, 88)
            },
            surface,
            on_surface,
            surface_variant: if dark {
                peniko::Color::from_rgb8(73, 69, 79)
            } else {
                peniko::Color::from_rgb8(231, 224, 236)
            },
            on_surface_variant: if dark {
                peniko::Color::from_rgb8(202, 196, 208)
            } else {
                peniko::Color::from_rgb8(73, 69, 79)
            },
            surface_container: if dark {
                peniko::Color::from_rgb8(43, 41, 48)
            } else {
                peniko::Color::from_rgb8(243, 237, 247)
            },
            outline: if dark {
                peniko::Color::from_rgb8(147, 143, 153)
            } else {
                peniko::Color::from_rgb8(121, 116, 126)
            },
            error: peniko::Color::from_rgb8(179, 38, 30),
            on_error: peniko::Color::from_rgb8(255, 255, 255),
        }
    }
}

fn shift_hue(hsv: Hsv, degrees: f32, dark: bool) -> peniko::Color {
    let mut h = hsv.hue.into_degrees() + degrees;
    if h >= 360.0 {
        h -= 360.0;
    }
    let rgb: Srgb = Hsv::new(h, hsv.saturation, if dark { 0.5 } else { 0.7 }).into_color();
    peniko::Color::new([rgb.red, rgb.green, rgb.blue, 1.0])
}

/// Elevation shadow parameters for MD3.
#[derive(Debug, Clone, Copy)]
pub struct Elevation {
    pub level: u8,
    pub opacity: f32,
    pub blur: f32,
    pub offset_y: f32,
}

impl Elevation {
    pub fn from_level(level: u8) -> Self {
        let l = level.min(24) as f32;
        Self {
            level,
            opacity: 0.05 + l * 0.01,
            blur: l * 1.5,
            offset_y: l * 0.5,
        }
    }
}
