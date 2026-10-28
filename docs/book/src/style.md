# Style (CSS subset)

SynUI provides a **simplified style system** — CSS-*like* properties for layout
and a few paint tokens. It is **not** a browser stylesheet engine.

## What you get

| Channel | Type | Destination |
|---------|------|-------------|
| Layout | `LayoutStyle` / CSS box+flex+grid+position | Taffy |
| Visual | `VisualStyle` (background, radius, opacity, border) | Paint (`StyledBox`) |
| Aggregate | `Style` + non-panicking `StyleInput` | Both |

```rust
use synui::prelude::*;

// Prefer Styled::column / Styled::row when gap/align must apply to children:
Styled::column(
    "padding:16;gap:8;align-items:center;background:surface-container;border-radius:12;width:fill",
    [Label::new("Hi").node()],
)
.node()

// Wrap any existing Node (including Node::column) in an outer style host:
Node::column([Label::new("Card").node()])
    .styled("padding:12;background:surface;border-radius:8")
```

`Node::column` / `Node::row` set flex **direction only** (no CSS). `.styled(css)` and
`Styled::{new,column,row}` mount a `StyledBox` host via validated `StyleInput`.

## Supported CSS properties

- **Box:** `width` / `height` / `min-*` / `max-*` (`px`, `%`, `auto`, `fill`)
- **Spacing:** `padding` / `margin` (1–4 values), `gap` / `row-gap` / `column-gap`
- **Flex:** `display` (`flex` \| `none`), `flex-direction`, `justify-content`,
  `align-items`, `align-self`, `flex-grow` / `flex-shrink` / `flex-basis`
- **Grid:** `display:grid` **requires** `grid-template-columns` and/or
  `grid-template-rows` (explicit `px` / `fr` / `auto` tracks only — no `repeat(auto-fit)`,
  no named areas)
- **Position:** `position:relative|absolute|sticky|fixed` (also `static` → relative); inset
  `top` / `right` / `bottom` / `left` in **px**. Absolute children leave normal flex/grid
  flow (Taffy). **Sticky** (vertical MVP): pins to the nearest clip ancestor using `top`.
  **Fixed:** viewport insets; does not scroll with `ScrollView` content.
- **Stacking:** `z-index` (`integer` or `auto`) orders paint/hit among siblings. Overlay
  stack (menus/dialogs) still paints **above** all CSS z-index content.
- **Visual:** `background` (`#rgb` / `#rrggbb` or theme token), `border-radius`,
  `opacity`, `border` / `border-width` / `border-color`, and per-side
  `border-top` / `border-right` / `border-bottom` / `border-left`
  (`1px solid #888` or `1px solid outline`)

Theme tokens: `primary`, `surface`, `surface-container`, `outline`, `error`, …

**Unknown properties hard-fail** (`StyleInput` / `StyleCache` → `StyleDiagnostic`).

Demo: `cargo run -p style_demo` and `cargo run -p style_sticky_demo`.

## Overlays

Dropdown / bottom sheet / dialog placement goes through `OverlayPlacement`
(`Below` / `Above` / `Start` / `End` / `Center` / `BottomSheet`) via
`PopupAnchor` — not ad hoc viewport math alone.

## Not supported (by design)

Id selectors, `:nth-child` / `:not()`, width/height `@media`, container queries,
horizontal sticky, full CSS stacking-context rules, `float`, arbitrary-node overflow,
transforms/filters/animations in style sheets.

**Phase 40 cascade v2:** specificity (class count), `!important`, `:hover` / `:focus` /
`:disabled`, multi-class `.a.b`, descendant `.a .b`, and
`@media (prefers-color-scheme: dark|light)`. Local CSS may use `!important`.
Hover/focus recompute via interaction-driven cascade refresh.
Theme light/dark flips clear text caches and Damaged-repaint widgets **without**
`scene.reset()` (`force_theme_full_reset` / `SYNUI_THEME_FULL_RESET` escape hatch).

**Phase 41:** `:focus` matches Styled hosts when focus is in the subtree (like `:hover`);
`Button` / `TextField` implement real `is_disabled()`.
Overlay open/close uses dirty-rect Damaged erase (`force_overlay_full_paint` escape).

**Phase 35+ supported:** narrow inheritance (`color`, `font_size`, `opacity`). See
[Declarative](declarative.md#style-cascade-mvp-phase-35).

ScrollView transparent virt of **arbitrary nested** flex/grid trees (auto-scan) remains
hard-locked. Nested virt via independent scrollables + wheel chaining is available.

Use prelude `Scroll`, overlays, and theme tokens for those concerns.

## Hot reload / plugins

`synui_create_view` descriptors may include a quoted style field:

```text
kind=1;title=Hot;button=+1;style="padding:24;gap:12;background:primary"
```

Or a full JSON tree when the descriptor starts with `{` (kind 2):

```json
{"type":"column","style":"padding:16;gap:8","children":[{"type":"label","text":"Hi"}]}
```

Plugins may export `synui_plugin_style()` and `synui_plugin_create_view()`;
hosts use `plugin_styled_panel` / `plugin_view_page`. These paths are experimental /
non-Production-ABI.

## Examples

```bash
cargo run -p style_demo
cargo run -p style_sticky_demo
cargo run -p widget_gallery_facade
```
