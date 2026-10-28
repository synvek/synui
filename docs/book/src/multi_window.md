# Multi-window

Phase 10: each window has its own GPU surface and paint loop; the same
`AppDriver` UI is applied to every window.

```rust
use synui::{event_loop, menu_id, Application, AppTray, DesktopMenu, run_app_signal, Signal, View};

let tray_ok = AppTray::install_with_menu(); // Show + Quit tray items
let menu = DesktopMenu::file_menu()?;
run_app_signal(Signal::new(0u32), |n| {
    View::column(vec![
        View::label(format!("Count: {n}")),
        View::label("File → New Window opens another painted surface"),
    ])
})?;
Application::new(/* driver from run_app_signal internals */)
    .with_title("Main")
    .with_second_window("Second")
    .with_menu(menu)
    .run(event_loop())?;
```

## Per-window surfaces

`Application` tracks a primary window plus `extra_windows`. Each entry owns a
Vello/wgpu surface; resize and `render_frame` run per `WindowId`. Both windows
show the same declarative tree (shared `Signal` state).

## Menu: New Window

File → **New Window** activates `menu_id::NEW_WINDOW` (`"synui.window.new"`).
The shell calls `open_extra_window` with the configured secondary title.

## Tray Show / Quit

`AppTray::install_with_menu()` adds a system tray with:

| Item | `menu_id` |
|------|-----------|
| Show | `menu_id::TRAY_SHOW` |
| Quit | `menu_id::TRAY_QUIT` |

Quit exits the event loop; Show focuses the primary window (platform-dependent).

See `examples/multi_window_demo`.
