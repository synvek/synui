//! Sticky / fixed / z-index demo via facade `Scroll` + `Styled`.

use synui::prelude::*;

fn style_diagnostic_report() -> (String, bool) {
    let mut cache = StyleCache::default();
    let _ = cache.apply(StyleInput::text("padding:8;gap:4"));
    let previous = cache.last_successful().cloned();
    let result = cache.apply(StyleInput::text("gap:-1"));
    let diagnostic = result
        .diagnostic
        .expect("invalid runtime style must return a diagnostic");
    (
        format!(
            "StyleDiagnostic {{ code: {}, property: {} }}",
            diagnostic.code, diagnostic.property_or_token
        ),
        previous == cache.last_successful().cloned(),
    )
}

fn main() -> Result<(), PlatformError> {
    let app = App::new(AppConfig::default())?;
    let context = app.context();
    let (message, retained) = style_diagnostic_report();
    let status = context.state(message)?;
    let retained_state = context.state(retained)?;
    let recheck = {
        let status = status.clone();
        let retained_state = retained_state.clone();
        context.action(move |cx, ()| {
            let (message, retained) = style_diagnostic_report();
            let _ = status.set_in(cx, message);
            let _ = retained_state.set_in(cx, retained);
        })?
    };

    app.run(Window::new("style_sticky_demo"), move |cx| {
        let mut body = vec![
            Styled::sticky_top(0.0, Node::text("Sticky top bar — scroll this page"))
                .node()
                .key(BusinessKey::new("sticky-header")),
            Node::text("Scroll content below. The bar sticks to the ScrollView top.")
                .key(BusinessKey::new("scroll-help")),
        ];
        for index in 0..40 {
            body.push(
                Node::text(format!("Row {index} — filler for scrolling"))
                    .key(BusinessKey::new(format!("row:{index}"))),
            );
        }
        body.extend([
            Styled::new(
                "position:fixed;top:16;right:16;z-index:20;padding:10;background:primary;border-radius:8",
                [Node::text("Fixed badge (viewport)")],
            )
            .node()
            .key(BusinessKey::new("fixed-badge")),
            Styled::new(
                "position:absolute;top:120;left:40;z-index:1;padding:16;background:surface-variant;width:140",
                [Node::text("z-index:1 box")],
            )
            .node()
            .key(BusinessKey::new("z-index-1")),
            Styled::new(
                "position:absolute;top:140;left:80;z-index:3;padding:16;background:secondary;width:140",
                [Node::text("z-index:3 overlaps")],
            )
            .node()
            .key(BusinessKey::new("z-index-3")),
            Node::text(status.get_in(cx)?)
                .key(BusinessKey::new("style-diagnostic")),
            Node::text(format!(
                "invalid-style-retained={}",
                retained_state.get_in(cx)?
            ))
            .key(BusinessKey::new("retained")),
        ]);

        Ok(Node::column([
            Node::text("SynUI sticky/scroll — facade Scroll + Styled"),
            Scroll::new(body)
                .node()
                .key(BusinessKey::new("sticky-scroll")),
            Button::new("Re-check invalid runtime style")
                .on_press(recheck.clone())
                .node(),
        ]))
    })
}
