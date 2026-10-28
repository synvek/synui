# Platform examples: mobile_hello & web_hello

Phase 7+ thin examples exercise cross-platform lifecycle, web accessibility, and the shared `Application` paint path.

## mobile_hello

Declarative counter used as the mobile entry-point smoke app.

### Desktop

```bash
cargo run -p mobile_hello
```

Runs on the normal winit desktop loop. Tap / click **+1** to increment.

### Mobile feature flags

```bash
cargo check -p mobile_hello --features mobile
```

| Target | Command (toolchain required) |
|--------|------------------------------|
| Android | `cargo check -p mobile_hello --target aarch64-linux-android --features mobile` |
| iOS | `cargo check -p mobile_hello --target aarch64-apple-ios --features mobile` |

`--features mobile` enables `synui-winit/mobile`, which picks `AndroidLifecycle` / `IosLifecycle` via `cfg(target_os)`. Those structs expose safe-area insets and soft-keyboard visibility for the host to fill from NDK/UIKit hooks.

## web_hello

Interactive counter via **`Application` + Vello/WebGPU** when the browser exposes WebGPU, with **DomA11y** mirroring the same widget tree under `#synui-a11y-host`. Click **+1** on the canvas or the ARIA button.

### Native (no browser)

```bash
cargo run -p web_hello
```

### Browser

```bash
./examples/web_hello/build-web.sh
cd examples/web_hello/www && python3 -m http.server 8080
# open http://127.0.0.1:8080/ — click +1; DevTools shows ARIA nodes
```

`build-web.sh` runs `cargo build` for `wasm32-unknown-unknown`, then `wasm-bindgen` into `www/pkg/`. **CI runs this script on Ubuntu** after `cargo check -p web_hello --target wasm32-unknown-unknown`.

If GPU/surface init fails, SynUI writes a message to **`#synui-error`** (hidden by default in `index.html`; shown with red text on failure) and logs to the console.

## Mobile hosts

Use `mobile_hello` for ordinary Android/iOS target smoke. Historical Engine
`android_host` / `ios_host` gold sources were deleted in Phase 52.

## See also

- [desktop_shell.md](./desktop_shell.md) — muda/rfd menus
- [DEVELOPMENT.md](../DEVELOPMENT.md) — platform feature matrix and benches
- [docs/gallery_facade_vs_engine.md](../gallery_facade_vs_engine.md) — facade vs archived Engine gallery
