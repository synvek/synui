# Tooltips

Hover tips are short, non-interactive overlays shown after a brief dwell
(~500ms). They do not take focus or block pointer hits.

## Facade usage

```rust
use synui::prelude::*;

Button::new("Save")
    .tooltip("Write changes to disk")
    .on_press(save)
    .node()

Icon::new("settings")
    .size(24.0)
    .tooltip("Open settings")
    .node()

// Any built-in Node:
Label::new("?").node().tooltip("Help for this field")
```

Custom SVGs / other elements: prefer `.tooltip` on `Button` / `Icon`, or
`Node::tooltip` on Builtin specs. Tips are stored on the mounted widget id via
`WidgetStore::set_widget_tooltip` (ancestor walk finds the nearest tip).

## Behaviour

| Event | Effect |
|-------|--------|
| Hover tip target ≥ delay | Show bubble above (or below if no room) |
| Leave / new target | Hide; restart delay for the new target |
| Pointer down | Hide immediately |
| Modal overlay open | Tips suppressed |

Colour: `surface_container` fill, light `field_border` stroke, `on_surface` text.

## Engine notes

- Bubble: `synui_widgets::TooltipBubble` (`hit_test` always false).
- Session: `WidgetStore::tooltip_session` + `tooltip_on_hover` / `tooltip_tick` /
  `sync_overlays`.
- Overlay config: `OverlayConfig::tooltip()` (non-modal, no backdrop dismiss).

## Not in v1

Rich HTML tips, cursor-following tips, keyboard-only tip popups (use a11y
`name` for assistive tech).
