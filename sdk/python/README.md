# SynUI Python SDK

This package is a cffi **ABI-mode** wrapper over the versioned `synui-abi` dynamic library. It never imports a Rust crate and does not expose plugin, JSON-view, hot-reload, or Engine symbols.

```python
from synui import Abi, Runtime

abi = Abi.load("/absolute/path/to/libsynui_abi.dylib")
runtime_result = Runtime.create(abi)
runtime = runtime_result.value
info = runtime.query().value
app = runtime.create_app().value

app.close()
runtime.close()
```

Every successful command returns `SynuiResult(value, NoError())`. Every failed command raises `SynuiError` carrying the matching ABI status and copied Error Record. ABI-owned records are freed immediately after copying.

`Runtime` and `App` are created by ABI v1. `IntState`, `ActionCallback`, `TextElement`, `ButtonElement`, and `render()` form the retained Counter path: a button callback mutates Context-owned integer State, application code converts the value to text with `TextElement.set_text()`, and the copied UTF-8 snapshot changes only on the following render batch. `TextElement.rendered_text()` copies an ABI-owned buffer and releases it immediately. See `fixtures/counter.py`.

`App.post()` may be called from worker threads; `App.drain_posts()` is UI-thread-only, and `PostToken.query()` reports accepted/running/executed/cancelled/failed. cffi enters Python with the GIL held, payload bytes are copied before user code, and `BaseException` is converted to `SYNUI_CALLBACK_ERROR` rather than crossing the ABI.

`Window`, `Element`, and `Callback` are exported wrapper types, but their ABI v1 ownership APIs remain safe placeholders without fake constructors. `VirtualDataTableModel.create(app, provider)` is a real Context-owned `Model`: its ABI vtable supplies dimensions, bounded half-open cell requests, cell updates, sort changes, and selection changes. Viewport requests are clipped and expanded by the Rust public `VirtualDataTableAdapter`, not calculated by Python; see `fixtures/virtual_data_table.py` for the 100,000-row path.

Every owned handle has a lock-backed, exactly-once close gate shared by explicit `close()`, context-manager exit, and `weakref.finalize`. Closing waits for concurrent operations, rejects new operations once closing begins, and invokes the matching ABI release exactly once across those paths; the finalizer is only a fallback, so deterministic code should use `with` or `close()`.

Library discovery checks an explicit path, then `SYNUI_ABI_LIBRARY`, the platform linker lookup, and standard library names. Production deployments should pass or configure an explicit released library path.
