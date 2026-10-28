//! Multi-window demo: second native window shares App-owned `State`.
//!
//! Tray / DesktopMenu remain explicit host boundaries (not prelude Elements).

use synui::prelude::*;

struct TrayHostBoundary;

impl Element for TrayHostBoundary {
    fn describe(&self, _cx: &mut BuildCx) -> ElementSpec {
        ElementSpec::custom("tray-host", [])
            .property("tray-menu", "host-orchestration-boundary")
            .accessibility(
                Accessibility::new()
                    .role("generic")
                    .name("Tray host boundary"),
            )
    }
}

fn main() -> Result<(), PlatformError> {
    let app = App::new(AppConfig::default())?;
    let context = app.context();
    let count = context.state(0_u32)?;
    let increment = context.action({
        let count = count.clone();
        move |_, (): ()| {
            count
                .set(count.get().expect("count state is active") + 1)
                .expect("count state is active");
        }
    })?;

    let window = Window::new("multi_window_demo").with_second_window("SynUI Second Window");
    app.run(window, move |_| {
        Ok(Node::column([
            Label::new("multi_window_demo")
                .node()
                .key(BusinessKey::new("title")),
            Label::new(format!("Count: {}", count.get()?))
                .node()
                .key(BusinessKey::new("count")),
            Label::new("Both windows share this App-owned State; press +1 in either window.")
                .node()
                .key(BusinessKey::new("hint")),
            Button::new("+1").on_press(increment.clone()).node(),
            Label::new("Tray / DesktopMenu remain host boundaries (Non-equivalent).")
                .node()
                .key(BusinessKey::new("tray-note")),
            Node::boxed(Box::new(TrayHostBoundary)).key(BusinessKey::new("tray-host")),
        ]))
    })
}
