# Text Editing

`TextField`, `TextArea`, and `RichEdit` share [`EditCore`](../../crates/synui-widgets/src/edit_core.rs)
for plain-text mutation. `RichEdit` composes `EditCore` (plain layer) with `BlockDoc` blocks and
per-block `RichDoc` spans; insert/delete/selection/IME/shortcuts go through the same path.
Selection indices are UTF-8 **byte** offsets (`selection: Range<usize>`).
Moves snap to char boundaries.

## Selection

- Pointer drag extends `selection.end` (Parley hit-test, not 8px grid)
- Shift+Arrow extends; bare Arrow collapses then moves
- Cmd/Ctrl+A selects all; Ctrl/Cmd+Arrow moves by word; Home/End line ends
- Double-click selects the word under the caret
- Backspace/Delete remove the selected range (or one char)
- TextArea / RichEdit: ArrowUp/Down move by hard-newline lines; RichEdit
  scrolls to keep the caret visible

Selection highlight and caret use `TextPainter::caret_rect` /
`hit_test_xy` / `selection_rects` (Parley-backed). Styled hit-test uses the
same runs as paint. Shared access: `synui_render::with_text_renderer`.

## Clipboard & undo

- Cmd/Ctrl+C / X / V — copy / cut / paste (desktop `arboard`; wasm uses a
  process-local buffer)
- Cmd/Ctrl+Z undo; Shift+Z or Y redo (`EditHistory`, max 64 snapshots)
- winit IME Commit goes through the shared API and is undoable

## IME

Composition text is shown in-place over the selection span
(`composition_range`). Commit replaces the selection and places the caret
after the committed string. `ime_cursor_area()` uses Parley caret geometry
for TextField, TextArea, and RichEdit. During composition, RichEdit paints
plain text (no style-run indexing) so CJK preedit cannot desync spans.

## Focus-aware bindings

While a TextField / TextArea / RichEdit holds focus, declarative `patch` and
`Binding::push` do **not** overwrite widget text (external Signal updates wait
until blur). Use `TextBinding`, `TextAreaBinding`, and `RichBinding`.

## Bidi

`synui_render::paragraph_direction` and `logical_index_for_visual_x` document
the logical↔visual contract. Complex scripts are shaped by Parley/HarfRust;
widget code should not implement UAX#9 itself.

## RichEdit / BlockDoc

Phase 20: `RichEdit` edits a flat [`BlockDoc`](../../crates/synui-core/src/block_doc.rs)
(`Paragraph` / `Heading` / list items / `Image`). Focus is
`(block_index, byte_offset)`; only the active inline block uses `EditCore`.
Inline styles still use `TextSpan` / `RichDoc` as `InlineBlock`.

Phase 25: **cross-block selection** via `sel_anchor` + focus. Shift+Arrow / drag
extends across text blocks; Delete/Backspace and Ctrl/Cmd+C/X operate on the
document range (`BlockDoc::delete_range` / `plain_slice`). Image blocks are
skipped (atomic). IME composition blocks cross-block extend.

- **Enter** splits the current block; **Backspace** at offset 0 merges / outdents
- **Tab** / **Shift+Tab** list indent; Cmd/Ctrl+B / I / U on the selection
- **Markdown subset:** `to_markdown` / `from_markdown` (`#`, lists, bold/italic, `![](id)`)
- Flat `RichDoc` migrates via `BlockDoc::from_rich_doc` (split on `\n`)
- `RichBinding` is `Signal<BlockDoc>`

```bash
cargo run -p rich_blocks_demo
cargo run -p rich_edit_demo
```

**Not in this phase (Phase 39+):** HTML interchange, nested block trees, nested editable
tables, collaborative OT/CRDT, full CommonMark/GFM. Word boundaries remain
whitespace/punctuation MVP (not full UAX#29).

Legacy flat `RichDoc::to_json` / `from_json` still exists for single-inline
spans.