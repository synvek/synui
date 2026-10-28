# Router

Typed navigation stack for multi-screen apps (Phase 8).

```rust
use synui::{App, Application, Route, Router, View, event_loop};

#[derive(Clone, PartialEq)]
enum RouteId { List, Detail(u32), Settings }
impl Route for RouteId {}

let router = Router::new(RouteId::List);
let nav = router.clone();
let app = App::new(move |_| match nav.current() {
    RouteId::List => View::button("Open").on_press({
        let r = nav.clone();
        move || r.push(RouteId::Detail(1))
    }),
    RouteId::Detail(id) => View::column(vec![
        View::label(format!("#{id}")),
        View::button("Back").on_press({
            let r = nav.clone();
            move || { r.pop(); }
        }),
    ]),
    RouteId::Settings => View::label("Settings"),
});
Application::new(app).run(event_loop())?;
```

`Router::push` / `pop` / `replace` / `current` / `can_pop` schedule a rebuild via `Signal`.

See `examples/navigation_demo`.
