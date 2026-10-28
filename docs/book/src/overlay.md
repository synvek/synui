# Overlay Architecture

Overlays are managed by `WidgetStore::overlay_stack`:

- `push_overlay(root, OverlayConfig::modal(), focus_trap)` — modal layer with scrim
- `pop_overlay()` — dismiss top layer (Escape / backdrop click)
- Pointer events hit-test overlays before the main tree

`ModalHost` is a full-viewport container for centered dialog content. `PopupAnchor` in `synui-layout` positions dropdown menus relative to anchor bounds.
