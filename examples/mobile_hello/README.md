# mobile_hello

Cross-platform counter whose ordinary UI uses the curated `synui` facade:
`App`, App-owned `State`/typed `Action`, `Node`, `Button`, `BusinessKey`, and
`platform::Window`. `synui-winit` is a declared host-support boundary selected
by Cargo features; it is not imported as a Prelude component.

## Declared target checks

```bash
# Facade-only desktop compilation
cargo check -p mobile_hello

# Android host-support compilation (target/toolchain required)
cargo check -p mobile_hello --target aarch64-linux-android --features mobile

# Web host-support compilation (wasm target required)
cargo check -p mobile_hello --target wasm32-unknown-unknown --features web
```

The desktop launch baseline is a native counter window. Android and web launch
behavior is owned by their host integration; missing targets/toolchains are
reported by the platform Validation Matrix as `unavailable`, never `passed`.
This example has no history or safe-area implementation of its own.

Full usage: [docs/book/src/platform_examples.md](../../docs/book/src/platform_examples.md).
