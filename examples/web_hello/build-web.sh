#!/usr/bin/env bash
# Build web_hello for the browser and write JS+WASM into www/pkg/
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

#!/usr/bin/env bash
# Keep wasm-bindgen-cli in lockstep with the crate (see Cargo.lock).
WB_VER="$(cargo tree -p wasm-bindgen -i wasm-bindgen --depth 0 2>/dev/null | sed -n 's/.*wasm-bindgen v\([0-9.]*\).*/\1/p' | head -1)"
WB_VER="${WB_VER:-0.2.125}"

need_install=1
if command -v wasm-bindgen >/dev/null 2>&1; then
  cur="$(wasm-bindgen --version 2>/dev/null | awk '{print $2}')"
  if [[ "$cur" == "$WB_VER" ]]; then
    need_install=0
  fi
fi
if [[ "$need_install" -eq 1 ]]; then
  echo "Installing wasm-bindgen-cli ${WB_VER}..."
  cargo install wasm-bindgen-cli --version "$WB_VER" --locked --force
fi

rustup target add wasm32-unknown-unknown >/dev/null

echo "Building cdylib for wasm32..."
# Build lib only to avoid bin/lib .wasm name collision.
# Hybrid (wgpu 29) is cfg'd off on wasm32 so wasm-bindgen sees one web-sys Gpu* set.
cargo build -p web_hello --lib --target wasm32-unknown-unknown --release

OUT="examples/web_hello/www/pkg"
mkdir -p "$OUT"
WASM="target/wasm32-unknown-unknown/release/web_hello.wasm"
if [[ ! -f "$WASM" ]]; then
  echo "missing $WASM" >&2
  exit 1
fi

echo "Generating JS glue → $OUT"
wasm-bindgen "$WASM" --target web --out-dir "$OUT" --no-typescript

echo
echo "Done. Serve and open:"
echo "  cd examples/web_hello/www && python3 -m http.server 8080"
echo "  open http://127.0.0.1:8080/"
