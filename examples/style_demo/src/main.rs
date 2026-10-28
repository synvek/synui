//! Wave G facade style showcase: prelude `Styled` layouts + StyleCache diagnostics.

use synui::prelude::*;

fn invalid_runtime_style_report() -> (String, bool) {
    let mut cache = StyleCache::default();
    let _ = cache.apply(StyleInput::text("padding:8;gap:12;opacity:0.95"));
    let previous = cache.last_successful().cloned();
    let result = cache.apply(StyleInput::text("not-a-property:1"));
    let diagnostic = result
        .diagnostic
        .expect("invalid runtime style must return a diagnostic");
    (
        format!(
            "invalid runtime style → StyleDiagnostic {{ code: {}, property: {} }}",
            diagnostic.code, diagnostic.property_or_token
        ),
        previous == cache.last_successful().cloned(),
    )
}

fn main() -> Result<(), PlatformError> {
    let app = App::new(AppConfig::default())?;
    let context = app.context();
    let initial = invalid_runtime_style_report();
    let report = context.state(initial.0)?;
    let retained = context.state(initial.1)?;
    let check_invalid = {
        let report = report.clone();
        let retained = retained.clone();
        context.action(move |cx, ()| {
            let (message, still_retained) = invalid_runtime_style_report();
            let _ = report.set_in(cx, message);
            let _ = retained.set_in(cx, still_retained);
        })?
    };

    app.run(Window::new("style_demo"), move |cx| {
        Ok(Styled::new(
            "padding:24;gap:12;width:fill;height:fill",
            [
                Label::new("SynUI Style — facade Styled MVP + StyleCache")
                    .node()
                    .key(BusinessKey::new("heading")),
                Label::new("Typed Style builder: padding / gap / opacity")
                    .node()
                    .key(BusinessKey::new("typed")),
                Label::new("CSS Grid 3 columns")
                    .node()
                    .key(BusinessKey::new("grid-title")),
                Styled::new(
                    "display:grid;grid-template-columns:1fr 1fr 1fr;gap:8;padding:8;width:fill",
                    [
                        Styled::new(
                            "padding:16;background:#e8def8;border-radius:8;width:fill",
                            [Label::new("A").node()],
                        )
                        .node()
                        .key(BusinessKey::new("cell-a")),
                        Styled::new(
                            "padding:16;background:primary;border-radius:8;width:fill",
                            [Label::new("B").node()],
                        )
                        .node()
                        .key(BusinessKey::new("cell-b")),
                        Styled::new(
                            "padding:16;background:#cce8cc;border-radius:8;width:fill",
                            [Label::new("C").node()],
                        )
                        .node()
                        .key(BusinessKey::new("cell-c")),
                    ],
                )
                .node()
                .key(BusinessKey::new("grid")),
                Node::row([
                    Styled::new(
                        "padding:16;flex-grow:1;background:#ddeeff;border-radius:8;margin:4",
                        [Label::new("flex-grow 1").node()],
                    )
                    .node()
                    .key(BusinessKey::new("flex-1")),
                    Styled::new(
                        "padding:16;flex-grow:2;background:#ffeedd;border-radius:8;margin:4",
                        [Label::new("flex-grow 2").node()],
                    )
                    .node()
                    .key(BusinessKey::new("flex-2")),
                ])
                .key(BusinessKey::new("flex-row")),
                Label::new("Absolute badge (relative parent)")
                    .node()
                    .key(BusinessKey::new("abs-title")),
                Styled::new(
                    "position:relative;padding:8;background:surface-container;border-radius:8;width:fill;min-height:80",
                    [
                        Styled::new(
                            "padding:24;width:fill",
                            [Label::new("Card body").node()],
                        )
                        .node()
                        .key(BusinessKey::new("card-body")),
                        Styled::new(
                            "position:absolute;top:4;right:4;padding:4;background:error;border-radius:4",
                            [Label::new("NEW").node()],
                        )
                        .node()
                        .key(BusinessKey::new("badge")),
                    ],
                )
                .node()
                .key(BusinessKey::new("relative-card")),
                Label::new(report.get_in(cx)?)
                    .node()
                    .key(BusinessKey::new("diagnostic")),
                Label::new(format!(
                    "last-successful-style-retained={}",
                    retained.get_in(cx)?
                ))
                .node()
                .key(BusinessKey::new("retained")),
                Button::new("Try invalid runtime style")
                    .on_press(check_invalid.clone())
                    .node(),
            ],
        )
        .node())
    })
}
