//! Icon cache: curated Material Symbols SVGs + app-loaded SVG strings.
//!
//! # Usage
//!
//! - Pass a **short name** (`"home"`) or **canonical** id (`"material-symbols:home"`).
//!   Short names and bare `material-symbols:name` resolve to the **outlined** glyph.
//!   Use `"home-fill"` / `"material-symbols:home-fill"` for the filled variant.
//! - [`IconCache::paint`] lazily loads curated glyphs via [`iconify_svg`].
//! - Tint with `currentColor` substitution (same as Image SVG); pass theme / cascade color.
//! - Extend the curated set by editing [`iconify_svg`] and [`resolve_icon_name`] (see book `icons.md`).
//! - App-specific icons: [`IconCache::load`] or facade `Window::with_icon`.

use std::collections::HashMap;
use synui_core::IconPainter;
use thiserror::Error;
use vello::kurbo::{Affine, Rect, Vec2};
use vello::peniko::Color;
use vello::Scene;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IconId(pub String);

#[derive(Debug, Error)]
pub enum IconError {
    #[error("parse error: {0}")]
    Parse(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub struct IconCache {
    raw: HashMap<IconId, String>,
    trees: HashMap<(IconId, u32), usvg::Tree>,
}

impl Default for IconCache {
    fn default() -> Self {
        Self::new()
    }
}

impl IconCache {
    pub fn new() -> Self {
        Self {
            raw: HashMap::new(),
            trees: HashMap::new(),
        }
    }

    /// Insert or replace an SVG string for `id` (use `currentColor` for theme tinting).
    pub fn load(&mut self, id: IconId, svg: &str) -> Result<(), IconError> {
        self.raw.insert(id.clone(), svg.to_string());
        self.trees.retain(|(cached_id, _), _| cached_id != &id);
        Ok(())
    }

    /// Load an SVG file from disk into the cache under `id`.
    pub fn load_path(
        &mut self,
        id: IconId,
        path: impl AsRef<std::path::Path>,
    ) -> Result<(), IconError> {
        let svg = std::fs::read_to_string(path)?;
        self.load(id, &svg)
    }

    pub fn contains(&self, id: &IconId) -> bool {
        self.raw.contains_key(id)
    }

    /// Ensure `id` is present: already loaded, or resolved from the curated table.
    pub fn ensure(&mut self, id: &IconId) -> Result<(), IconError> {
        if self.raw.contains_key(id) {
            return Ok(());
        }
        let canonical = resolve_icon_name(&id.0);
        let svg = iconify_svg(canonical).ok_or_else(|| {
            IconError::Parse(format!(
                "icon not loaded and not in curated set: {} (resolved: {canonical})",
                id.0
            ))
        })?;
        self.raw.insert(id.clone(), svg.to_string());
        let canonical_id = IconId(canonical.to_string());
        if canonical_id != *id {
            self.raw
                .entry(canonical_id)
                .or_insert_with(|| svg.to_string());
        }
        Ok(())
    }

    fn tree_for(&mut self, id: &IconId, color: Color) -> Result<&usvg::Tree, IconError> {
        self.ensure(id)?;
        let key = (id.clone(), color_key(color));
        if !self.trees.contains_key(&key) {
            let svg = self
                .raw
                .get(id)
                .ok_or_else(|| IconError::Parse(format!("icon not loaded: {}", id.0)))?;
            let tinted = svg.replace("currentColor", &color_to_hex(color));
            let opt = usvg::Options::default();
            let tree =
                usvg::Tree::from_str(&tinted, &opt).map_err(|e| IconError::Parse(e.to_string()))?;
            self.trees.insert(key.clone(), tree);
        }
        Ok(self.trees.get(&key).expect("tree inserted"))
    }

    pub fn paint(
        &mut self,
        scene: &mut Scene,
        id: &IconId,
        bounds: Rect,
        color: Color,
    ) -> Result<(), IconError> {
        let tree = self.tree_for(id, color)?;
        let size = tree.size();
        let scale_x = bounds.width() / size.width() as f64;
        let scale_y = bounds.height() / size.height() as f64;
        let scale = scale_x.min(scale_y);
        let tx = bounds.x0 + (bounds.width() - size.width() as f64 * scale) / 2.0;
        let ty = bounds.y0 + (bounds.height() - size.height() as f64 * scale) / 2.0;
        let transform = Affine::translate(Vec2::new(tx, ty)) * Affine::scale(scale);
        let mut icon_scene = Scene::new();
        vello_svg::append_tree(&mut icon_scene, tree);
        scene.append(&icon_scene, Some(transform));
        Ok(())
    }
}

impl IconPainter for IconCache {
    fn paint_icon(
        &mut self,
        scene: &mut Scene,
        id: &str,
        bounds: Rect,
        color: Color,
    ) -> Result<(), String> {
        self.paint(scene, &IconId(id.to_string()), bounds, color)
            .map_err(|e| e.to_string())
    }
}

fn color_key(color: Color) -> u32 {
    let c = color.to_rgba8();
    u32::from_be_bytes([c.r, c.g, c.b, c.a])
}

fn color_to_hex(color: Color) -> String {
    let c = color.to_rgba8();
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

macro_rules! ms_svg {
    ($d:expr) => {
        concat!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24"><path fill="currentColor" d=""#,
            $d,
            r#""/></svg>"#
        )
    };
}

// --- Outlined (default for short names / bare material-symbols:*) ---

pub const ICON_HOME_SVG: &str = ms_svg!(
    "M12 5.69l5 4.5V18h-2v-6H9v6H7v-7.81l5-4.5M12 3L2 12h3v8h6v-6h2v6h6v-8h3L12 3z"
);
pub const ICON_ADD_SVG: &str = ms_svg!("M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6z");
pub const ICON_DELETE_SVG: &str = ms_svg!(
    "M16 9v10H8V9h8m-1.5-6h-5l-1 1H5v2h14V4h-3.5l-1-1zM18 7H6v12c0 1.1.9 2 2 2h8c1.1 0 2-.9 2-2V7z"
);
pub const ICON_CLOSE_SVG: &str = ms_svg!(
    "M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z"
);
pub const ICON_MENU_SVG: &str = ms_svg!("M3 18h18v-2H3v2zm0-5h18v-2H3v2zm0-7v2h18V6H3z");
pub const ICON_SEARCH_SVG: &str = ms_svg!(
    "M15.5 14h-.79l-.28-.27C15.41 12.59 16 11.11 16 9.5 16 5.91 13.09 3 9.5 3S3 5.91 3 9.5 5.91 16 9.5 16c1.61 0 3.09-.59 4.23-1.57l.27.28v.79l5 4.99L20.49 19l-4.99-5zm-6 0C7.01 14 5 11.99 5 9.5S7.01 5 9.5 5 14 7.01 14 9.5 11.99 14 9.5 14z"
);
pub const ICON_SETTINGS_SVG: &str = ms_svg!(
    "M19.43 12.98c.04-.32.07-.64.07-.98s-.03-.66-.07-.98l2.11-1.65c.19-.15.24-.42.12-.64l-2-3.46c-.12-.22-.39-.3-.61-.22l-2.49 1c-.52-.4-1.08-.73-1.69-.98l-.38-2.65C14.46 2.18 14.25 2 14 2h-4c-.25 0-.46.18-.49.42l-.38 2.65c-.61.25-1.17.59-1.69.98l-2.49-1c-.23-.09-.49 0-.61.22l-2 3.46c-.13.22-.07.49.12.64l2.11 1.65c-.04.32-.07.65-.07.98s.03.66.07.98l-2.11 1.65c-.19.15-.24.42-.12.64l2 3.46c.12.22.39.3.61.22l2.49-1c.52.4 1.08.73 1.69.98l.38 2.65c.03.24.24.42.49.42h4c.25 0 .46-.18.49-.42l.38-2.65c.61-.25 1.17-.59 1.69-.98l2.49 1c.23.09.49 0 .61-.22l2-3.46c.12-.22.07-.49-.12-.64l-2.11-1.65zM12 15.5c-1.93 0-3.5-1.57-3.5-3.5s1.57-3.5 3.5-3.5 3.5 1.57 3.5 3.5-1.57 3.5-3.5 3.5z"
);
pub const ICON_EDIT_SVG: &str = ms_svg!(
    "M14.06 9.02l.92.92L5.92 19H5v-.92l9.06-9.06M17.66 3c-.25 0-.51.1-.7.29l-1.83 1.83 3.75 3.75 1.83-1.83c.39-.39.39-1.02 0-1.41l-2.34-2.34c-.2-.2-.45-.29-.71-.29zm-3.6 3.19L3 17.25V21h3.75L17.81 9.94l-3.75-3.75z"
);
pub const ICON_CHECK_SVG: &str = ms_svg!("M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z");
pub const ICON_ARROW_BACK_SVG: &str =
    ms_svg!("M20 11H7.83l5.59-5.59L12 4l-8 8 8 8 1.41-1.41L7.83 13H20v-2z");
pub const ICON_ARROW_FORWARD_SVG: &str =
    ms_svg!("M12 4l-1.41 1.41L16.17 11H4v2h12.17l-5.58 5.59L12 20l8-8z");
pub const ICON_ARROW_UPWARD_SVG: &str =
    ms_svg!("M4 12l1.41 1.41L11 7.83V20h2V7.83l5.58 5.59L20 12l-8-8z");
pub const ICON_ARROW_DOWNWARD_SVG: &str =
    ms_svg!("M20 12l-1.41-1.41L13 16.17V4h-2v12.17l-5.58-5.59L4 12l8 8z");
pub const ICON_MORE_VERT_SVG: &str = ms_svg!(
    "M12 8c1.1 0 2-.9 2-2s-.9-2-2-2-2 .9-2 2 .9 2 2 2zm0 2c-1.1 0-2 .9-2 2s.9 2 2 2 2-.9 2-2-.9-2-2-2zm0 6c-1.1 0-2 .9-2 2s.9 2 2 2 2-.9 2-2-.9-2-2-2z"
);
pub const ICON_MORE_HORIZ_SVG: &str = ms_svg!(
    "M6 10c-1.1 0-2 .9-2 2s.9 2 2 2 2-.9 2-2-.9-2-2-2zm12 0c-1.1 0-2 .9-2 2s.9 2 2 2 2-.9 2-2-.9-2-2-2zm-6 0c-1.1 0-2 .9-2 2s.9 2 2 2 2-.9 2-2-.9-2-2-2z"
);
pub const ICON_INFO_SVG: &str = ms_svg!(
    "M11 7h2v2h-2zm0 4h2v6h-2zm1-9C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm0 18c-4.41 0-8-3.59-8-8s3.59-8 8-8 8 3.59 8 8-3.59 8-8 8z"
);
pub const ICON_WARNING_SVG: &str = ms_svg!(
    "M12 5.99L19.53 19H4.47L12 5.99M12 2L1 21h22L12 2zm1 14h-2v2h2v-2zm0-6h-2v4h2v-4z"
);
pub const ICON_ERROR_SVG: &str = ms_svg!(
    "M11 15h2v2h-2v-2zm0-8h2v6h-2V7zm.99-5C6.47 2 2 6.48 2 12s4.47 10 9.99 10C17.52 22 22 17.52 22 12S17.52 2 11.99 2zM12 20c-4.42 0-8-3.58-8-8s3.58-8 8-8 8 3.58 8 8-3.58 8-8 8z"
);
pub const ICON_REFRESH_SVG: &str = ms_svg!(
    "M17.65 6.35C16.2 4.9 14.21 4 12 4c-4.42 0-7.99 3.58-7.99 8s3.57 8 7.99 8c3.73 0 6.84-2.55 7.73-6h-2.08c-.82 2.33-3.04 4-5.65 4-3.31 0-6-2.69-6-6s2.69-6 6-6c1.66 0 3.14.69 4.22 1.78L13 11h7V4l-2.35 2.35z"
);
pub const ICON_PERSON_SVG: &str = ms_svg!(
    "M12 6c1.1 0 2 .9 2 2s-.9 2-2 2-2-.9-2-2 .9-2 2-2m0 10c2.7 0 5.8 1.29 6 2H6c.23-.72 3.31-2 6-2m0-12C9.79 4 8 5.79 8 8s1.79 4 4 4 4-1.79 4-4-1.79-4-4-4zm0 10c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z"
);
pub const ICON_VISIBILITY_SVG: &str = ms_svg!(
    "M12 6c3.79 0 7.17 2.13 8.82 5.5C19.17 14.87 15.79 17 12 17s-7.17-2.13-8.82-5.5C4.83 8.13 8.21 6 12 6m0-2C7 4 2.73 7.11 1 11.5 2.73 15.89 7 19 12 19s9.27-3.11 11-7.5C21.27 7.11 17 4 12 4zm0 5c1.38 0 2.5 1.12 2.5 2.5S13.38 14 12 14s-2.5-1.12-2.5-2.5S10.62 9 12 9m0-2c-2.48 0-4.5 2.02-4.5 4.5S9.52 16 12 16s4.5-2.02 4.5-4.5S14.48 7 12 7z"
);
pub const ICON_VISIBILITY_OFF_SVG: &str = ms_svg!(
    "M12 6c3.79 0 7.17 2.13 8.82 5.5-.59 1.22-1.46 2.29-2.52 3.15l1.45 1.45C21.15 14.66 22.5 13.2 23 11.5 21.27 7.11 17 4 12 4c-1.27 0-2.49.2-3.64.57l1.65 1.65C10.66 6.09 11.32 6 12 6zm-1.07 1.14L13 9.21c.57.25 1.03.71 1.28 1.28l2.07 2.07c.08-.34.14-.7.14-1.07C16.5 9.01 14.48 7 12 7c-.37 0-.72.05-1.07.14zM2.01 3.87l2.68 2.68C3.06 7.83 1.77 9.53 1 11.5 2.73 15.89 7 19 12 19c1.52 0 2.97-.3 4.31-.82l3.4 3.4 1.41-1.41L3.42 2.45 2.01 3.87zM7.53 9.8l1.55 1.55c-.05.21-.08.43-.08.65 0 1.66 1.34 3 3 3 .22 0 .44-.03.65-.08l1.55 1.55c-.67.33-1.41.53-2.2.53-2.76 0-5-2.24-5-5 0-.79.2-1.53.53-2.2zm4.31-.78l3.15 3.15.02-.16c0-1.66-1.34-3-3-3l-.17.01z"
);
pub const ICON_FAVORITE_SVG: &str = ms_svg!(
    "M16.5 3c-1.74 0-3.41.81-4.5 2.09C10.91 3.81 9.24 3 7.5 3 4.42 3 2 5.42 2 8.5c0 3.78 3.4 6.86 8.55 11.54L12 21.35l1.45-1.32C18.6 15.36 22 12.28 22 8.5 22 5.42 19.58 3 16.5 3zm-4.4 15.55l-.1.1-.1-.1C7.14 14.24 4 11.39 4 8.5 4 6.5 5.5 5 7.5 5c1.54 0 3.04.99 3.57 2.36h1.87C13.46 5.99 14.96 5 16.5 5c2 0 3.5 1.5 3.5 3.5 0 2.89-3.14 5.74-7.9 10.05z"
);
pub const ICON_STAR_SVG: &str = ms_svg!(
    "M22 9.24l-7.19-.62L12 2 9.19 8.63 2 9.24l5.46 4.73L5.82 21 12 17.27 18.18 21l-1.63-7.03L22 9.24zM12 15.4l-3.76 2.27 1-4.28-3.32-2.88 4.38-.38L12 6.1l1.71 4.04 4.38.38-3.32 2.88 1 4.28L12 15.4z"
);
pub const ICON_DOWNLOAD_SVG: &str =
    ms_svg!("M5 20h14v-2H5v2zM12 2v12.17l4.59-4.58L18 11l-6 6-6-6 1.41-1.41L11 14.17V2h1z");
pub const ICON_UPLOAD_SVG: &str =
    ms_svg!("M5 20h14v-2H5v2zM12 2L6 8l1.41 1.41L11 5.83V16h2V5.83l3.59 3.58L18 8l-6-6z");
pub const ICON_CIRCLE_SVG: &str = ms_svg!(
    "M12 2C6.47 2 2 6.47 2 12s4.47 10 10 10 10-4.47 10-10S17.53 2 12 2zm0 18c-4.41 0-8-3.59-8-8s3.59-8 8-8 8 3.59 8 8-3.59 8-8 8z"
);

// --- Filled (explicit *-fill / material-symbols:*-fill) ---

pub const ICON_HOME_FILL_SVG: &str = ms_svg!("M10 20v-6h4v6h5v-8h3L12 3L2 12h3v8z");
pub const ICON_ADD_FILL_SVG: &str = ICON_ADD_SVG;
pub const ICON_DELETE_FILL_SVG: &str = ms_svg!(
    "M6 19c0 1.1.9 2 2 2h8c1.1 0 2-.9 2-2V7H6zM19 4h-3.5l-1-1h-5l-1 1H5v2h14z"
);
pub const ICON_CLOSE_FILL_SVG: &str = ICON_CLOSE_SVG;
pub const ICON_MENU_FILL_SVG: &str = ICON_MENU_SVG;
pub const ICON_SEARCH_FILL_SVG: &str = ICON_SEARCH_SVG;
pub const ICON_SETTINGS_FILL_SVG: &str = ms_svg!(
    "M19.14 12.94c.04-.31.06-.63.06-.94 0-.32-.02-.63-.06-.94l2.03-1.58c.18-.14.23-.41.12-.61l-1.92-3.32c-.12-.22-.37-.29-.59-.22l-2.39.96c-.5-.38-1.03-.7-1.62-.94l-.36-2.54c-.04-.24-.24-.41-.48-.41h-3.84c-.24 0-.43.17-.47.41l-.36 2.54c-.59.24-1.13.57-1.62.94l-2.39-.96c-.22-.08-.47 0-.59.22L2.74 8.87c-.12.21-.08.47.12.61l2.03 1.58c-.04.31-.06.63-.06.94s.02.63.06.94l-2.03 1.58c-.18.14-.23.41-.12.61l1.92 3.32c.12.22.37.29.59.22l2.39-.96c.5.38 1.03.7 1.62.94l.36 2.54c.05.24.24.41.48.41h3.84c.24 0 .44-.17.47-.41l.36-2.54c.59-.24 1.13-.56 1.62-.94l2.39.96c.22.08.47 0 .59-.22l1.92-3.32c.12-.22.07-.47-.12-.61l-2.01-1.58zM12 15.6c-1.98 0-3.6-1.62-3.6-3.6s1.62-3.6 3.6-3.6 3.6 1.62 3.6 3.6-1.62 3.6-3.6 3.6z"
);
pub const ICON_EDIT_FILL_SVG: &str = ms_svg!(
    "M3 17.25V21h3.75L17.81 9.94l-3.75-3.75L3 17.25zM20.71 7.04c.39-.39.39-1.02 0-1.41l-2.34-2.34c-.39-.39-1.02-.39-1.41 0l-1.83 1.83 3.75 3.75 1.83-1.83z"
);
pub const ICON_CHECK_FILL_SVG: &str = ICON_CHECK_SVG;
pub const ICON_ARROW_BACK_FILL_SVG: &str = ICON_ARROW_BACK_SVG;
pub const ICON_ARROW_FORWARD_FILL_SVG: &str = ICON_ARROW_FORWARD_SVG;
pub const ICON_ARROW_UPWARD_FILL_SVG: &str = ICON_ARROW_UPWARD_SVG;
pub const ICON_ARROW_DOWNWARD_FILL_SVG: &str = ICON_ARROW_DOWNWARD_SVG;
pub const ICON_MORE_VERT_FILL_SVG: &str = ICON_MORE_VERT_SVG;
pub const ICON_MORE_HORIZ_FILL_SVG: &str = ICON_MORE_HORIZ_SVG;
pub const ICON_INFO_FILL_SVG: &str = ms_svg!(
    "M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 15h-2v-6h2v6zm0-8h-2V7h2v2z"
);
pub const ICON_WARNING_FILL_SVG: &str =
    ms_svg!("M1 21h22L12 2 1 21zm12-3h-2v-2h2v2zm0-4h-2v-4h2v4z");
pub const ICON_ERROR_FILL_SVG: &str = ms_svg!(
    "M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 15h-2v-2h2v2zm0-4h-2V7h2v6z"
);
pub const ICON_REFRESH_FILL_SVG: &str = ICON_REFRESH_SVG;
pub const ICON_PERSON_FILL_SVG: &str = ms_svg!(
    "M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z"
);
pub const ICON_VISIBILITY_FILL_SVG: &str = ms_svg!(
    "M12 4.5C7 4.5 2.73 7.61 1 12c1.73 4.39 6 7.5 11 7.5s9.27-3.11 11-7.5c-1.73-4.39-6-7.5-11-7.5zM12 17c-2.76 0-5-2.24-5-5s2.24-5 5-5 5 2.24 5 5-2.24 5-5 5zm0-8c-1.66 0-3 1.34-3 3s1.34 3 3 3 3-1.34 3-3-1.34-3-3-3z"
);
pub const ICON_VISIBILITY_OFF_FILL_SVG: &str = ms_svg!(
    "M12 7c2.76 0 5 2.24 5 5 0 .65-.13 1.26-.36 1.83l2.92 2.92c1.51-1.26 2.7-2.89 3.43-4.75-1.73-4.39-6-7.5-11-7.5-1.4 0-2.74.25-3.98.7l2.16 2.16C10.74 7.13 11.35 7 12 7zM2 4.27l2.28 2.28.46.46C3.08 8.3 1.78 10.02 1 12c1.73 4.39 6 7.5 11 7.5 1.55 0 3.03-.3 4.38-.84l.42.42L19.73 22 21 20.73 3.27 3 2 4.27zM7.53 9.8l1.55 1.55c-.05.21-.08.43-.08.65 0 1.66 1.34 3 3 3 .22 0 .44-.03.65-.08l1.55 1.55c-.67.33-1.41.53-2.2.53-2.76 0-5-2.24-5-5 0-.79.2-1.53.53-2.2zm4.31-.78l3.15 3.15.02-.16c0-1.66-1.34-3-3-3l-.17.01z"
);
pub const ICON_FAVORITE_FILL_SVG: &str = ms_svg!(
    "M12 21.35l-1.45-1.32C5.4 15.36 2 12.28 2 8.5 2 5.42 4.42 3 7.5 3c1.74 0 3.41.81 4.5 2.09C13.09 3.81 14.76 3 16.5 3 19.58 3 22 5.42 22 8.5c0 3.78-3.4 6.86-8.55 11.54L12 21.35z"
);
pub const ICON_STAR_FILL_SVG: &str =
    ms_svg!("M12 17.27L18.18 21l-1.64-7.03L22 9.24l-7.19-.61L12 2 9.19 8.63 2 9.24l5.46 4.73L5.82 21z");
pub const ICON_DOWNLOAD_FILL_SVG: &str = ICON_DOWNLOAD_SVG;
pub const ICON_UPLOAD_FILL_SVG: &str = ICON_UPLOAD_SVG;
pub const ICON_CIRCLE_FILL_SVG: &str =
    ms_svg!("M12 2C6.47 2 2 6.47 2 12s4.47 10 10 10 10-4.47 10-10S17.53 2 12 2z");

/// Base glyph names (without `material-symbols:` / `-fill`).
const CURATED_BASE_NAMES: &[&str] = &[
    "add",
    "arrow_back",
    "arrow_downward",
    "arrow_forward",
    "arrow_upward",
    "check",
    "circle",
    "close",
    "delete",
    "download",
    "edit",
    "error",
    "favorite",
    "home",
    "info",
    "menu",
    "more_horiz",
    "more_vert",
    "person",
    "refresh",
    "search",
    "settings",
    "star",
    "upload",
    "visibility",
    "visibility_off",
    "warning",
];

/// Canonical curated ids (outlined + filled). Keep in sync with [`iconify_svg`].
pub const CURATED_ICON_NAMES: &[&str] = &[
    "material-symbols:add",
    "material-symbols:add-fill",
    "material-symbols:arrow_back",
    "material-symbols:arrow_back-fill",
    "material-symbols:arrow_downward",
    "material-symbols:arrow_downward-fill",
    "material-symbols:arrow_forward",
    "material-symbols:arrow_forward-fill",
    "material-symbols:arrow_upward",
    "material-symbols:arrow_upward-fill",
    "material-symbols:check",
    "material-symbols:check-fill",
    "material-symbols:circle",
    "material-symbols:circle-fill",
    "material-symbols:close",
    "material-symbols:close-fill",
    "material-symbols:delete",
    "material-symbols:delete-fill",
    "material-symbols:download",
    "material-symbols:download-fill",
    "material-symbols:edit",
    "material-symbols:edit-fill",
    "material-symbols:error",
    "material-symbols:error-fill",
    "material-symbols:favorite",
    "material-symbols:favorite-fill",
    "material-symbols:home",
    "material-symbols:home-fill",
    "material-symbols:info",
    "material-symbols:info-fill",
    "material-symbols:menu",
    "material-symbols:menu-fill",
    "material-symbols:more_horiz",
    "material-symbols:more_horiz-fill",
    "material-symbols:more_vert",
    "material-symbols:more_vert-fill",
    "material-symbols:person",
    "material-symbols:person-fill",
    "material-symbols:refresh",
    "material-symbols:refresh-fill",
    "material-symbols:search",
    "material-symbols:search-fill",
    "material-symbols:settings",
    "material-symbols:settings-fill",
    "material-symbols:star",
    "material-symbols:star-fill",
    "material-symbols:upload",
    "material-symbols:upload-fill",
    "material-symbols:visibility",
    "material-symbols:visibility-fill",
    "material-symbols:visibility_off",
    "material-symbols:visibility_off-fill",
    "material-symbols:warning",
    "material-symbols:warning-fill",
];

fn short_to_base(name: &str) -> Option<&'static str> {
    match name {
        "add" | "plus" => Some("add"),
        "arrow_back" | "back" => Some("arrow_back"),
        "arrow_downward" | "arrow_down" | "down" => Some("arrow_downward"),
        "arrow_forward" | "forward" => Some("arrow_forward"),
        "arrow_upward" | "arrow_up" | "up" => Some("arrow_upward"),
        "check" | "done" => Some("check"),
        "circle" => Some("circle"),
        "close" | "clear" | "cancel" => Some("close"),
        "delete" | "trash" => Some("delete"),
        "download" => Some("download"),
        "edit" | "pencil" => Some("edit"),
        "error" => Some("error"),
        "favorite" | "heart" => Some("favorite"),
        "home" => Some("home"),
        "info" => Some("info"),
        "menu" | "hamburger" => Some("menu"),
        "more_horiz" | "more_horizontal" => Some("more_horiz"),
        "more_vert" | "more" | "more_vertical" => Some("more_vert"),
        "person" | "user" => Some("person"),
        "refresh" | "sync" => Some("refresh"),
        "search" => Some("search"),
        "settings" | "gear" => Some("settings"),
        "star" => Some("star"),
        "upload" => Some("upload"),
        "visibility" | "eye" => Some("visibility"),
        "visibility_off" | "eye_off" => Some("visibility_off"),
        "warning" => Some("warning"),
        _ => None,
    }
}

/// Map short / alternate names to canonical `material-symbols:*` ids.
///
/// Bare short names and `material-symbols:name` → **outlined**.
/// `name-fill` / `material-symbols:name-fill` → **filled**.
pub fn resolve_icon_name(name: &str) -> &str {
    if let Some(base) = name.strip_suffix("-fill").or_else(|| name.strip_suffix("_fill")) {
        if let Some(b) = short_to_base(base) {
            return fill_id(b);
        }
        if CURATED_BASE_NAMES.contains(&base) {
            return fill_id(base);
        }
    }
    if let Some(b) = short_to_base(name) {
        return outline_id(b);
    }
    if let Some(rest) = name.strip_prefix("material-symbols:") {
        if let Some(base) = rest.strip_suffix("-fill") {
            if CURATED_BASE_NAMES.contains(&base) {
                return fill_id(base);
            }
        } else if CURATED_BASE_NAMES.contains(&rest) {
            return outline_id(rest);
        }
    }
    name
}

fn outline_id(base: &str) -> &'static str {
    match base {
        "add" => "material-symbols:add",
        "arrow_back" => "material-symbols:arrow_back",
        "arrow_downward" => "material-symbols:arrow_downward",
        "arrow_forward" => "material-symbols:arrow_forward",
        "arrow_upward" => "material-symbols:arrow_upward",
        "check" => "material-symbols:check",
        "circle" => "material-symbols:circle",
        "close" => "material-symbols:close",
        "delete" => "material-symbols:delete",
        "download" => "material-symbols:download",
        "edit" => "material-symbols:edit",
        "error" => "material-symbols:error",
        "favorite" => "material-symbols:favorite",
        "home" => "material-symbols:home",
        "info" => "material-symbols:info",
        "menu" => "material-symbols:menu",
        "more_horiz" => "material-symbols:more_horiz",
        "more_vert" => "material-symbols:more_vert",
        "person" => "material-symbols:person",
        "refresh" => "material-symbols:refresh",
        "search" => "material-symbols:search",
        "settings" => "material-symbols:settings",
        "star" => "material-symbols:star",
        "upload" => "material-symbols:upload",
        "visibility" => "material-symbols:visibility",
        "visibility_off" => "material-symbols:visibility_off",
        "warning" => "material-symbols:warning",
        _ => "material-symbols:home",
    }
}

fn fill_id(base: &str) -> &'static str {
    match base {
        "add" => "material-symbols:add-fill",
        "arrow_back" => "material-symbols:arrow_back-fill",
        "arrow_downward" => "material-symbols:arrow_downward-fill",
        "arrow_forward" => "material-symbols:arrow_forward-fill",
        "arrow_upward" => "material-symbols:arrow_upward-fill",
        "check" => "material-symbols:check-fill",
        "circle" => "material-symbols:circle-fill",
        "close" => "material-symbols:close-fill",
        "delete" => "material-symbols:delete-fill",
        "download" => "material-symbols:download-fill",
        "edit" => "material-symbols:edit-fill",
        "error" => "material-symbols:error-fill",
        "favorite" => "material-symbols:favorite-fill",
        "home" => "material-symbols:home-fill",
        "info" => "material-symbols:info-fill",
        "menu" => "material-symbols:menu-fill",
        "more_horiz" => "material-symbols:more_horiz-fill",
        "more_vert" => "material-symbols:more_vert-fill",
        "person" => "material-symbols:person-fill",
        "refresh" => "material-symbols:refresh-fill",
        "search" => "material-symbols:search-fill",
        "settings" => "material-symbols:settings-fill",
        "star" => "material-symbols:star-fill",
        "upload" => "material-symbols:upload-fill",
        "visibility" => "material-symbols:visibility-fill",
        "visibility_off" => "material-symbols:visibility_off-fill",
        "warning" => "material-symbols:warning-fill",
        _ => "material-symbols:home-fill",
    }
}

/// Curated Material Symbols SVG by canonical (or short) name.
///
/// Returns `None` when the name is unknown — load via [`IconCache::load`] /
/// [`IconCache::load_path`] instead.
pub fn iconify_svg(name: &str) -> Option<&'static str> {
    match resolve_icon_name(name) {
        "material-symbols:add" => Some(ICON_ADD_SVG),
        "material-symbols:add-fill" => Some(ICON_ADD_FILL_SVG),
        "material-symbols:arrow_back" => Some(ICON_ARROW_BACK_SVG),
        "material-symbols:arrow_back-fill" => Some(ICON_ARROW_BACK_FILL_SVG),
        "material-symbols:arrow_downward" => Some(ICON_ARROW_DOWNWARD_SVG),
        "material-symbols:arrow_downward-fill" => Some(ICON_ARROW_DOWNWARD_FILL_SVG),
        "material-symbols:arrow_forward" => Some(ICON_ARROW_FORWARD_SVG),
        "material-symbols:arrow_forward-fill" => Some(ICON_ARROW_FORWARD_FILL_SVG),
        "material-symbols:arrow_upward" => Some(ICON_ARROW_UPWARD_SVG),
        "material-symbols:arrow_upward-fill" => Some(ICON_ARROW_UPWARD_FILL_SVG),
        "material-symbols:check" => Some(ICON_CHECK_SVG),
        "material-symbols:check-fill" => Some(ICON_CHECK_FILL_SVG),
        "material-symbols:circle" => Some(ICON_CIRCLE_SVG),
        "material-symbols:circle-fill" => Some(ICON_CIRCLE_FILL_SVG),
        "material-symbols:close" => Some(ICON_CLOSE_SVG),
        "material-symbols:close-fill" => Some(ICON_CLOSE_FILL_SVG),
        "material-symbols:delete" => Some(ICON_DELETE_SVG),
        "material-symbols:delete-fill" => Some(ICON_DELETE_FILL_SVG),
        "material-symbols:download" => Some(ICON_DOWNLOAD_SVG),
        "material-symbols:download-fill" => Some(ICON_DOWNLOAD_FILL_SVG),
        "material-symbols:edit" => Some(ICON_EDIT_SVG),
        "material-symbols:edit-fill" => Some(ICON_EDIT_FILL_SVG),
        "material-symbols:error" => Some(ICON_ERROR_SVG),
        "material-symbols:error-fill" => Some(ICON_ERROR_FILL_SVG),
        "material-symbols:favorite" => Some(ICON_FAVORITE_SVG),
        "material-symbols:favorite-fill" => Some(ICON_FAVORITE_FILL_SVG),
        "material-symbols:home" => Some(ICON_HOME_SVG),
        "material-symbols:home-fill" => Some(ICON_HOME_FILL_SVG),
        "material-symbols:info" => Some(ICON_INFO_SVG),
        "material-symbols:info-fill" => Some(ICON_INFO_FILL_SVG),
        "material-symbols:menu" => Some(ICON_MENU_SVG),
        "material-symbols:menu-fill" => Some(ICON_MENU_FILL_SVG),
        "material-symbols:more_horiz" => Some(ICON_MORE_HORIZ_SVG),
        "material-symbols:more_horiz-fill" => Some(ICON_MORE_HORIZ_FILL_SVG),
        "material-symbols:more_vert" => Some(ICON_MORE_VERT_SVG),
        "material-symbols:more_vert-fill" => Some(ICON_MORE_VERT_FILL_SVG),
        "material-symbols:person" => Some(ICON_PERSON_SVG),
        "material-symbols:person-fill" => Some(ICON_PERSON_FILL_SVG),
        "material-symbols:refresh" => Some(ICON_REFRESH_SVG),
        "material-symbols:refresh-fill" => Some(ICON_REFRESH_FILL_SVG),
        "material-symbols:search" => Some(ICON_SEARCH_SVG),
        "material-symbols:search-fill" => Some(ICON_SEARCH_FILL_SVG),
        "material-symbols:settings" => Some(ICON_SETTINGS_SVG),
        "material-symbols:settings-fill" => Some(ICON_SETTINGS_FILL_SVG),
        "material-symbols:star" => Some(ICON_STAR_SVG),
        "material-symbols:star-fill" => Some(ICON_STAR_FILL_SVG),
        "material-symbols:upload" => Some(ICON_UPLOAD_SVG),
        "material-symbols:upload-fill" => Some(ICON_UPLOAD_FILL_SVG),
        "material-symbols:visibility" => Some(ICON_VISIBILITY_SVG),
        "material-symbols:visibility-fill" => Some(ICON_VISIBILITY_FILL_SVG),
        "material-symbols:visibility_off" => Some(ICON_VISIBILITY_OFF_SVG),
        "material-symbols:visibility_off-fill" => Some(ICON_VISIBILITY_OFF_FILL_SVG),
        "material-symbols:warning" => Some(ICON_WARNING_SVG),
        "material-symbols:warning-fill" => Some(ICON_WARNING_FILL_SVG),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_short_aliases_to_outline() {
        assert_eq!(resolve_icon_name("home"), "material-symbols:home");
        assert_eq!(resolve_icon_name("circle"), "material-symbols:circle");
        assert_eq!(resolve_icon_name("plus"), "material-symbols:add");
        assert_eq!(
            resolve_icon_name("material-symbols:home"),
            "material-symbols:home"
        );
    }

    #[test]
    fn resolves_fill_aliases() {
        assert_eq!(resolve_icon_name("home-fill"), "material-symbols:home-fill");
        assert_eq!(resolve_icon_name("home_fill"), "material-symbols:home-fill");
        assert_eq!(
            resolve_icon_name("material-symbols:home-fill"),
            "material-symbols:home-fill"
        );
    }

    #[test]
    fn outline_and_fill_svgs_differ_for_home() {
        assert_ne!(
            iconify_svg("home").unwrap(),
            iconify_svg("home-fill").unwrap()
        );
        assert_eq!(iconify_svg("home").unwrap(), ICON_HOME_SVG);
        assert_eq!(iconify_svg("home-fill").unwrap(), ICON_HOME_FILL_SVG);
    }

    #[test]
    fn curated_table_covers_all_listed_names() {
        for name in CURATED_ICON_NAMES {
            assert!(
                iconify_svg(name).is_some(),
                "missing curated svg for {name}"
            );
        }
        assert_eq!(CURATED_ICON_NAMES.len(), CURATED_BASE_NAMES.len() * 2);
    }

    #[test]
    fn ensure_lazy_loads_curated_and_paints() {
        let mut cache = IconCache::new();
        let id = IconId("home".into());
        assert!(!cache.contains(&id));
        cache.ensure(&id).unwrap();
        assert!(cache.contains(&id));
        assert!(cache.contains(&IconId("material-symbols:home".into())));
        let mut scene = Scene::new();
        cache
            .paint(
                &mut scene,
                &id,
                Rect::new(0.0, 0.0, 24.0, 24.0),
                Color::from_rgb8(0x67, 0x50, 0xa4),
            )
            .unwrap();
    }

    #[test]
    fn unknown_icon_errors() {
        let mut cache = IconCache::new();
        let err = cache
            .ensure(&IconId("not-a-real-icon".into()))
            .unwrap_err();
        assert!(err.to_string().contains("not in curated set"));
    }
}
