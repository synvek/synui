use criterion::{black_box, criterion_group, criterion_main, Criterion};
use synui::prelude::*;

#[derive(Default)]
struct BenchTable {
    requests: u64,
}

impl Model for BenchTable {}

impl VirtualDataTableModel for BenchTable {
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

fn facade_api(c: &mut Criterion) {
    c.bench_function("facade_app_startup", |b| {
        b.iter(|| black_box(App::new(AppConfig::default()).unwrap()))
    });

    c.bench_function("facade_idle_resources_32_state_8_action", |b| {
        b.iter(|| {
            let app = App::new(AppConfig::default()).unwrap();
            let context = app.context();
            let states: Vec<_> = (0..32).map(|value| context.state(value).unwrap()).collect();
            let actions: Vec<_> = (0..8)
                .map(|_| context.action(|_, (): ()| {}).unwrap())
                .collect();
            black_box((app, states, actions));
        })
    });

    c.bench_function("facade_state_update", |b| {
        let app = App::new(AppConfig::default()).unwrap();
        let state = app.context().state(0_u64).unwrap();
        b.iter(|| {
            let next = state.get().unwrap().wrapping_add(1);
            state.set(next).unwrap();
            black_box(state.get().unwrap())
        })
    });

    c.bench_function("facade_virtual_table_scroll_100k", |b| {
        let app = App::new(AppConfig::default()).unwrap();
        let context = app.context();
        let model = context.model(BenchTable::default()).unwrap();
        let mut adapter = VirtualDataTableAdapter::new(model).with_prefetch(PrefetchBand {
            rows_before: 8,
            rows_after: 8,
            columns_before: 1,
            columns_after: 1,
        });
        let mut row = 0_usize;
        b.iter(|| {
            row = (row + 37) % 99_960;
            let viewport = CellRange::new(row, row + 40, 1, 7).unwrap();
            black_box(adapter.request_visible(&context, viewport).unwrap())
        })
    });
}

criterion_group!(benches, facade_api);
criterion_main!(benches);
