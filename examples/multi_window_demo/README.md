# multi_window_demo

Wave G facade demo. **Status: migrated**. The primary App opens a second native window via `Window::with_second_window`; both windows share the same App-owned `State` / `Action` graph. Tray and DesktopMenu remain explicit host boundaries (Non-equivalent).

## Behavior

- Counter `State` updates are visible in both windows after `+1`.
- Second window title: `SynUI Second Window`.
- Tray / shell menu are documented host boundaries, not prelude Elements.

## Legacy comparison

Before v0.2, the example used `Application`/`AppDriver`, `Signal`, root `View`, `ViewEngine`, `WidgetStore`, `DesktopMenu`, and `AppTray`.

After v0.2, the host uses facade `App`/`Window::with_second_window`, AppContext-owned `State`, typed `Action`, and prelude `Label`/`Button`. Tray parity stays a host boundary.

See [the behavior baseline](../../docs/baselines/multi_window_demo.md) and [the migration guide](../../docs/migration-v0.2.md).

## Exact commands

```bash
cargo check -p multi_window_demo
cargo test -p synui --test counter_prelude_fixture
python3 scripts/migration_manifest.py --check
```

Native dual-window behavior requires a desktop environment.
