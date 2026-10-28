//! Stable App-owned and virtualized data-model contracts.

use crate::app::{AppContext, AppError};

pub use crate::app::{Model, ModelHandle};

/// Number of logical rows and columns exposed by a virtual table.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TableDimensions {
    pub rows: usize,
    pub columns: usize,
}

impl TableDimensions {
    pub const fn new(rows: usize, columns: usize) -> Self {
        Self { rows, columns }
    }
}

/// Half-open logical cell rectangle: `[row_start, row_end) × [column_start, column_end)`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CellRange {
    pub row_start: usize,
    pub row_end: usize,
    pub column_start: usize,
    pub column_end: usize,
}

impl CellRange {
    /// Creates a validated half-open range.
    pub fn new(
        row_start: usize,
        row_end: usize,
        column_start: usize,
        column_end: usize,
    ) -> Result<Self, VirtualModelError> {
        if row_start > row_end || column_start > column_end {
            return Err(VirtualModelError::InvalidRange);
        }
        Ok(Self {
            row_start,
            row_end,
            column_start,
            column_end,
        })
    }

    pub fn is_empty(self) -> bool {
        self.row_start == self.row_end || self.column_start == self.column_end
    }

    pub fn clipped(self, dimensions: TableDimensions) -> Self {
        let row_start = self.row_start.min(dimensions.rows);
        let column_start = self.column_start.min(dimensions.columns);
        Self {
            row_start,
            row_end: self.row_end.min(dimensions.rows).max(row_start),
            column_start,
            column_end: self.column_end.min(dimensions.columns).max(column_start),
        }
    }
}

/// Monotonic request identifier and its bounded logical range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CellRequest {
    pub id: u64,
    pub range: CellRange,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CellCoordinate {
    pub row: usize,
    pub column: usize,
}

/// Language-neutral value supported by the virtual table model.
#[derive(Clone, Debug, PartialEq)]
pub enum CellValue {
    Empty,
    Text(String),
    Bool(bool),
    Number(f64),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SortDescriptor {
    pub column: usize,
    pub direction: SortDirection,
}

impl SortDescriptor {
    /// Creates an ascending sort for a logical column.
    pub const fn ascending(column: usize) -> Self {
        Self {
            column,
            direction: SortDirection::Ascending,
        }
    }

    /// Creates a descending sort for a logical column.
    pub const fn descending(column: usize) -> Self {
        Self {
            column,
            direction: SortDirection::Descending,
        }
    }
}

/// Logical selection represented without materializing rows or cells.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SelectionChange {
    pub ranges: Vec<CellRange>,
}

impl SelectionChange {
    /// Creates a selection containing one logical range.
    pub fn one(range: CellRange) -> Self {
        Self {
            ranges: vec![range],
        }
    }

    /// Creates an empty logical selection.
    pub fn clear() -> Self {
        Self::default()
    }
}

/// Explicit viewport expansion used when requesting virtual cells.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PrefetchBand {
    pub rows_before: usize,
    pub rows_after: usize,
    pub columns_before: usize,
    pub columns_after: usize,
}

/// Defined model failures raised before an Engine widget or callback is touched.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum VirtualModelError {
    #[error("cell range start must not exceed its end")]
    InvalidRange,
    #[error("cell coordinate or logical range is outside model dimensions")]
    OutOfBounds,
    #[error("virtual cell request id overflowed")]
    RequestIdOverflow,
    #[error(transparent)]
    App(#[from] AppError),
}

/// App-owned provider for a range-requested DataTable.
///
/// Every mutating callback is invoked by [`VirtualDataTableAdapter`] through the
/// owning [`AppContext`] on its UI thread.
pub trait VirtualDataTableModel: Model {
    fn dimensions(&self) -> TableDimensions;
    fn request_cells(&mut self, request: CellRequest) -> Result<(), VirtualModelError>;
    fn apply_cell_update(
        &mut self,
        coordinate: CellCoordinate,
        value: CellValue,
    ) -> Result<(), VirtualModelError>;
    /// Typed edit callback. The default preserves the cell-update contract so
    /// existing models remain source-compatible while editors can distinguish
    /// an explicit edit when needed.
    fn edit_cell(
        &mut self,
        coordinate: CellCoordinate,
        value: CellValue,
    ) -> Result<(), VirtualModelError> {
        self.apply_cell_update(coordinate, value)
    }
    fn sort_changed(&mut self, sort: &[SortDescriptor]) -> Result<(), VirtualModelError>;
    fn selection_changed(&mut self, selection: &SelectionChange) -> Result<(), VirtualModelError>;
    /// Receives a UI-thread viewport/scroll notification. Models that need
    /// scroll-specific bookkeeping may override it; data supply remains bounded
    /// by [`VirtualDataTableAdapter::request_visible`].
    fn scroll_changed(&mut self, _viewport: CellRange) -> Result<(), VirtualModelError> {
        Ok(())
    }
}

/// Context-bound adapter that converts a viewport into bounded model requests.
pub struct VirtualDataTableAdapter<M: VirtualDataTableModel> {
    model: ModelHandle<M>,
    prefetch: PrefetchBand,
    next_request_id: u64,
}

impl<M: VirtualDataTableModel> VirtualDataTableAdapter<M> {
    pub fn new(model: ModelHandle<M>) -> Self {
        Self {
            model,
            prefetch: PrefetchBand::default(),
            next_request_id: 1,
        }
    }

    pub fn with_prefetch(mut self, prefetch: PrefetchBand) -> Self {
        self.prefetch = prefetch;
        self
    }

    /// Replaces the explicit viewport expansion without resetting request ids.
    pub fn set_prefetch(&mut self, prefetch: PrefetchBand) {
        self.prefetch = prefetch;
    }

    pub fn model(&self) -> &ModelHandle<M> {
        &self.model
    }

    /// Requests only the clipped viewport plus the configured prefetch band.
    /// Empty viewports or zero-sized models do not produce a model callback.
    pub fn request_visible(
        &mut self,
        context: &AppContext,
        viewport: CellRange,
    ) -> Result<Option<CellRequest>, VirtualModelError> {
        let dimensions = self
            .model
            .with(context, VirtualDataTableModel::dimensions)?;
        let visible = viewport.clipped(dimensions);
        if visible.is_empty() {
            return Ok(None);
        }

        let range = CellRange {
            row_start: visible.row_start.saturating_sub(self.prefetch.rows_before),
            row_end: visible
                .row_end
                .saturating_add(self.prefetch.rows_after)
                .min(dimensions.rows),
            column_start: visible
                .column_start
                .saturating_sub(self.prefetch.columns_before),
            column_end: visible
                .column_end
                .saturating_add(self.prefetch.columns_after)
                .min(dimensions.columns),
        };
        let id = self.next_request_id;
        let next = id
            .checked_add(1)
            .ok_or(VirtualModelError::RequestIdOverflow)?;
        let request = CellRequest { id, range };
        self.model
            .with_mut(context, |model| model.request_cells(request))??;
        self.next_request_id = next;
        Ok(Some(request))
    }

    pub fn apply_cell_update(
        &self,
        context: &AppContext,
        coordinate: CellCoordinate,
        value: CellValue,
    ) -> Result<(), VirtualModelError> {
        let dimensions = self
            .model
            .with(context, VirtualDataTableModel::dimensions)?;
        if coordinate.row >= dimensions.rows || coordinate.column >= dimensions.columns {
            return Err(VirtualModelError::OutOfBounds);
        }
        self.model
            .with_mut(context, |model| model.apply_cell_update(coordinate, value))??;
        Ok(())
    }

    /// Applies an application edit through the typed model callback.
    pub fn edit_cell(
        &self,
        context: &AppContext,
        coordinate: CellCoordinate,
        value: CellValue,
    ) -> Result<(), VirtualModelError> {
        let dimensions = self
            .model
            .with(context, VirtualDataTableModel::dimensions)?;
        if coordinate.row >= dimensions.rows || coordinate.column >= dimensions.columns {
            return Err(VirtualModelError::OutOfBounds);
        }
        self.model
            .with_mut(context, |model| model.edit_cell(coordinate, value))??;
        Ok(())
    }

    /// Reports a UI-thread scroll/viewport change without exposing a widget.
    pub fn notify_scroll(
        &self,
        context: &AppContext,
        viewport: CellRange,
    ) -> Result<(), VirtualModelError> {
        let dimensions = self
            .model
            .with(context, VirtualDataTableModel::dimensions)?;
        let viewport = viewport.clipped(dimensions);
        self.model
            .with_mut(context, |model| model.scroll_changed(viewport))??;
        Ok(())
    }

    pub fn notify_sort(
        &self,
        context: &AppContext,
        sort: &[SortDescriptor],
    ) -> Result<(), VirtualModelError> {
        let dimensions = self
            .model
            .with(context, VirtualDataTableModel::dimensions)?;
        if sort.iter().any(|item| item.column >= dimensions.columns) {
            return Err(VirtualModelError::OutOfBounds);
        }
        self.model
            .with_mut(context, |model| model.sort_changed(sort))??;
        Ok(())
    }

    pub fn notify_selection(
        &self,
        context: &AppContext,
        selection: &SelectionChange,
    ) -> Result<(), VirtualModelError> {
        let dimensions = self
            .model
            .with(context, VirtualDataTableModel::dimensions)?;
        if selection.ranges.iter().any(|range| {
            range.row_start > range.row_end
                || range.column_start > range.column_end
                || range.row_end > dimensions.rows
                || range.column_end > dimensions.columns
        }) {
            return Err(VirtualModelError::OutOfBounds);
        }
        self.model
            .with_mut(context, |model| model.selection_changed(selection))??;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, AppConfig};
    use proptest::prelude::*;

    #[derive(Default)]
    struct RecordingModel {
        requests: Vec<CellRequest>,
        updates: Vec<(CellCoordinate, CellValue)>,
        sorts: Vec<Vec<SortDescriptor>>,
        selections: Vec<SelectionChange>,
    }

    impl Model for RecordingModel {}

    impl VirtualDataTableModel for RecordingModel {
        fn dimensions(&self) -> TableDimensions {
            TableDimensions::new(100_000, 8)
        }
        fn request_cells(&mut self, request: CellRequest) -> Result<(), VirtualModelError> {
            self.requests.push(request);
            Ok(())
        }
        fn apply_cell_update(
            &mut self,
            coordinate: CellCoordinate,
            value: CellValue,
        ) -> Result<(), VirtualModelError> {
            self.updates.push((coordinate, value));
            Ok(())
        }
        fn sort_changed(&mut self, sort: &[SortDescriptor]) -> Result<(), VirtualModelError> {
            self.sorts.push(sort.to_vec());
            Ok(())
        }
        fn selection_changed(
            &mut self,
            selection: &SelectionChange,
        ) -> Result<(), VirtualModelError> {
            self.selections.push(selection.clone());
            Ok(())
        }
    }

    struct BoundedModel {
        dimensions: TableDimensions,
        requests: Vec<CellRequest>,
    }

    impl Model for BoundedModel {}

    impl VirtualDataTableModel for BoundedModel {
        fn dimensions(&self) -> TableDimensions {
            self.dimensions
        }

        fn request_cells(&mut self, request: CellRequest) -> Result<(), VirtualModelError> {
            self.requests.push(request);
            Ok(())
        }

        fn apply_cell_update(
            &mut self,
            _coordinate: CellCoordinate,
            _value: CellValue,
        ) -> Result<(), VirtualModelError> {
            Ok(())
        }

        fn sort_changed(&mut self, _sort: &[SortDescriptor]) -> Result<(), VirtualModelError> {
            Ok(())
        }

        fn selection_changed(
            &mut self,
            _selection: &SelectionChange,
        ) -> Result<(), VirtualModelError> {
            Ok(())
        }
    }

    // Feature: synui-api-abi-reset, Property 11: 虚拟 DataTable 请求范围受视口限制
    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 100,
            max_shrink_iters: 256,
            .. ProptestConfig::default()
        })]
        #[test]
        fn virtual_table_requests_are_clipped_to_viewport_and_prefetch(
            large_table in any::<bool>(),
            small_rows in 1usize..=2_000,
            columns in 1usize..=32,
            row_start in 0usize..=200_000,
            row_height in 0usize..=256,
            column_start in 0usize..=64,
            column_width in 0usize..=16,
            rows_before in 0usize..=512,
            rows_after in 0usize..=512,
            columns_before in 0usize..=16,
            columns_after in 0usize..=16,
        ) {
            let rows = if large_table { 100_000 } else { small_rows };
            let dimensions = TableDimensions::new(rows, columns);
            let app = App::new(AppConfig::default()).unwrap();
            let context = app.context();
            let model = context
                .model(BoundedModel {
                    dimensions,
                    requests: Vec::new(),
                })
                .unwrap();
            let mut adapter = VirtualDataTableAdapter::new(model.clone()).with_prefetch(PrefetchBand {
                rows_before,
                rows_after,
                columns_before,
                columns_after,
            });
            let viewport = CellRange::new(
                row_start,
                row_start.saturating_add(row_height),
                column_start,
                column_start.saturating_add(column_width),
            )
            .unwrap();
            let visible = viewport.clipped(dimensions);
            let request = adapter.request_visible(&context, viewport).unwrap();

            if visible.is_empty() {
                prop_assert!(request.is_none());
                prop_assert!(model.with(&context, |model| model.requests.is_empty()).unwrap());
            } else {
                let request = request.expect("non-empty viewport must request cells");
                let range = request.range;
                prop_assert!(range.row_start <= visible.row_start);
                prop_assert!(range.row_end >= visible.row_end);
                prop_assert!(range.column_start <= visible.column_start);
                prop_assert!(range.column_end >= visible.column_end);
                prop_assert!(range.row_end <= dimensions.rows);
                prop_assert!(range.column_end <= dimensions.columns);
                prop_assert!(range.row_end > range.row_start);
                prop_assert!(range.column_end > range.column_start);

                let max_rows = visible
                    .row_end
                    .saturating_sub(visible.row_start)
                    .saturating_add(rows_before)
                    .saturating_add(rows_after)
                    .min(dimensions.rows);
                let max_columns = visible
                    .column_end
                    .saturating_sub(visible.column_start)
                    .saturating_add(columns_before)
                    .saturating_add(columns_after)
                    .min(dimensions.columns);
                prop_assert!(range.row_end - range.row_start <= max_rows);
                prop_assert!(range.column_end - range.column_start <= max_columns);
                if max_rows < dimensions.rows {
                    prop_assert!(range.row_end - range.row_start < dimensions.rows);
                }
                if max_columns < dimensions.columns {
                    prop_assert!(range.column_end - range.column_start < dimensions.columns);
                }
                prop_assert_eq!(request.id, 1);
                prop_assert_eq!(model.with(&context, |model| model.requests.clone()).unwrap(), vec![request]);
            }
        }
    }

    #[test]
    fn hundred_thousand_rows_request_only_viewport_and_prefetch() {
        let app = App::new(AppConfig::default()).unwrap();
        let context = app.context();
        let model = context.model(RecordingModel::default()).unwrap();
        let mut adapter = VirtualDataTableAdapter::new(model.clone()).with_prefetch(PrefetchBand {
            rows_before: 5,
            rows_after: 7,
            columns_before: 1,
            columns_after: 1,
        });
        let viewport = CellRange::new(50_000, 50_020, 2, 5).unwrap();

        let request = adapter
            .request_visible(&context, viewport)
            .unwrap()
            .unwrap();
        assert_eq!(request.id, 1);
        assert_eq!(request.range, CellRange::new(49_995, 50_027, 1, 6).unwrap());
        assert_ne!(request.range.row_end - request.range.row_start, 100_000);
        model
            .with(&context, |model| assert_eq!(model.requests, vec![request]))
            .unwrap();
    }

    #[test]
    fn callbacks_are_bounded_and_ui_thread_gated() {
        let app = App::new(AppConfig::default()).unwrap();
        let context = app.context();
        let model = context.model(RecordingModel::default()).unwrap();
        let adapter = VirtualDataTableAdapter::new(model.clone());

        assert_eq!(
            adapter.apply_cell_update(
                &context,
                CellCoordinate {
                    row: 100_000,
                    column: 0
                },
                CellValue::Empty,
            ),
            Err(VirtualModelError::OutOfBounds)
        );
        let worker_context = context.clone();
        let worker_model = model.clone();
        let result = std::thread::spawn(move || {
            VirtualDataTableAdapter::new(worker_model).apply_cell_update(
                &worker_context,
                CellCoordinate { row: 0, column: 0 },
                CellValue::Text("worker".into()),
            )
        })
        .join()
        .unwrap();
        assert_eq!(
            result,
            Err(VirtualModelError::App(AppError::ThreadAffinity))
        );
        model
            .with(&context, |model| assert!(model.updates.is_empty()))
            .unwrap();
    }
}
