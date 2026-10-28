//! Property tests for App-owned Context resource isolation.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use proptest::prelude::*;
use synui::prelude::*;

struct DropProbe(Arc<AtomicUsize>);

impl Drop for DropProbe {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

struct StateValue {
    value: u64,
    _probe: DropProbe,
}

struct TestModel {
    value: u64,
    _probe: DropProbe,
}

impl Model for TestModel {}

struct Fixture {
    _app: App,
    context: AppContext,
    states: Vec<State<Arc<StateValue>>>,
    actions: Vec<Action<u64>>,
    models: Vec<ModelHandle<TestModel>>,
    calls: Arc<AtomicUsize>,
    drops: Arc<AtomicUsize>,
    probes_created: usize,
    active: bool,
}

fn fresh_model(value: u64) -> TestModel {
    TestModel {
        value,
        _probe: DropProbe(Arc::new(AtomicUsize::new(0))),
    }
}

fn build_fixture(counts: (usize, usize, usize)) -> Fixture {
    let app = App::new(AppConfig::default()).expect("App creation must succeed");
    let context = app.context();
    let drops = Arc::new(AtomicUsize::new(0));
    let calls = Arc::new(AtomicUsize::new(0));
    let mut probes_created = 0;

    let mut states = Vec::with_capacity(counts.0);
    for slot in 0..counts.0 {
        probes_created += 1;
        states.push(
            context
                .state(Arc::new(StateValue {
                    value: slot as u64,
                    _probe: DropProbe(drops.clone()),
                }))
                .expect("active Context must accept State"),
        );
    }

    let mut actions = Vec::with_capacity(counts.1);
    for _ in 0..counts.1 {
        probes_created += 1;
        let observed = calls.clone();
        let probe = DropProbe(drops.clone());
        actions.push(
            context
                .action(move |_, _: u64| {
                    let _keep_alive = &probe;
                    observed.fetch_add(1, Ordering::SeqCst);
                })
                .expect("active Context must accept Action"),
        );
    }

    let mut models = Vec::with_capacity(counts.2);
    for slot in 0..counts.2 {
        probes_created += 1;
        models.push(
            context
                .model(TestModel {
                    value: slot as u64,
                    _probe: DropProbe(drops.clone()),
                })
                .expect("active Context must accept Model"),
        );
    }

    Fixture {
        _app: app,
        context,
        states,
        actions,
        models,
        calls,
        drops,
        probes_created,
        active: true,
    }
}

fn assert_destroyed(fixture: &Fixture) {
    assert_eq!(fixture.context.lifecycle(), LifecycleState::Destroyed);
    for state in &fixture.states {
        assert!(matches!(
            state.get(),
            Err(AppError::ContextDestroyed | AppError::StaleHandle)
        ));
    }
    for action in &fixture.actions {
        assert!(matches!(
            action.invoke(&fixture.context, 0),
            Err(AppError::ContextDestroyed | AppError::StaleHandle)
        ));
    }
    for model in &fixture.models {
        assert!(matches!(
            model.with(&fixture.context, |_| ()),
            Err(AppError::ContextDestroyed | AppError::StaleHandle)
        ));
    }
    assert!(fixture.drops.load(Ordering::SeqCst) >= fixture.probes_created);
}

// Feature: synui-api-abi-reset, Property 1: Context 资源所有权和生命周期隔离
proptest! {
    #![proptest_config(ProptestConfig {
        cases: 100,
        max_shrink_iters: 256,
        .. ProptestConfig::default()
    })]
    #[test]
    fn context_resources_remain_isolated_through_random_use_and_destroy(
        counts in prop::collection::vec(
            (1usize..=3, 1usize..=3, 1usize..=3),
            1..=4,
        ),
        operations in prop::collection::vec(
            (0u8..8, 0usize..8, 0usize..8, 0usize..8, any::<u64>()),
            1..=64,
        ),
    ) {
        let mut fixtures: Vec<_> = counts.into_iter().map(build_fixture).collect();
        let context_count = fixtures.len();

        for (opcode, source_raw, target_raw, resource_raw, value) in operations {
            let source = source_raw % context_count;
            let candidate = target_raw % context_count;
            let target = if candidate == source {
                (source + 1) % context_count
            } else {
                candidate
            };
            let source_active = fixtures[source].active;
            let resource = resource_raw;

            match opcode {
                0 if source_active && !fixtures[source].states.is_empty() => {
                    let index = resource % fixtures[source].states.len();
                    let state = fixtures[source].states[index].clone();
                    fixtures[source].probes_created += 1;
                    state
                        .set(Arc::new(StateValue {
                            value,
                            _probe: DropProbe(fixtures[source].drops.clone()),
                        }))
                        .expect("owner State operation must succeed");
                    assert_eq!(state.get().unwrap().value, value);
                }
                1 if source_active && !fixtures[source].actions.is_empty() => {
                    let index = resource % fixtures[source].actions.len();
                    let action = fixtures[source].actions[index].clone();
                    let context = fixtures[source].context.clone();
                    let before = fixtures[source].calls.load(Ordering::SeqCst);
                    action
                        .invoke(&context, value)
                        .expect("owner Action operation must succeed");
                    assert_eq!(fixtures[source].calls.load(Ordering::SeqCst), before + 1);
                }
                2 if source_active && !fixtures[source].models.is_empty() => {
                    let index = resource % fixtures[source].models.len();
                    let model = fixtures[source].models[index].clone();
                    let context = fixtures[source].context.clone();
                    model
                        .with_mut(&context, |stored| stored.value = value)
                        .expect("owner Model operation must succeed");
                    assert_eq!(model.with(&context, |stored| stored.value).unwrap(), value);
                }
                3 if context_count > 1 && !fixtures[source].states.is_empty() => {
                    let index = resource % fixtures[source].states.len();
                    let state = fixtures[source].states[index].clone();
                    let other_context = fixtures[target].context.clone();
                    assert!(matches!(
                        state.get_in(&other_context),
                        Err(AppError::ContextMismatch)
                    ));
                }
                4 if context_count > 1 && !fixtures[source].actions.is_empty() => {
                    let index = resource % fixtures[source].actions.len();
                    let action = fixtures[source].actions[index].clone();
                    let other_context = fixtures[target].context.clone();
                    let before = fixtures[source].calls.load(Ordering::SeqCst);
                    assert!(matches!(
                        action.invoke(&other_context, value),
                        Err(AppError::ContextMismatch)
                    ));
                    assert_eq!(fixtures[source].calls.load(Ordering::SeqCst), before);
                }
                5 if context_count > 1 && !fixtures[source].models.is_empty() => {
                    let index = resource % fixtures[source].models.len();
                    let model = fixtures[source].models[index].clone();
                    let other_context = fixtures[target].context.clone();
                    assert!(matches!(
                        model.with_mut(&other_context, |_| ()),
                        Err(AppError::ContextMismatch)
                    ));
                }
                6 => {
                    fixtures[source].context.destroy();
                    fixtures[source].active = false;
                    assert_destroyed(&fixtures[source]);
                    assert!(matches!(
                        fixtures[source].context.state(0_u64),
                        Err(AppError::ContextDestroyed)
                    ));
                    assert!(matches!(
                        fixtures[source].context.action(|_, _: ()| {}),
                        Err(AppError::ContextDestroyed)
                    ));
                    assert!(matches!(
                        fixtures[source].context.model(fresh_model(0)),
                        Err(AppError::ContextDestroyed)
                    ));
                }
                7 if !source_active => {
                    assert_destroyed(&fixtures[source]);
                }
                _ => {}
            }

            for (index, fixture) in fixtures.iter().enumerate() {
                if index != source && fixture.active {
                    if let Some(state) = fixture.states.first() {
                        assert!(state.get().is_ok());
                    }
                    if let Some(model) = fixture.models.first() {
                        assert!(model.with(&fixture.context, |_| ()).is_ok());
                    }
                }
            }
        }

        for fixture in &mut fixtures {
            if fixture.active {
                fixture.context.destroy();
                fixture.active = false;
            }
            assert_destroyed(fixture);
        }
    }
}
