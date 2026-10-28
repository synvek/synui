# Icons

SynUI ships a **curated** set of Material Symbols–style glyphs for toolbar /
button icons. Names resolve through `synui_render::resolve_icon_name` and
`iconify_svg`; the host `IconCache` **lazy-loads** them on first paint.

## Quick start (facade)

```rust
use synui::prelude::*;

// Short alias → outlined (default)
Icon::new("home").size(24.0).node()

// Same outlined glyph by canonical id
Icon::new("material-symbols:search").size(24.0).node()

// Filled variant
Icon::new("home-fill").size(24.0).node()
// or: Icon::new("material-symbols:home-fill")
```

No preload is required for curated names. Custom SVGs:

```rust
Window::new("app")
    .with_icon("brand-mark", "assets/brand.svg");

// …
Icon::new("brand-mark").size(28.0).node()
```

Use `fill="currentColor"` (or strokes with `currentColor`) in the SVG so tinting works.

## Tint / button states

`Icon` paints with:

1. Explicit engine override (`Icon::with_color`), else
2. Style cascade `color` → host `PaintCtx.text_color`, else
3. Theme `on_surface`

So **hover / pressed / disabled** colors come from CSS (or parent `paint_inherit`),
not from swapping glyph assets:

```rust
Node::styled("icon-btn")
    .child(Icon::new("settings").size(24.0).node())
```

```css
.icon-btn { color: var(--md-sys-color-primary); }
.icon-btn:hover { color: var(--md-sys-color-on-surface); }
.icon-btn:disabled { color: var(--md-sys-color-on-surface-variant); }
```

For **swapping** artwork at runtime, prefer `Image` + SVG / `Window::with_image`.

## Curated names

Each base glyph has **outline** (default) and **fill** entries in
`synui_render::CURATED_ICON_NAMES`.

| Form | Example | Style |
|------|---------|--------|
| Short | `home`, `settings`, `plus` | outlined |
| Canonical | `material-symbols:home` | outlined |
| Short fill | `home-fill` / `home_fill` | filled |
| Canonical fill | `material-symbols:home-fill` | filled |

Short aliases include: `home`, `add`/`plus`, `delete`/`trash`, `close`/`clear`,
`menu`, `search`, `settings`/`gear`, `edit`, `check`/`done`, `arrow_back`/`back`,
`arrow_forward`, `arrow_upward`, `arrow_downward`, `more_vert`/`more`,
`more_horiz`, `info`, `warning`, `error`, `refresh`, `person`/`user`,
`visibility`/`eye`, `visibility_off`, `favorite`/`heart`, `star`, `download`,
`upload`, `circle`.

## Extending the curated table

Edit [`crates/synui-render/src/icon.rs`](../../../crates/synui-render/src/icon.rs):

1. Add `ICON_*_SVG` (outline) and `ICON_*_FILL_SVG` with `ms_svg!("…path…")`.
2. Add both arms in `iconify_svg` (`material-symbols:name` and `…-fill`).
3. Append both ids to `CURATED_ICON_NAMES` and the base to `CURATED_BASE_NAMES`.
4. Wire `short_to_base` / `outline_id` / `fill_id`.
5. Run `cargo test -p synui-render --lib icon::tests`.

### App-only icons (no framework change)

```rust
Window::new("app").with_icon("my-icon", "assets/my-icon.svg");
icons.load(IconId("my-icon".into()), include_str!("../assets/my-icon.svg"))?;
```

## Image vs Icon

| Need | Prefer |
|------|--------|
| Toolbar / button glyph, theme tint, crisp scale | `Icon` |
| Photo / illustration / multi-color asset | `Image` |

## Engine note

Native `synui_widgets::Icon` accepts `IconId` and optional `with_color`.
Navigation chrome that calls `paint_icon` with an explicit color already
supports selected / unselected tinting independently of cascade.
