# Testing and Benchmarks

## Unit and integration tests

```bash
cargo test --workspace
```

Key suites:

- `crates/synui-testing/tests/golden_tests.rs` — golden PNG snapshots (`insta`) and interaction regressions
- `crates/synui-testing/tests/backend_gold_tests.rs` — **Req 2.9** Gpu↔Cpu pixel parity (+ StyledBox; Hybrid scene-bridge)
- `crates/synui-testing/tests/declarative_tests.rs` — keyed `ViewEngine` diff behavior
- `crates/synui-testing/tests/phase3_tests.rs` — overlay, dropdown, layout smoke tests

CI sets `INSTA_UPDATE=no` so snapshot drift fails the build.

## Golden PNG workflow

1. Change rendering code
2. Run `INSTA_UPDATE=1 cargo test -p synui-testing golden`
3. Review `crates/synui-testing/tests/snapshots/`
4. Commit updated `.snap` / `.snap.png` files

Headless rendering uses `HeadlessRenderer::with_cpu(width, height, use_cpu)`:

| `use_cpu` | Backend | Used for |
|-----------|---------|----------|
| `false` | Vello GPU-compute | `snapshot_rgba_gpu` |
| `true` | Vello CPU path | `snapshot_rgba_cpu` |

Both paths use AA=`Area` for comparable output on Linux CI.

## Req 2.9 backend gold (Phase 11–12)

`backend_gold_tests.rs` renders the same `WidgetStore` tree through Gpu and Cpu, then compares RGBA buffers with **`assert_rgba_within_tolerance`** (per-channel absolute diff ≤ **1**, i.e. 1/256 for solid fills; ≤ **64** for text/AA).

Coverage includes solid fills, **StyledBox** backgrounds, default-theme Label, Cupertino Switch, and Fluent Button.

**Hybrid:** `backend_hybrid_kind_and_scene_bridge_replay` asserts `RenderBackendKind::Hybrid` and replays a Vello scene into `vello_hybrid::Scene` via `replay_vello_scene` (windowless). Full Hybrid wgpu presentation remains surface-bound.

```bash
cargo test -p synui-testing --test backend_gold_tests
```

## Benchmarks

```bash
cargo bench -p synui-bench -- --sample-size 10
```

| Bench | Purpose |
|-------|---------|
| `layout_1000_labels` | Fast layout regression |
| `layout_10k_labels` | Req 4.1 scale baseline |
| `paint_1000_labels` | Paint hot path |
| `declarative_add_item_keyed_diff` | List growth with keyed diff |

Results are written to `target/criterion/` for HTML reports.
