//! Cross-platform facade counter.
//!
//! The ordinary UI is built with the curated `synui` facade. `synui-winit` is
//! retained only as the platform support boundary selected by Cargo features.
//!
//! Desktop: `cargo run -p mobile_hello`
//! Android: `cargo check -p mobile_hello --target aarch64-linux-android --features mobile`
//! Web: `cargo check -p mobile_hello --target wasm32-unknown-unknown --features web`

use synui::prelude::*;

fn main() -> Result<(), PlatformError> {
    let app = App::new(AppConfig::default())?;
    let context = app.context();
    let count = context.state(0_u32)?;
    let increment = context.action({
        let count = count.clone();
        move |_, (): ()| {
            count
                .set(count.get().expect("mobile_hello state is active") + 1)
                .expect("mobile_hello state is active");
        }
    })?;

    app.run(Window::new("SynUI mobile_hello"), move |_| {
        Ok(Node::column([
            Node::text("SynUI mobile_hello"),
            Node::text(format!("Count: {}", count.get()?)).key(BusinessKey::new("count")),
            Button::new("+1").on_press(increment.clone()).node(),
        ]))
    })
}
