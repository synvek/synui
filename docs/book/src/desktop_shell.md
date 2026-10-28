# Desktop shell

Native menus (`muda`) and file dialogs (`rfd`) on macOS/Windows.

```rust
use synui::{menu_id, Application, DesktopMenu, FileDialog, AppDriver, WidgetStore};

let menu = DesktopMenu::file_menu()?;
Application::new(driver).with_menu(menu).run(event_loop())?;

// In AppDriver::on_menu:
if id == menu_id::OPEN {
    if let Some(text) = FileDialog::pick_and_read_text(&[("Text", &["txt"])]) {
        // load text into app state
    }
}
```

## Linux menu attach

On Linux, `DesktopMenu::attach` returns **`Ok(false)`** when no GTK window is
available (menu bar cannot be wired yet). The shell logs a hint and invokes
**`AppDriver::on_platform_fallback("menu", store)`** so apps can show an
in-app menu or status label instead of failing silently.

### `attach_gtk_hint` (Phase 11)

Pure winit apps have no GTK window handle. **`DesktopMenu::attach_gtk_hint`**
documents the integration path for GTK hosts:

```rust
// From a GTK host that owns the window:
menu.init_for_gtk_window(&gtk_window); // on DesktopMenu::menu (muda::Menu)
```

`attach_gtk_hint` returns `Err` on non-Linux or when no GTK window is linked —
callers should use **`on_platform_fallback("menu")`** instead.

```rust
impl AppDriver for MyDriver {
    fn on_platform_fallback(&mut self, feature: &str, _store: &mut WidgetStore) {
        if feature == "menu" {
            self.menu_fallback = true; // e.g. show "menu: fallback" in UI
        }
    }
}
```

See `examples/multi_window_demo` for the fallback label pattern.

`todo_list` File → Open… loads newline-separated tasks; Quit uses the platform predefined item.

Unavailable platforms compile with no-op stubs (Req 1.4).
