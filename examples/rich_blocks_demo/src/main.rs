//! Phase 45: BlockDoc headings/lists/images with RichDocument host + Markdown I/O.

use synui::prelude::*;

fn sample_document() -> BlockDoc {
    BlockDoc::from_blocks(vec![
        Block::heading(1, "Wave C BlockDoc"),
        Block::heading(2, "Editing"),
        Block::paragraph("Markdown round-trip keeps headings, paragraphs, and lists editable."),
        Block::bullet(0, "Bullet one"),
        Block::bullet(1, "Nested bullet"),
        Block::ordered(0, 1, "Ordered first"),
        Block::ordered(0, 2, "Ordered second"),
        Block::heading(3, "Image slot"),
        Block::image("gallery"),
    ])
}

fn main() -> Result<(), PlatformError> {
    let app = App::new(AppConfig::default())?;
    let context = app.context();
    let document = context.state(sample_document())?;
    let preview = context.state(String::new())?;

    let on_change = {
        let document = document.clone();
        context.action(move |cx, next: BlockDoc| {
            let _ = document.set_in(cx, next);
        })?
    };
    let export_markdown = {
        let document = document.clone();
        let preview = preview.clone();
        context.action(move |cx, ()| {
            if let Ok(current) = document.get_in(cx) {
                let _ = preview.set_in(cx, current.to_markdown());
            }
        })?
    };
    let import_markdown = {
        let document = document.clone();
        let preview = preview.clone();
        context.action(move |cx, ()| {
            if let Ok(markdown) = preview.get_in(cx) {
                if !markdown.is_empty() {
                    let _ = document.set_in(cx, BlockDoc::from_markdown(&markdown));
                }
            }
        })?
    };
    let reload = {
        let document = document.clone();
        context.action(move |cx, ()| {
            let _ = document.set_in(cx, sample_document());
        })?
    };

    app.run(Window::new("rich_blocks_demo"), move |cx| {
        let current = document.get_in(cx)?;
        let markdown = preview.get_in(cx)?;
        Ok(Node::column([
            Node::text("SynUI rich_blocks_demo — prelude RichDocument"),
            Node::text("Headings · lists · image slots · Markdown import/export"),
            Node::row([
                Button::new("Export MD")
                    .on_press(export_markdown.clone())
                    .node(),
                Button::new("Import MD")
                    .on_press(import_markdown.clone())
                    .node(),
                Button::new("Reload sample").on_press(reload.clone()).node(),
            ]),
            RichDocument::new(current.clone())
                .on_change(on_change.clone())
                .node()
                .key(BusinessKey::new("block-document")),
            Node::text(if markdown.is_empty() {
                "Export Markdown to preview…".to_owned()
            } else {
                format!("MD: {}", markdown.chars().take(200).collect::<String>())
            }),
            Node::text(format!("blocks={}", current.blocks.len())),
        ]))
    })
}
