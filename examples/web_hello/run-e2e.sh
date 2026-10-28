#!/usr/bin/env bash
# Build web_hello wasm package and run Playwright smoke tests.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

./examples/web_hello/build-web.sh

E2E="examples/web_hello/e2e"
cd "$E2E"
if [[ ! -d node_modules ]]; then
  npm install
fi
npx playwright install --with-deps chromium
npm test
