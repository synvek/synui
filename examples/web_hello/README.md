# web_hello

Interactive browser counter. The ordinary UI entry uses the curated `synui`
facade (`App`, App-owned `State`/typed `Action`, `Node`, `Button`,
`BusinessKey`, and `platform::Window`). `synui-platform`/`synui-winit`,
`wasm-bindgen`, `web-sys`, and the retained Engine types used by `run_dom_a11y_demo`
are explicit web/host support boundaries, not ordinary facade types.

## Exact commands

```bash
# Facade/native host smoke
cargo check -p web_hello
cargo run -p web_hello

# Wasm target check
cargo check -p web_hello --target wasm32-unknown-unknown --features web

# Browser package build and host launch
./examples/web_hello/build-web.sh
cd examples/web_hello/www && python3 -m http.server 8080

# Browser history/launch and DOM accessibility smoke
./examples/web_hello/run-e2e.sh
```

Open <http://127.0.0.1:8080/>. The browser baseline is a counter whose wasm
start hook launches the facade App; the ARIA host remains host glue for launch
and DOM accessibility fallback. Canvas/WebGPU failures are reported as browser
host failures, not as evidence about the facade. Missing wasm target,
wasm-bindgen CLI, browser, WebGPU, or supporting toolchain is `unavailable` in
the platform Matrix; source/behavior failures are `failed`.

`run_dom_a11y_demo` intentionally remains an Engine/web support check: it
constructs a small retained `WidgetStore` tree and syncs `DomA11yTree`. It is
not a Stable Component or Prelude example.
