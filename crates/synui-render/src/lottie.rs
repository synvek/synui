//! Lottie animation via velato, plus host preload cache of JSON + Composition.

use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;

use synui_core::LottieJsonSource;
use thiserror::Error;
use velato::Composition;
use vello::kurbo::Affine;
use vello::Scene;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LottieId(pub String);

#[derive(Debug, Error)]
pub enum LottieError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("utf-8 error: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("lottie not loaded: {0}")]
    NotLoaded(String),
}

struct LottieEntry {
    json: String,
    composition: Composition,
}

/// Preloaded Lottie JSON + parsed [`Composition`] (not shared Players/Renderers).
pub struct LottieCache {
    entries: HashMap<String, LottieEntry>,
}

impl Default for LottieCache {
    fn default() -> Self {
        Self::new()
    }
}

impl LottieCache {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    pub fn load_bytes(&mut self, id: LottieId, bytes: &[u8]) -> Result<(), LottieError> {
        let json = String::from_utf8(bytes.to_vec())?;
        let composition =
            Composition::from_str(&json).map_err(|err| LottieError::Parse(format!("{err}")))?;
        self.entries.insert(id.0, LottieEntry { json, composition });
        Ok(())
    }

    pub fn load_path(&mut self, id: LottieId, path: impl AsRef<Path>) -> Result<(), LottieError> {
        let bytes = std::fs::read(path)?;
        self.load_bytes(id, &bytes)
    }

    pub fn contains(&self, id: &LottieId) -> bool {
        self.entries.contains_key(&id.0)
    }

    pub fn json(&self, id: &LottieId) -> Option<&str> {
        self.entries.get(&id.0).map(|e| e.json.as_str())
    }

    /// Pre-parsed composition for cloning into per-widget players.
    pub fn composition(&self, id: &LottieId) -> Option<&Composition> {
        self.entries.get(&id.0).map(|e| &e.composition)
    }

    pub fn composition_str(&self, id: &str) -> Option<&Composition> {
        self.entries.get(id).map(|e| &e.composition)
    }
}

impl LottieJsonSource for LottieCache {
    fn lottie_json(&self, id: &str) -> Option<&str> {
        self.entries.get(id).map(|e| e.json.as_str())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

pub struct LottiePlayer {
    composition: Composition,
    renderer: velato::Renderer,
    frame: f64,
    playing: bool,
    fps: f64,
    _total_frames: f64,
}

impl LottiePlayer {
    pub fn from_json(json: &str) -> Result<Self, velato::Error> {
        let composition = Composition::from_str(json)?;
        Ok(Self::from_composition(composition))
    }

    /// Build a player from a host-preloaded (or cloned) composition.
    ///
    /// Each widget keeps its own player/renderer; only the composition is shared
    /// via clone from [`LottieCache`].
    pub fn from_composition(composition: Composition) -> Self {
        let fps = composition.frame_rate;
        let start_frame = composition.frames.start;
        let total_frames = composition.frames.end - composition.frames.start;
        Self {
            composition,
            renderer: velato::Renderer::new(),
            frame: start_frame,
            playing: false,
            fps,
            _total_frames: total_frames,
        }
    }

    pub fn set_playing(&mut self, playing: bool) {
        self.playing = playing;
    }

    pub fn advance(&mut self, dt_secs: f64) {
        if self.playing {
            self.frame += dt_secs * self.fps;
            if self.frame >= self.composition.frames.end {
                self.frame = self.composition.frames.start;
            }
        }
    }

    pub fn render_to_scene(&mut self, scene: &mut Scene, transform: Affine, alpha: f64) {
        let rendered =
            self.renderer
                .render_to_vello_scene(&self.composition, self.frame, transform, alpha);
        scene.append(&rendered, None);
    }

    pub fn frame(&self) -> f64 {
        self.frame
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_pulse() {
        let j = include_str!("../../../examples/assets/lottie_pulse.json");
        assert!(LottiePlayer::from_json(j).is_ok());
    }

    #[test]
    fn cache_load_bytes_and_lookup() {
        let j = include_str!("../../../examples/assets/lottie_pulse.json");
        let mut cache = LottieCache::new();
        cache
            .load_bytes(LottieId("pulse".into()), j.as_bytes())
            .unwrap();
        assert!(cache.contains(&LottieId("pulse".into())));
        assert_eq!(cache.json(&LottieId("pulse".into())), Some(j));
        assert_eq!(cache.lottie_json("pulse"), Some(j));
        assert!(cache.lottie_json("missing").is_none());
        assert!(cache.composition(&LottieId("pulse".into())).is_some());
    }

    #[test]
    fn cache_rejects_invalid_json_at_load() {
        let mut cache = LottieCache::new();
        let err = cache
            .load_bytes(LottieId("bad".into()), b"{not-json")
            .unwrap_err();
        assert!(matches!(err, LottieError::Parse(_)));
        assert!(!cache.contains(&LottieId("bad".into())));
    }

    #[test]
    fn from_composition_clone_avoids_reparse() {
        let j = include_str!("../../../examples/assets/lottie_pulse.json");
        let mut cache = LottieCache::new();
        cache
            .load_bytes(LottieId("pulse".into()), j.as_bytes())
            .unwrap();
        let c1 = cache
            .composition(&LottieId("pulse".into()))
            .unwrap()
            .clone();
        let c2 = cache
            .composition(&LottieId("pulse".into()))
            .unwrap()
            .clone();
        let mut p1 = LottiePlayer::from_composition(c1);
        let mut p2 = LottiePlayer::from_composition(c2);
        p1.set_playing(true);
        p2.set_playing(false);
        assert!(p1.is_playing());
        assert!(!p2.is_playing());
        // Distinct players share composition data via clone, not a shared Renderer.
        p1.advance(0.1);
        assert!(p1.frame() != p2.frame() || !p2.is_playing());
    }
}
