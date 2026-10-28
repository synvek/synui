use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::path::PathBuf;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::time::Instant;
use synui::prelude::*;
use synui_bench::{evaluate, Measurement, MetricUnit};

struct TrackingAllocator;
static LIVE_BYTES: AtomicIsize = AtomicIsize::new(0);

unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            LIVE_BYTES.fetch_add(layout.size() as isize, Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            LIVE_BYTES.fetch_add(layout.size() as isize, Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
        LIVE_BYTES.fetch_sub(layout.size() as isize, Ordering::Relaxed);
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let replacement = unsafe { System.realloc(pointer, layout, new_size) };
        if !replacement.is_null() {
            LIVE_BYTES.fetch_add(
                new_size as isize - layout.size() as isize,
                Ordering::Relaxed,
            );
        }
        replacement
    }
}

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

#[derive(Default)]
struct GateTable {
    requests: u64,
}

impl Model for GateTable {}
impl VirtualDataTableModel for GateTable {
    fn dimensions(&self) -> TableDimensions {
        TableDimensions::new(100_000, 8)
    }

    fn request_cells(&mut self, request: CellRequest) -> Result<(), VirtualModelError> {
        self.requests = self.requests.wrapping_add(request.id);
        Ok(())
    }

    fn apply_cell_update(
        &mut self,
        _: CellCoordinate,
        _: CellValue,
    ) -> Result<(), VirtualModelError> {
        Ok(())
    }

    fn sort_changed(&mut self, _: &[SortDescriptor]) -> Result<(), VirtualModelError> {
        Ok(())
    }

    fn selection_changed(&mut self, _: &SelectionChange) -> Result<(), VirtualModelError> {
        Ok(())
    }
}

fn mean_nanos(iterations: u64, mut operation: impl FnMut()) -> f64 {
    let started = Instant::now();
    for _ in 0..iterations {
        operation();
    }
    started.elapsed().as_nanos() as f64 / iterations as f64
}

fn measure_startup() -> f64 {
    mean_nanos(1_000, || {
        black_box(App::new(AppConfig::default()).unwrap());
    })
}

fn measure_idle_resources() -> f64 {
    let before = LIVE_BYTES.load(Ordering::Relaxed);
    let app = App::new(AppConfig::default()).unwrap();
    let context = app.context();
    let states: Vec<_> = (0..32).map(|value| context.state(value).unwrap()).collect();
    let actions: Vec<_> = (0..8)
        .map(|_| context.action(|_, (): ()| {}).unwrap())
        .collect();
    black_box((&app, &states, &actions));
    let retained = LIVE_BYTES.load(Ordering::Relaxed).saturating_sub(before);
    drop(actions);
    drop(states);
    drop(app);
    retained.max(0) as f64
}

fn measure_state_update() -> f64 {
    let app = App::new(AppConfig::default()).unwrap();
    let state = app.context().state(0_u64).unwrap();
    mean_nanos(100_000, || {
        let next = state.get().unwrap().wrapping_add(1);
        state.set(next).unwrap();
        black_box(state.get().unwrap());
    })
}
fn measure_virtual_table_scroll() -> f64 {
    let app = App::new(AppConfig::default()).unwrap();
    let context = app.context();
    let model = context.model(GateTable::default()).unwrap();
    let mut adapter = VirtualDataTableAdapter::new(model).with_prefetch(PrefetchBand {
        rows_before: 8,
        rows_after: 8,
        columns_before: 1,
        columns_after: 1,
    });
    let mut row = 0_usize;
    mean_nanos(10_000, || {
        row = (row + 37) % 99_960;
        let viewport = CellRange::new(row, row + 40, 1, 7).unwrap();
        black_box(adapter.request_visible(&context, viewport).unwrap());
    })
}

fn arguments() -> Result<(String, PathBuf), String> {
    let mut machine_class = None;
    let mut binary = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--machine-class" => machine_class = args.next(),
            "--binary" => binary = args.next().map(PathBuf::from),
            _ => return Err(format!("unknown argument: {argument}")),
        }
    }
    Ok((
        machine_class.ok_or("missing --machine-class <class>")?,
        binary.ok_or("missing --binary <path>")?,
    ))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (machine_class, binary) = arguments().map_err(|message| {
        format!("{message}; usage: bench_gate --machine-class <class> --binary <path>")
    })?;
    let binary_size = std::fs::metadata(&binary)?.len() as f64;
    let measurements = [
        Measurement {
            scenario_id: "facade_app_startup",
            value: measure_startup(),
            unit: MetricUnit::NanosecondsPerOperation,
        },
        Measurement {
            scenario_id: "facade_idle_resources",
            value: measure_idle_resources(),
            unit: MetricUnit::Bytes,
        },
        Measurement {
            scenario_id: "facade_state_update",
            value: measure_state_update(),
            unit: MetricUnit::NanosecondsPerOperation,
        },
        Measurement {
            scenario_id: "facade_virtual_table_scroll",
            value: measure_virtual_table_scroll(),
            unit: MetricUnit::NanosecondsPerOperation,
        },
        Measurement {
            scenario_id: "counter_binary_size",
            value: binary_size,
            unit: MetricUnit::Bytes,
        },
    ];

    let mut failed = false;
    for measurement in measurements {
        match evaluate(&machine_class, measurement)? {
            Some(result) => {
                println!(
                    "{} measured={:.2} {:?} baseline={:.2} limit={:.2} status={}",
                    result.scenario.id,
                    result.measured,
                    result.scenario.unit,
                    result.scenario.baseline,
                    result.limit,
                    if result.passed { "pass" } else { "FAIL" }
                );
                failed |= !result.passed;
            }
            None => println!(
                "{} measured={:.2} {:?} status=skipped machine-class={}",
                measurement.scenario_id, measurement.value, measurement.unit, machine_class
            ),
        }
    }
    if failed {
        return Err("one or more comparable benchmark scenarios regressed".into());
    }
    Ok(())
}
