//! Phase 45: facade Image Element with Window::with_image preload.

use std::path::PathBuf;

use synui::prelude::*;

const IMAGE_ID: &str = "cat";

fn asset_path(file: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("assets")
        .join(file)
}

fn main() -> Result<(), PlatformError> {
    let app = App::new(AppConfig::default())?;
    let window = Window::new("show_image").with_image(IMAGE_ID, asset_path("cat.png"));
    app.run(window, |_| {
        Ok(Node::column([
            Node::text("assets/cat.png"),
            Image::new(IMAGE_ID).size(480.0, 320.0).node(),
        ]))
    })
}
