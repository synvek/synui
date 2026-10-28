//! Desktop shell helpers: native menus (muda), file dialogs (rfd), tray.

use std::path::PathBuf;

/// Menu item identifiers used by SynUI examples.
pub mod menu_id {
    pub const OPEN: &str = "synui.open";
    pub const QUIT: &str = "synui.quit";
    pub const TRAY_SHOW: &str = "synui.tray.show";
    pub const TRAY_QUIT: &str = "synui.tray.quit";
    pub const NEW_WINDOW: &str = "synui.window.new";
}

/// Native file dialog helpers (rfd).
pub struct FileDialog;

impl FileDialog {
    pub fn pick_file(filters: &[(&str, &[&str])]) -> Option<PathBuf> {
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        {
            let mut dialog = rfd::FileDialog::new();
            for (name, exts) in filters {
                dialog = dialog.add_filter(*name, exts);
            }
            return dialog.pick_file();
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            let _ = filters;
            None
        }
    }

    pub fn pick_and_read_text(filters: &[(&str, &[&str])]) -> Option<String> {
        let path = Self::pick_file(filters)?;
        std::fs::read_to_string(path).ok()
    }

    pub fn save_file(filters: &[(&str, &[&str])]) -> Option<PathBuf> {
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        {
            let mut dialog = rfd::FileDialog::new();
            for (name, exts) in filters {
                dialog = dialog.add_filter(*name, exts);
            }
            return dialog.save_file();
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            let _ = filters;
            None
        }
    }
}

/// Built-in File menu: Open… + New Window + Quit.
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
pub struct DesktopMenu {
    pub menu: muda::Menu,
    pub open_id: muda::MenuId,
}

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
impl DesktopMenu {
    pub fn file_menu() -> Result<Self, Box<dyn std::error::Error>> {
        use muda::{Menu, MenuItem, PredefinedMenuItem, Submenu};
        let open = MenuItem::with_id(menu_id::OPEN, "Open…", true, None);
        let open_id = open.id().clone();
        let new_win = MenuItem::with_id(menu_id::NEW_WINDOW, "New Window", true, None);
        let file = Submenu::with_items(
            "File",
            true,
            &[
                &open,
                &new_win,
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::quit(None),
            ],
        )?;
        let menu = Menu::new();
        menu.append(&file)?;
        Ok(Self { menu, open_id })
    }

    /// Attach the menu to the OS window.
    /// Returns `Ok(true)` when attached; `Ok(false)` when the host must use GTK /
    /// [`AppDriver::on_platform_fallback`] (`"menu"`).
    pub fn attach(
        &self,
        window: &winit::window::Window,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        #[cfg(target_os = "macos")]
        {
            let _ = window;
            self.menu.init_for_nsapp();
            Ok(true)
        }
        #[cfg(target_os = "windows")]
        {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            let handle = window
                .window_handle()
                .map_err(|e| format!("window handle: {e}"))?;
            match handle.as_raw() {
                RawWindowHandle::Win32(h) => {
                    let hwnd = h.hwnd.get() as isize;
                    unsafe { self.menu.init_for_hwnd(hwnd)? };
                    Ok(true)
                }
                _ => Err("expected Win32 window handle".into()),
            }
        }
        #[cfg(target_os = "linux")]
        {
            let _ = window;
            // Pure winit has no GTK window. Use [`Self::attach_gtk_hint`] docs
            // or call muda `init_for_gtk_window` from a GTK host.
            tracing::info!(
                "synui: Linux DesktopMenu not attached (no GTK window); use DesktopMenu::attach_gtk_hint / on_platform_fallback(\"menu\")"
            );
            Ok(false)
        }
    }

    /// Documented GTK attach helper (Phase 11).
    ///
    /// Pure winit apps cannot call this successfully without a real GTK window
    /// handle. Hosts that embed SynUI inside GTK should call:
    /// `menu.init_for_gtk_window(&gtk_window)` on the inner [`Self::menu`].
    /// Returns `Err` on non-Linux or when no GTK feature is linked — callers
    /// should fall back to [`crate::AppDriver::on_platform_fallback`].
    pub fn attach_gtk_hint(&self) -> Result<(), Box<dyn std::error::Error>> {
        #[cfg(target_os = "linux")]
        {
            tracing::info!(
                "synui: DesktopMenu::attach_gtk_hint — call muda::Menu::init_for_gtk_window on self.menu from your GTK host"
            );
            Err("synui: GTK window handle required; see desktop_shell.md".into())
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = self;
            Err("attach_gtk_hint is Linux-only".into())
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
pub struct DesktopMenu;

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
impl DesktopMenu {
    pub fn file_menu() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self)
    }

    pub fn attach(
        &self,
        _window: &winit::window::Window,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        Ok(false)
    }
}

/// Minimal system tray helper with Show / Quit.
pub struct AppTray;

impl AppTray {
    /// Try to install a tray icon with Show/Quit. Returns whether installation succeeded.
    /// Menu events use the same muda `MenuEvent` channel as [`DesktopMenu`] when a handler
    /// is registered via `Application::run`.
    pub fn install_minimal() -> bool {
        Self::install_with_menu()
    }

    /// Install tray + Show/Quit menu items (ids: [`menu_id::TRAY_SHOW`], [`menu_id::TRAY_QUIT`]).
    pub fn install_with_menu() -> bool {
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        {
            use muda::{Menu, MenuItem};
            // 16x16 solid purple RGBA icon.
            let mut rgba = vec![0u8; 16 * 16 * 4];
            for px in rgba.chunks_exact_mut(4) {
                px[0] = 0x67;
                px[1] = 0x50;
                px[2] = 0xa4;
                px[3] = 0xff;
            }
            let icon = match tray_icon::Icon::from_rgba(rgba, 16, 16) {
                Ok(i) => i,
                Err(e) => {
                    tracing::warn!("synui: tray icon: {e}");
                    return false;
                }
            };
            let show = MenuItem::with_id(menu_id::TRAY_SHOW, "Show", true, None);
            let quit = MenuItem::with_id(menu_id::TRAY_QUIT, "Quit", true, None);
            let menu = match Menu::with_items(&[&show, &quit]) {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!("synui: tray menu: {e}");
                    return false;
                }
            };
            match tray_icon::TrayIconBuilder::new()
                .with_tooltip("SynUI")
                .with_icon(icon)
                .with_menu(Box::new(menu))
                .build()
            {
                Ok(tray) => {
                    std::mem::forget(tray);
                    true
                }
                Err(e) => {
                    tracing::warn!("synui: tray install failed: {e}");
                    false
                }
            }
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            false
        }
    }
}
