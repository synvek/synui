//! Phase 45: facade RichDocument — true RichEdit host + Markdown export/load.

use synui::prelude::*;

fn initial_document() -> BlockDoc {
    let mut document = BlockDoc::from_plain("Hello RichEdit\nSelect text and press B / I / U");
    if let Some(inline) = document.get_inline_mut(0) {
        inline.spans.push(TextSpan {
            range: 0..5,
            bold: true,
            italic: false,
            underline: false,
            font_size: None,
            color: None,
        });
    }
    document
}

fn main() -> Result<(), PlatformError> {
    let app = App::new(AppConfig::default())?;
    let context = app.context();
    let document = context.state(initial_document())?;
    let exported = context.state(String::new())?;

    let on_change = {
        let document = document.clone();
        context.action(move |cx, next: BlockDoc| {
            let _ = document.set_in(cx, next);
        })?
    };
    let export_markdown = {
        let document = document.clone();
        let exported = exported.clone();
        context.action(move |cx, ()| {
            if let Ok(current) = document.get_in(cx) {
                let _ = exported.set_in(cx, current.to_markdown());
            }
        })?
    };
    let load_sample = {
        let document = document.clone();
        context.action(move |cx, ()| {
            let sample =
                BlockDoc::from_markdown("# Sample\n**Bold** and *italic* sample\n\n- List item");
            let _ = document.set_in(cx, sample);
        })?
    };

    app.run(Window::new("rich_edit_demo"), move |cx| {
        let current = document.get_in(cx)?;
        let preview = exported.get_in(cx)?;
        Ok(Node::column([
            Node::text("SynUI RichEdit — prelude RichDocument"),
            Node::text("Shortcuts: Ctrl/Cmd+B I U Z · Enter splits block · Tab list"),
            Node::row([
                Button::new("Export MD")
                    .on_press(export_markdown.clone())
                    .node(),
                Button::new("Load sample")
                    .on_press(load_sample.clone())
                    .node(),
            ]),
            RichDocument::new(current.clone())
                .on_change(on_change.clone())
                .node()
                .key(BusinessKey::new("rich-document")),
            Node::text(if preview.is_empty() {
                "Export Markdown to preview…".to_owned()
            } else {
                format!("MD: {}", preview.chars().take(120).collect::<String>())
            })
            .key(BusinessKey::new("markdown-preview")),
            Node::text(format!("blocks={}", current.blocks.len()))
                .key(BusinessKey::new("block-count")),
        ]))
    })
}
