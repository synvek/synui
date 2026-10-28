//! PNG snapshot testing support.

use crate::headless::HeadlessRenderer;
use synui_core::A11yRole;
use synui_core::{Event, Key, Point, PointerButton, WidgetStore};
use synui_theme::Theme;
use vello::Scene;

/// Render widget tree to PNG bytes (deterministic headless mode).
pub fn snapshot_store(store: &mut WidgetStore, theme: &Theme, width: u32, height: u32) -> Scene {
    let mut headless = HeadlessRenderer::new(width, height);
    headless.render_store(store, theme)
}

/// Render widget tree to PNG bytes.
pub fn snapshot_png(
    store: &mut WidgetStore,
    theme: &Theme,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, image::ImageError> {
    let mut headless = HeadlessRenderer::new(width, height);
    headless.render_to_png(store, theme)
}

/// Simulate pointer tap at position.
pub fn simulate_tap(store: &mut WidgetStore, x: f64, y: f64) {
    let pos = Point::new(x, y);
    store.dispatch_event(&Event::PointerDown {
        id: 0,
        position: pos,
        button: PointerButton::Primary,
        shift: false,
        ctrl: false,
    });
    store.dispatch_event(&Event::PointerUp {
        id: 0,
        position: pos,
        button: PointerButton::Primary,
    });
}

/// Simulate key press on focused widget.
pub fn simulate_key(store: &mut WidgetStore, key: Key) {
    store.dispatch_event(&Event::KeyDown {
        key: key.clone(),
        repeat: false,
        shift: false,
        ctrl: false,
        alt: false,
    });
    store.dispatch_event(&Event::KeyUp { key });
}

/// Simulate drag gesture from start to end.
pub fn simulate_drag(store: &mut WidgetStore, from: Point, to: Point) {
    store.dispatch_event(&Event::PointerDown {
        id: 0,
        position: from,
        button: PointerButton::Primary,
        shift: false,
        ctrl: false,
    });
    store.dispatch_event(&Event::PointerMove {
        id: 0,
        position: to,
    });
    store.dispatch_event(&Event::PointerUp {
        id: 0,
        position: to,
        button: PointerButton::Primary,
    });
}

/// Simulate scroll wheel at position.
pub fn simulate_scroll(store: &mut WidgetStore, x: f64, y: f64, delta_y: f32) {
    store.dispatch_event(&Event::Scroll {
        position: Point::new(x, y),
        delta_x: 0.0,
        delta_y,
    });
}

/// Find widgets by accessibility role.
pub fn find_by_role(store: &WidgetStore, role: A11yRole) -> Vec<synui_core::WidgetId> {
    store.find_by_role(role)
}

/// Find widget by accessibility label.
pub fn find_by_id(store: &WidgetStore, label: &str) -> Option<synui_core::WidgetId> {
    store.find_by_label(label)
}

/// Property-based layout test helper: verify all widgets have non-negative bounds.
pub fn assert_layout_valid(store: &WidgetStore) -> bool {
    fn check(store: &WidgetStore, id: synui_core::WidgetId) -> bool {
        let Some(w) = store.get(id) else {
            return false;
        };
        let b = w.bounds();
        if b.width() < 0.0 || b.height() < 0.0 {
            return false;
        }
        store
            .children_of(id)
            .iter()
            .all(|&child| check(store, child))
    }
    store.root().map(|r| check(store, r)).unwrap_or(true)
}

/// Encode RGBA8 pixels as PNG bytes for snapshot comparison.
pub fn rgba_to_png(width: u32, height: u32, pixels: &[u8]) -> Result<Vec<u8>, image::ImageError> {
    use image::ImageEncoder;
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4))
        .ok_or_else(|| {
            image::ImageError::Parameter(image::error::ParameterError::from_kind(
                image::error::ParameterErrorKind::DimensionMismatch,
            ))
        })?;
    if pixels.len() != expected {
        return Err(image::ImageError::Parameter(
            image::error::ParameterError::from_kind(
                image::error::ParameterErrorKind::DimensionMismatch,
            ),
        ));
    }
    let img = image::RgbaImage::from_raw(width, height, pixels.to_vec()).ok_or_else(|| {
        image::ImageError::Parameter(image::error::ParameterError::from_kind(
            image::error::ParameterErrorKind::DimensionMismatch,
        ))
    })?;
    let mut buf = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut buf);
    encoder.write_image(img.as_raw(), width, height, image::ExtendedColorType::Rgba8)?;
    Ok(buf)
}

/// Compare PNG bytes to a baseline with optional pixel tolerance (0 = exact).
pub fn assert_png_matches(png: &[u8], expected: &[u8], max_diff_ratio: f32) -> Result<(), String> {
    if png == expected {
        return Ok(());
    }
    use image::ImageReader;
    let a = ImageReader::new(std::io::Cursor::new(png))
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())?
        .to_rgba8();
    let b = ImageReader::new(std::io::Cursor::new(expected))
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())?
        .to_rgba8();
    if a.dimensions() != b.dimensions() {
        return Err(format!(
            "dimension mismatch {:?} vs {:?}",
            a.dimensions(),
            b.dimensions()
        ));
    }
    let pixels = a.width() * a.height();
    let mut diff = 0u32;
    for (pa, pb) in a.pixels().zip(b.pixels()) {
        if pa.0 != pb.0 {
            diff += 1;
        }
    }
    let ratio = diff as f32 / pixels as f32;
    if ratio > max_diff_ratio {
        Err(format!("pixel diff ratio {ratio} exceeds {max_diff_ratio}"))
    } else {
        Ok(())
    }
}

/// Req 2.9: per-channel absolute difference ≤ `tol` (typically 1 = 1/256).
pub fn assert_rgba_within_tolerance(
    a: &[u8],
    b: &[u8],
    width: u32,
    height: u32,
    tol: u8,
) -> Result<(), String> {
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4))
        .ok_or_else(|| "dimension overflow".to_string())?;
    if a.len() != expected || b.len() != expected {
        return Err(format!(
            "rgba length mismatch a={} b={} expected={}",
            a.len(),
            b.len(),
            expected
        ));
    }
    for (i, (&pa, &pb)) in a.iter().zip(b.iter()).enumerate() {
        let d = pa.abs_diff(pb);
        if d > tol {
            let px = i / 4;
            let ch = i % 4;
            let x = px as u32 % width;
            let y = px as u32 / width;
            return Err(format!("px ({x},{y}) ch {ch}: |{pa}-{pb}|={d} > tol {tol}"));
        }
    }
    Ok(())
}

/// Snapshot RGBA via Vello GPU-compute path (`use_cpu = false`).
pub fn snapshot_rgba_gpu(
    store: &mut WidgetStore,
    theme: &Theme,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let mut h = HeadlessRenderer::with_cpu(width, height, false);
    h.render_to_rgba(store, theme)
}

/// Snapshot RGBA via Vello CPU path (`use_cpu = true`).
pub fn snapshot_rgba_cpu(
    store: &mut WidgetStore,
    theme: &Theme,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let mut h = HeadlessRenderer::with_cpu(width, height, true);
    h.render_to_rgba(store, theme)
}
