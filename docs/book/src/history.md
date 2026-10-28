# History API (wasm)

Phase 11 bridges typed `Router` navigation to the browser **History API** on
wasm32 with full **deep-link** support. Desktop builds ignore history calls (no-op).

## Enable sync

```rust
use synui::{Route, Router};

#[derive(Clone, PartialEq)]
enum AppRoute { Home, Settings }
impl Route for AppRoute {
    fn path_segment(&self) -> String {
        match self {
            AppRoute::Home => "home".into(),
            AppRoute::Settings => "settings".into(),
        }
    }
    fn from_path_segment(seg: &str) -> Option<Self> {
        match seg {
            "home" => Some(AppRoute::Home),
            "settings" => Some(AppRoute::Settings),
            _ => None,
        }
    }
}

let router = Router::new(AppRoute::Home).with_history_sync(true);
```

## Behavior

| Router call | History effect (wasm) |
|-------------|----------------------|
| `push(route)` | `history.pushState(null, "", "/{segment}")` |
| `replace(route)` | `history.replaceState(...)` |
| `pop()` | `history.replaceState` to match stack top (avoids double `popstate`) |

Implement **`Route::path_segment`** and **`Route::from_path_segment`** so URLs
round-trip.

### `popstate` → Router (Phase 11)

On `popstate`, the listener parses `window.location.pathname` and queues the
segment via **`take_pending_history_path`**. Each frame, call
**`router.sync_pending_history()`** (typically from `AppDriver::on_frame`) to
consume the queue and invoke **`Router::apply_path_segment`**.

Boot: `with_history_sync(true)` reads the current URL and `replace`s or queues
the initial route without writing history.

## Demo

**`navigation_demo`** enables history sync on wasm32:

```bash
cargo run -p navigation_demo                    # desktop
./examples/web_hello/build-web.sh             # or wasm build for browser
```

In the browser, forward/back updates the Router stack; Escape pops in-app.

## Back / Escape

Wire `App::with_back_handler` to `router.pop()` when `router.can_pop()` so
browser back and in-app back stay aligned when history sync is enabled.

See also [router.md](./router.md) and `crates/synui/src/nav.rs`.
