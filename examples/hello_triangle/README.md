# hello_triangle (retained Engine integration)

This example is an explicit retained Engine/render boundary. It verifies the low-level `synui-render`, `winit`, surface, resize, and redraw path; it is not a facade-only application or Stable Component evidence. The stable replacement for ordinary UI is the documented `synui::prelude` path in [`docs/api-reference-v0.2.md`](../../docs/api-reference-v0.2.md).

Exact retained-boundary checks: `cargo check -p hello_triangle` and `cargo run -p hello_triangle`. Native window/GPU availability is reported by the platform workload.
