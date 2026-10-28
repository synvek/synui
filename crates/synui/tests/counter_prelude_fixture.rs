//! Consumer fixture: imports only the supported v0.2 Prelude.
//!
//! This is the facade-compilation/component-interaction fixture for the stable
//! text, container, input, style, and desktop-host contracts.

use synui::prelude::*;

#[test]
fn counter_uses_only_the_curated_prelude() {
    let app = App::new(AppConfig::default()).unwrap();
    let context = app.context();
    let count = context.state(0_i32).unwrap();
    let increment = context
        .action({
            let count = count.clone();
            move |_, (): ()| count.set(count.get().unwrap() + 1).unwrap()
        })
        .unwrap();
    let button = Button::new("Increment").on_press(increment);

    button.press(&context).unwrap();
    assert_eq!(count.get().unwrap(), 1);

    let root = Node::container([button.node(), Node::text(count.get().unwrap().to_string())]);
    assert_eq!(root.children().len(), 2);
    let mut styles = StyleCache::default();
    assert!(
        styles
            .apply(StyleInput::Macro(style!(padding: 8; gap: 4)))
            .applied
    );
}

#[test]
fn app_state_actions_and_rendering_are_update_stable() {
    let app = App::new(AppConfig::default()).unwrap();
    let context = app.context();
    let count = context.state(0_i32).unwrap();
    let increment = context
        .action({
            let count = count.clone();
            move |_, (): ()| count.set(count.get().unwrap() + 1).unwrap()
        })
        .unwrap();
    let button = Button::new("Increment").on_press(increment);

    let render = || {
        Node::container([
            Node::text(format!("count={}", count.get().unwrap())),
            button.node(),
        ])
    };
    let before = render().describe(&mut BuildCx::new());
    assert_eq!(
        before.children()[0]
            .describe(&mut BuildCx::new())
            .text_content(),
        Some("count=0")
    );

    button.press(&context).unwrap();
    let after = render().describe(&mut BuildCx::new());
    assert_eq!(
        after.children()[0]
            .describe(&mut BuildCx::new())
            .text_content(),
        Some("count=1")
    );
}

#[test]
fn input_selection_and_business_keys_stay_in_the_facade_contract() {
    let app = App::new(AppConfig::default()).unwrap();
    let context = app.context();

    let text = context.state(String::from("before")).unwrap();
    let text_change = {
        let text = text.clone();
        context
            .action(move |_, value: String| text.set(value).unwrap())
            .unwrap()
    };
    let controlled_text =
        TextInput::new("controlled").controlled(text.clone(), text_change.clone());
    text_change.invoke(&context, String::from("after")).unwrap();
    assert_eq!(text.get().unwrap(), "after");
    let controlled_spec = controlled_text.node().describe(&mut BuildCx::new());
    assert_eq!(
        controlled_spec.accessibility_metadata().role_name(),
        Some("text-input")
    );
    assert_eq!(controlled_spec.event_slots()[0].event(), &Event::Change);

    let uncontrolled_seen = context.state(String::new()).unwrap();
    let uncontrolled_action = {
        let uncontrolled_seen = uncontrolled_seen.clone();
        context
            .action(move |_, value: String| uncontrolled_seen.set(value).unwrap())
            .unwrap()
    };
    let uncontrolled_text =
        TextInput::new("uncontrolled").uncontrolled("initial", Some(uncontrolled_action.clone()));
    uncontrolled_action
        .invoke(&context, String::from("native-value"))
        .unwrap();
    assert_eq!(uncontrolled_seen.get().unwrap(), "native-value");
    assert_eq!(
        uncontrolled_text
            .node()
            .describe(&mut BuildCx::new())
            .kind(),
        &ElementKind::Custom("text-input".into())
    );

    let checked = context.state(false).unwrap();
    let checked_change = {
        let checked = checked.clone();
        context
            .action(move |_, value: bool| checked.set(value).unwrap())
            .unwrap()
    };
    let checkbox = Checkbox::new(false).controlled(checked.clone(), checked_change.clone());
    checked_change.invoke(&context, true).unwrap();
    assert!(checked.get().unwrap());
    assert_eq!(
        checkbox
            .node()
            .describe(&mut BuildCx::new())
            .accessibility_metadata()
            .checked_state(),
        Some(true)
    );

    let number = context.state(1.0_f64).unwrap();
    let number_change = context
        .action({
            let number = number.clone();
            move |_, value: f64| number.set(value).unwrap()
        })
        .unwrap();
    let number_input = NumberInput::new(1.0)
        .range(0.0, 10.0)
        .controlled(number, number_change);
    assert_eq!(
        number_input
            .node()
            .describe(&mut BuildCx::new())
            .accessibility_metadata()
            .role_name(),
        Some("slider")
    );

    let choice = context.state(ChoiceId::new(0)).unwrap();
    let choice_change = context
        .action({
            let choice = choice.clone();
            move |_, value: ChoiceId| choice.set(value).unwrap()
        })
        .unwrap();
    let select = Select::new(["one", "two"], 0).controlled(choice.clone(), choice_change.clone());
    choice_change.invoke(&context, ChoiceId::new(1)).unwrap();
    assert_eq!(choice.get().unwrap(), ChoiceId::new(1));
    assert_eq!(
        select
            .node()
            .describe(&mut BuildCx::new())
            .accessibility_metadata()
            .role_name(),
        Some("list")
    );

    let repeated = Node::row([
        Node::text("first").key(BusinessKey::new("first")),
        Node::text("second").key(BusinessKey::new("second")),
    ]);
    let repeated_spec = repeated.describe(&mut BuildCx::new());
    assert_eq!(
        repeated_spec.children()[0]
            .describe(&mut BuildCx::new())
            .key_value()
            .map(BusinessKey::as_str),
        Some("first")
    );
    assert_eq!(
        repeated_spec.children()[1]
            .describe(&mut BuildCx::new())
            .key_value()
            .map(BusinessKey::as_str),
        Some("second")
    );
}

#[test]
fn style_diagnostics_and_platform_host_are_explicit_boundaries() {
    let mut styles = StyleCache::default();
    assert!(styles.apply(StyleInput::text("padding: 8; gap: 4")).applied);
    let previous = styles.last_successful().cloned();
    let rejected = styles.apply(StyleInput::text("opacity: 2"));
    assert!(rejected.is_diagnostic_success());
    assert_eq!(
        rejected
            .diagnostic
            .as_ref()
            .map(|diagnostic| diagnostic.code),
        Some("invalid-value")
    );
    assert_eq!(styles.last_successful().cloned(), previous);

    let window = Window::new("facade fixture").with_title("updated facade fixture");
    assert_eq!(window.title(), "updated facade fixture");
    let _run_loop = RunLoop::new();
}
