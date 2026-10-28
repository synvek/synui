# Gallery: facade vs Engine capability matrix

Phase 56 (Wave O). Ordinary gallery is [`examples/widget_gallery_facade`](../examples/widget_gallery_facade). The Engine gold tree was **physically deleted** in Phase 52 (see [DEVELOPMENT Phase 52 notes](DEVELOPMENT.md#phase-52-notes-gold-tree-deletion--wave-k)).

**Boundary:** prelude Elements = in-app hosts via `EngineAdapter`. Platform `DesktopMenu`/muda is a shell option and is **not** listed as a facade gallery row. Full interactive demos remain dedicated packages.

| Capability | Engine gold (deleted) | Facade `widget_gallery_facade` | Notes |
|------------|----------------------|--------------------------------|-------|
| Button / Label | Deleted (Phase 52) | Yes | Prelude `Label` + paint-width ellipsis when `truncate` |
| TextInput / Checkbox / Number / Select | Deleted (Phase 52) | Yes | Controlled `InputConfig`; Volume drag must not vanish; focus-protected controlled sync (Wave O) |
| TextArea | Deleted (Phase 52) | Yes (smoke) | Phase 55 Wave N; multiline `InputConfig<String>`; label is a11y name only |
| Switch / Radio / LinearProgress | Deleted (Phase 52) | Yes (smoke) | Phase 53 Wave L; Switch ≠ Checkbox |
| Card / ListItem / CircularProgress | Deleted (Phase 52) | Yes (smoke) | Phase 54–56; CircularProgress determinate arc + indeterminate short arc |
| Scroll + sticky/fixed (Styled) | Deleted (Phase 52) | Yes (smoke) | Phase 55 Wave N; full sticky suite remains `style_sticky_demo` |
| Lottie / MorphingShape | Deleted (Phase 52) | Yes (smoke, prelude) | Phase 57 Wave P; prelude + `Window::with_lottie` / `Lottie::from_asset`; Phase 59 gallery `playing(true)` while Animation mounted |
| VirtualList | Deleted (Phase 52) | Yes (smoke) | Full 10k: `list_demo` |
| DataTable | Deleted (Phase 52) | Yes (smoke) | Full + merge spans: `table_demo` |
| VirtualTree | Deleted (Phase 52) | Yes (smoke) | Full: `tree_list_demo` |
| SplitPane / TabBar | Deleted (Phase 52) | Yes (smoke) | Full split: `data_browser_demo` |
| RichDocument | Deleted (Phase 52) | Yes (smoke) | Full: `rich_*` demos |
| AppBar / Drawer / BottomNav | Deleted (Phase 52) | Yes (smoke) | Full chrome: `navigation_demo` |
| Dialog / BottomSheet / SnackBar | Deleted (Phase 52) | Yes (smoke) | Full: `navigation_demo` |
| Icon / Image | Deleted (Phase 52) | Yes (smoke) | Image via `Window::with_image`; SVG crisp + `currentColor`; full: `show_image` |
| **MenuBar (in-app)** | Deleted (Phase 52) | **Yes** | Phase 46+ |
| **ContextMenu (in-app)** | Deleted (Phase 52) | **Yes** | Right-click or `show_at` |
| DesktopMenu / muda | Shell optional | Shell optional | Not prelude |
| GTK true menus | No | No | Hard lock |
| Floating dock / Cascade v3 / OT | No | No | Hard lock |
| `synui` `legacy` feature | Removed (v0.3) | N/A | Phase 51 |

## Smoke

```bash
cargo check -p widget_gallery_facade
cargo test -p synui --lib facade_menu_bar_and_context_menu_mount_and_dispatch_actions
python3 scripts/migration_gate.py --facade-regression
python3 scripts/migration_manifest.py --check
```
