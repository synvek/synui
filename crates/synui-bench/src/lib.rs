//! Versioned benchmark scenarios and regression-gate policy for the v0.2 facade.

/// Comparable CI runner class used by the initial v1 baselines.
pub const CI_MACHINE_CLASS: &str = "github-ubuntu-x64-rust-1.88-release";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MetricUnit {
    NanosecondsPerOperation,
    Bytes,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BenchmarkScenario {
    pub id: &'static str,
    pub workload: &'static str,
    pub deterministic_input: &'static str,
    pub metric: &'static str,
    pub unit: MetricUnit,
    pub baseline: f64,
    pub machine_class: &'static str,
    pub regression_threshold_percent: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BenchmarkManifest {
    pub version: u32,
    pub scenarios: &'static [BenchmarkScenario],
    pub retained_engine_baselines: &'static [&'static str],
}

const SCENARIOS: &[BenchmarkScenario] = &[
    BenchmarkScenario {
        id: "facade_app_startup",
        workload: "construct and drop an App Context",
        deterministic_input: "1,000 sequential App::new/App::drop operations",
        metric: "mean startup latency",
        unit: MetricUnit::NanosecondsPerOperation,
        baseline: 1_000_000.0,
        machine_class: CI_MACHINE_CLASS,
        regression_threshold_percent: 50.0,
    },
    BenchmarkScenario {
        id: "facade_idle_resources",
        workload: "idle App-owned resource footprint",
        deterministic_input: "one App with 32 u64 State values and 8 no-op Action<()> values",
        metric: "live allocator bytes above pre-App baseline",
        unit: MetricUnit::Bytes,
        baseline: 262_144.0,
        machine_class: CI_MACHINE_CLASS,
        regression_threshold_percent: 50.0,
    },
    BenchmarkScenario {
        id: "facade_state_update",
        workload: "typed App-owned State read/write",
        deterministic_input: "100,000 set/get pairs over one State<u64>",
        metric: "mean set/get latency",
        unit: MetricUnit::NanosecondsPerOperation,
        baseline: 20_000.0,
        machine_class: CI_MACHINE_CLASS,
        regression_threshold_percent: 50.0,
    },
    BenchmarkScenario {
        id: "facade_virtual_table_scroll",
        workload: "bounded 100,000-row VirtualDataTable viewport requests",
        deterministic_input: "10,000 40x6 viewports with 8-row/1-column prefetch",
        metric: "mean request latency",
        unit: MetricUnit::NanosecondsPerOperation,
        baseline: 100_000.0,
        machine_class: CI_MACHINE_CLASS,
        regression_threshold_percent: 50.0,
    },
    BenchmarkScenario {
        id: "counter_binary_size",
        workload: "release facade-only Counter executable",
        deterministic_input: "cargo build --release -p counter with the locked dependency graph",
        metric: "executable file size",
        unit: MetricUnit::Bytes,
        baseline: 50_000_000.0,
        machine_class: CI_MACHINE_CLASS,
        regression_threshold_percent: 25.0,
    },
];

pub const BENCHMARK_MANIFEST_V1: BenchmarkManifest = BenchmarkManifest {
    version: 1,
    scenarios: SCENARIOS,
    retained_engine_baselines: &[
        "layout_paint",
        // declarative_diff removed with synui legacy feature (Phase 51 Wave J)
    ],
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Measurement {
    pub scenario_id: &'static str,
    pub value: f64,
    pub unit: MetricUnit,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GateResult {
    pub scenario: &'static BenchmarkScenario,
    pub measured: f64,
    pub limit: f64,
    pub passed: bool,
}

/// Evaluates one result only when its runner class is comparable to the baseline.
pub fn evaluate(
    machine_class: &str,
    measurement: Measurement,
) -> Result<Option<GateResult>, &'static str> {
    let scenario = BENCHMARK_MANIFEST_V1
        .scenarios
        .iter()
        .find(|scenario| scenario.id == measurement.scenario_id)
        .ok_or("measurement does not name a v1 benchmark scenario")?;
    if scenario.unit != measurement.unit {
        return Err("measurement unit does not match the scenario manifest");
    }
    if scenario.machine_class != machine_class {
        return Ok(None);
    }
    let limit = scenario.baseline * (1.0 + scenario.regression_threshold_percent / 100.0);
    Ok(Some(GateResult {
        scenario,
        measured: measurement.value,
        limit,
        passed: measurement.value <= limit,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn v1_manifest_has_five_complete_unique_scenarios() {
        assert_eq!(BENCHMARK_MANIFEST_V1.version, 1);
        assert_eq!(BENCHMARK_MANIFEST_V1.scenarios.len(), 5);
        assert!(!BENCHMARK_MANIFEST_V1.retained_engine_baselines.is_empty());
        let ids = BENCHMARK_MANIFEST_V1
            .scenarios
            .iter()
            .map(|scenario| scenario.id)
            .collect::<HashSet<_>>();
        assert_eq!(ids.len(), 5);
        for scenario in BENCHMARK_MANIFEST_V1.scenarios {
            assert!(!scenario.workload.is_empty());
            assert!(!scenario.deterministic_input.is_empty());
            assert!(!scenario.metric.is_empty());
            assert!(scenario.baseline.is_finite() && scenario.baseline > 0.0);
            assert!(scenario.regression_threshold_percent.is_finite());
            assert!(scenario.regression_threshold_percent > 0.0);
            assert_eq!(scenario.machine_class, CI_MACHINE_CLASS);
        }
    }

    #[test]
    fn each_scenario_gate_isolated_at_limit_and_over_threshold() {
        for scenario in BENCHMARK_MANIFEST_V1.scenarios {
            let limit = scenario.baseline * (1.0 + scenario.regression_threshold_percent / 100.0);
            let at_limit = evaluate(
                CI_MACHINE_CLASS,
                Measurement {
                    scenario_id: scenario.id,
                    value: limit,
                    unit: scenario.unit,
                },
            )
            .unwrap()
            .unwrap();
            assert!(at_limit.passed, "{} should pass at its limit", scenario.id);
            assert_eq!(at_limit.scenario.id, scenario.id);
            assert_eq!(at_limit.limit, limit);

            let over_limit = evaluate(
                CI_MACHINE_CLASS,
                Measurement {
                    scenario_id: scenario.id,
                    value: limit + 1.0,
                    unit: scenario.unit,
                },
            )
            .unwrap()
            .unwrap();
            assert!(
                !over_limit.passed,
                "{} should fail above its limit",
                scenario.id
            );
            assert_eq!(over_limit.scenario.id, scenario.id);
        }
    }

    #[test]
    fn incomparable_machine_and_invalid_measurements_do_not_run_a_gate() {
        let scenario = BENCHMARK_MANIFEST_V1.scenarios[0];
        assert!(evaluate(
            "different-machine-class",
            Measurement {
                scenario_id: scenario.id,
                value: f64::MAX,
                unit: scenario.unit,
            },
        )
        .unwrap()
        .is_none());
        assert_eq!(
            evaluate(
                CI_MACHINE_CLASS,
                Measurement {
                    scenario_id: scenario.id,
                    value: 1.0,
                    unit: MetricUnit::Bytes,
                },
            ),
            Err("measurement unit does not match the scenario manifest")
        );
        assert_eq!(
            evaluate(
                CI_MACHINE_CLASS,
                Measurement {
                    scenario_id: "missing",
                    value: 1.0,
                    unit: MetricUnit::Bytes,
                },
            ),
            Err("measurement does not name a v1 benchmark scenario")
        );
    }
}
