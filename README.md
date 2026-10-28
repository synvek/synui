# SynUI

Cross-platform Rust GUI framework built on Vello, Winit, Taffy, and Parley.

## Quick start

```bash
cargo run -p hello_triangle
cargo run -p counter
cargo run -p todo_list
cargo run -p navigation_demo
cargo run -p multi_window_demo
cargo run -p style_demo
cargo run -p rich_edit_demo
cargo run -p rich_blocks_demo
cargo run -p list_demo
cargo run -p tree_list_demo
cargo run -p table_demo
cargo run -p data_browser_demo
cargo run -p style_sticky_demo
cargo run -p widget_gallery_facade
cargo run -p mobile_hello
cargo run -p web_hello          # browser: ./examples/web_hello/build-web.sh
```

See [docs/book/src/platform_examples.md](docs/book/src/platform_examples.md) and [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).

## Platform support

- **Desktop**: muda/rfd menus, tray, multi-window; App facade
- **Web**: Application + Vello path (`web_hello`); DomA11y parallel; CI `wasm32`
- **Mobile**: `mobile_hello` (+ target checks)
