//! MD3 shape corner tokens.

#[derive(Debug, Clone)]
pub struct ShapeScheme {
    pub extra_small: f64,
    pub small: f64,
    pub medium: f64,
    pub large: f64,
    pub extra_large: f64,
    pub full: f64,
}

impl Default for ShapeScheme {
    fn default() -> Self {
        Self {
            extra_small: 4.0,
            small: 8.0,
            medium: 12.0,
            large: 16.0,
            extra_large: 28.0,
            full: 9999.0,
        }
    }
}

impl ShapeScheme {
    pub fn button_radius(&self) -> f64 {
        self.large
    }

    pub fn card_radius(&self) -> f64 {
        self.medium
    }
}

/// MD3 elevation shadow parameters: (offset_y, blur_radius, opacity).
pub fn elevation_shadow(elevation: u8) -> (f64, f64, f32) {
    match elevation {
        0 => (0.0, 0.0, 0.0),
        1 => (1.0, 3.0, 0.15),
        2 => (2.0, 6.0, 0.18),
        3 => (4.0, 8.0, 0.20),
        4 => (6.0, 10.0, 0.22),
        _ => (8.0, 12.0, 0.24),
    }
}
