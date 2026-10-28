//! Minimal Counter using only SynUI v0.2's supported public prelude.
//!
//! The facade owns state and callbacks through an [`AppContext`]. The app mounts
//! the declarative root in a native window; rebuilding after a press reads the
//! updated App-scoped value.

use synui::prelude::*;

fn counter_root(button: &Button, count: &State<i32>) -> Result<Node, AppError> {
    Ok(Node::container([
        Node::text(format!("Count: {}", count.get()?)),
        button.node(),
    ]))
}

fn main() -> Result<(), PlatformError> {
    let app = App::new(AppConfig::default())?;
    let context = app.context();
    let count = context.state(0_i32)?;
    let increment = context.action({
        let count = count.clone();
        move |_, (): ()| {
            let next = count.get().expect("counter state is active") + 1;
            count.set(next).expect("counter state is active");
        }
    })?;
    let button = Button::new("+1").on_press(increment);

    app.run(Window::new("SynUI Counter"), move |_| {
        counter_root(&button, &count)
    })
}
