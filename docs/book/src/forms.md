# Forms

`TextBinding` keeps a `Signal<String>` in sync with a keyed `TextField`.
`TextAreaBinding` does the same for `TextArea`. `RichBinding` syncs `RichDoc`.
`FormState` + `FieldValidator` handle validation (including `validate_textarea`).

While the bound widget is focused, push/patch skips overwriting text so typing
is not clobbered by Signal rebuilds.

## Automatic `.bind()` sync (Phase 10)

Call `.bind()` on a `TextField`, `.bind_textarea()` on a `TextArea`,
`.bind_bool()` on checkbox/switch, or `.bind_number()` on a slider. The `App`
runtime registers the binding and runs **automatic pull/push every frame** —
you do **not** need `with_frame_sync` / `with_after_apply` for bound fields.

```rust
use synui::{App, FieldValidator, FormState, TextBinding, View};

let draft = std::sync::Arc::new(TextBinding::new(101, ""));

App::new(|_| {
    View::column(vec![
        View::text_field("Name").with_key(101).bind(&draft),
        View::button("Submit").on_press(|| { /* validate */ }),
    ])
});
```

Validators: `FieldValidator::Required`, `MinLen(n)`, `Custom(fn)`.

Empty submit in `examples/todo_list` shows an inline error via `FormState::set_error`.
