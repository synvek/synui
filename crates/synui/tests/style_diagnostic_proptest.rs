//! Property tests for atomic, non-panicking style application.

use std::panic::{catch_unwind, AssertUnwindSafe};

use proptest::prelude::*;
use synui::prelude::*;

fn source_span(enabled: bool, start: usize, end: usize) -> Option<SourceSpan> {
    enabled.then_some(SourceSpan { start, end })
}

fn invalid_input(kind: u8, token: u8, source: Option<SourceSpan>) -> StyleInput {
    let text = match kind % 4 {
        0 => format!("unknown-property-{token}:1"),
        1 => "padding:-1".to_owned(),
        2 => "opacity:2".to_owned(),
        _ => return StyleInput::Rust(Style::new().gap(-1.0)),
    };
    StyleInput::Text { text, source }
}

// Feature: synui-api-abi-reset, Property 7: 样式诊断的原子且非 panic 应用
proptest! {
    #![proptest_config(ProptestConfig {
        cases: 100,
        max_shrink_iters: 256,
        .. ProptestConfig::default()
    })]
    #[test]
    fn invalid_styles_are_diagnostic_successes_and_preserve_the_last_valid_style(
        padding in 0.0f32..=100.0,
        gap in 0.0f32..=100.0,
        opacity in 0.0f32..=1.0,
        kind in 0u8..4,
        token in any::<u8>(),
        has_source in any::<bool>(),
        source_start in 0usize..10_000,
        source_width in 0usize..100,
    ) {
        let baseline = StyleInput::text(format!(
            "padding:{padding};gap:{gap};opacity:{opacity}"
        ));
        let mut cache = StyleCache::default();
        let baseline_result = cache.apply(baseline);
        prop_assert!(baseline_result.applied);
        prop_assert!(baseline_result.diagnostic.is_none());
        let previous = cache.last_successful().cloned();
        prop_assert!(previous.is_some());

        let source = source_span(
            has_source,
            source_start,
            source_start.saturating_add(source_width),
        );
        let input = invalid_input(kind, token, source.clone());
        let result = catch_unwind(AssertUnwindSafe(|| cache.apply(input)));
        prop_assert!(result.is_ok(), "invalid style application must not panic");
        let result = result.expect("panic was checked above");
        prop_assert!(!result.applied);
        prop_assert!(result.is_diagnostic_success());
        let diagnostic = result.diagnostic.expect("diagnostic success has a diagnostic");
        prop_assert_eq!(diagnostic.severity, StyleSeverity::Error);
        prop_assert!(!diagnostic.code.is_empty());
        prop_assert!(!diagnostic.message.is_empty());
        prop_assert!(!diagnostic.property_or_token.is_empty());
        if kind % 4 == 3 {
            prop_assert_eq!(diagnostic.source, None);
        } else {
            prop_assert_eq!(diagnostic.source, source);
        }
        prop_assert_eq!(cache.last_successful(), previous.as_ref());
    }
}
