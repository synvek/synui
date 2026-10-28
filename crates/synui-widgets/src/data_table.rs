//! Virtualized data table — columns, sorting, cell edit, horizontal clip (Phase 17).

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet, VecDeque};

use synui_core::alloc_widget_id;
use synui_core::{
    A11yAction, A11yProps, A11yRole, Constraints, Dimension, ErasedWidget, Event, EventCtx,
    EventResult, Key, LayoutStyle, MeasureCtx, NamedKey, PaintCtx, Point, PointerButton, Rect,
    Size, TextAlign, TextMeasurer, Widget, WidgetId, WidgetStore,
};
use synui_render::paint_simple_rect;
use vello::Scene;

use crate::basic::Label;
use crate::scroll::{paint_v_scrollbar, VScrollbarState};
use crate::virtual_mount::{scroll_at_edge, TABLE_POOL_CAP};
use crate::virtual_viewport::{layout_row_subtree, VirtualViewport};

const HEADER_HEIGHT: f64 = 28.0;
const ROW_HEIGHT: f64 = 28.0;
const CELL_PAD_X: f64 = 8.0;
const DOUBLE_CLICK_MS: f64 = 400.0;
const RESIZE_HIT: f64 = 4.0;
const REORDER_THRESHOLD: f64 = 5.0;
const MIN_COL_WIDTH: f64 = 40.0;

/// Column metadata for [`DataTable`].
#[derive(Clone, Debug)]
pub struct ColumnDef {
    pub id: String,
    pub title: String,
    pub width: f64,
    pub sortable: bool,
}

impl ColumnDef {
    pub fn new(id: impl Into<String>, title: impl Into<String>, width: f64) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            width: width.max(MIN_COL_WIDTH),
            sortable: true,
        }
    }
}

/// Snapshot of visual column order and widths (Phase 21).
#[derive(Clone, Debug, PartialEq)]
pub struct ColumnLayout {
    pub columns: Vec<(String, f64)>,
}

#[derive(Clone, Debug)]
enum HeaderDrag {
    /// Pressed header; may become sort (click) or reorder (drag).
    Pending {
        col: usize,
        start_x: f64,
    },
    Reorder {
        from: usize,
        insert_at: usize,
    },
    Resize {
        col: usize,
        start_x: f64,
        start_width: f64,
    },
}

/// Merged cell region in **data** coordinates (Phase 23).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellSpan {
    pub data_row: usize,
    pub data_col: usize,
    pub row_span: usize,
    pub col_span: usize,
}

impl CellSpan {
    pub fn new(data_row: usize, data_col: usize, row_span: usize, col_span: usize) -> Self {
        Self {
            data_row,
            data_col,
            row_span: row_span.max(1),
            col_span: col_span.max(1),
        }
    }

    pub fn contains(&self, data_row: usize, data_col: usize) -> bool {
        data_row >= self.data_row
            && data_row < self.data_row + self.row_span
            && data_col >= self.data_col
            && data_col < self.data_col + self.col_span
    }

    pub fn is_anchor(&self, data_row: usize, data_col: usize) -> bool {
        data_row == self.data_row && data_col == self.data_col
    }
}

/// Virtualized table with sticky header, sort, and optional cell widgets.
pub struct DataTable {
    id: WidgetId,
    bounds: Rect,
    child_ids: Vec<WidgetId>,
    mounted: Vec<(usize, usize, WidgetId)>,
    /// Off-viewport cells keyed by (data_row, col) (Phase 18).
    pool: HashMap<(usize, usize), WidgetId>,
    pool_order: VecDeque<(usize, usize)>,
    pub columns: Vec<ColumnDef>,
    /// Visual column → data column index for `cell_fn` (Phase 21 reorder).
    data_col_of: Vec<usize>,
    /// Total data rows (before filter).
    pub data_row_count: usize,
    /// Visible row count after filter (= `row_order.len()`).
    pub row_count: usize,
    cell_fn: Box<dyn Fn(usize, usize) -> String>,
    cell_builder: Option<Box<dyn Fn(usize, usize) -> Box<dyn ErasedWidget>>>,
    cell_builder_store: Option<Box<dyn Fn(&mut WidgetStore, usize, usize) -> WidgetId>>,
    use_mount: bool,
    vp: VirtualViewport,
    pub scroll_x: f64,
    /// Multi-select visual rows (Phase 19).
    pub selected_rows: std::collections::HashSet<usize>,
    pub selection_anchor: Option<usize>,
    /// Leading columns that do not scroll horizontally.
    pub freeze_leading: usize,
    pub focused_col: usize,
    pub sort_column: Option<usize>,
    pub sort_asc: bool,
    /// Full sorted data-row order (pre-filter).
    full_order: Vec<usize>,
    /// Visible data-row indices (post-filter).
    row_order: Vec<usize>,
    /// Global case-insensitive substring filter (Phase 22).
    pub filter_query: String,
    /// Per-column filters (data column index); empty = no filter.
    pub column_filters: Vec<String>,
    /// Optional external row predicate (Phase 26); ANDed with substring filters.
    filter_predicate: Option<Box<dyn Fn(&[String]) -> bool>>,
    /// Shared declarative row matrix (Phase 26).
    rows_arc: Option<std::sync::Arc<[Vec<String>]>>,
    /// Merged cells in data coordinates (Phase 23).
    spans: Vec<CellSpan>,
    /// `(data_row, data_col)` → index into `spans` (covers + anchors).
    span_cover: HashMap<(usize, usize), usize>,
    sort_with: Option<Box<dyn Fn(usize, usize, bool) -> Ordering>>,
    editing: Option<(usize, usize)>,
    edit_buffer: String,
    on_cell_edit: Option<Box<dyn FnMut(usize, usize, String)>>,
    pending_edit: Option<(usize, usize, String)>,
    focused: bool,
    scrollbar: VScrollbarState,
    last_click: Option<(f64, usize, usize)>,
    last_click_time_ms: f64,
    header_drag: Option<HeaderDrag>,
    /// Set when column order/widths change (for bindings).
    layout_dirty: bool,
}

impl DataTable {
    pub fn new(columns: Vec<ColumnDef>, row_count: usize) -> Self {
        let n = columns.len();
        let full_order: Vec<usize> = (0..row_count).collect();
        let this = Self {
            id: alloc_widget_id(),
            bounds: Rect::ZERO,
            child_ids: Vec::new(),
            mounted: Vec::new(),
            pool: HashMap::new(),
            pool_order: VecDeque::new(),
            columns,
            data_col_of: (0..n).collect(),
            data_row_count: row_count,
            row_count,
            cell_fn: Box::new(|row, col| format!("{row},{col}")),
            cell_builder: None,
            cell_builder_store: None,
            use_mount: false,
            vp: VirtualViewport::new(row_count, ROW_HEIGHT),
            scroll_x: 0.0,
            selected_rows: std::collections::HashSet::new(),
            selection_anchor: None,
            freeze_leading: 0,
            focused_col: 0,
            sort_column: None,
            sort_asc: true,
            full_order: full_order.clone(),
            row_order: full_order,
            filter_query: String::new(),
            column_filters: vec![String::new(); n],
            filter_predicate: None,
            rows_arc: None,
            spans: Vec::new(),
            span_cover: HashMap::new(),
            sort_with: None,
            editing: None,
            edit_buffer: String::new(),
            on_cell_edit: None,
            pending_edit: None,
            focused: false,
            scrollbar: VScrollbarState::default(),
            last_click: None,
            last_click_time_ms: 0.0,
            header_drag: None,
            layout_dirty: false,
        };
        crate::virtual_mount::register_virt_host(
            this.id,
            crate::virtual_mount::VirtHostKind::Table,
        );
        this
    }

    pub fn with_cell_fn(mut self, f: impl Fn(usize, usize) -> String + 'static) -> Self {
        self.cell_fn = Box::new(f);
        self
    }

    pub fn set_cell_fn(&mut self, f: impl Fn(usize, usize) -> String + 'static) {
        self.cell_fn = Box::new(f);
    }

    pub fn set_columns_and_rows(&mut self, columns: Vec<ColumnDef>, row_count: usize) {
        let n = columns.len();
        self.columns = columns;
        self.data_col_of = (0..n).collect();
        while self.column_filters.len() < n {
            self.column_filters.push(String::new());
        }
        self.column_filters.truncate(n);
        self.data_row_count = row_count;
        self.full_order = (0..row_count).collect();
        self.row_order = self.full_order.clone();
        self.row_count = row_count;
        self.vp.item_count = row_count;
        self.editing = None;
        self.vp.clear_variable_heights();
        self.vp.item_extent = ROW_HEIGHT;
        self.sort_column = None;
        self.reapply_filter();
        self.clamp_scroll();
    }

    /// Update row cell provider and row count without resetting column layout (Phase 21).
    pub fn set_rows_preserving_columns(
        &mut self,
        row_count: usize,
        f: impl Fn(usize, usize) -> String + 'static,
    ) {
        self.cell_fn = Box::new(f);
        if self.data_row_count != row_count || self.full_order.len() != row_count {
            self.data_row_count = row_count;
            self.full_order = (0..row_count).collect();
            self.editing = None;
            self.sort_column = None;
        }
        self.reapply_filter();
        self.clamp_scroll();
    }

    /// Compare declarative row matrix against current `cell_fn` (Phase 26).
    pub fn rows_match(&self, rows: &[Vec<String>]) -> bool {
        if rows.len() != self.data_row_count {
            return false;
        }
        let ncols = self.columns.len();
        for (r, row) in rows.iter().enumerate() {
            if row.len() != ncols {
                return false;
            }
            for (c, cell) in row.iter().enumerate() {
                if &(self.cell_fn)(r, c) != cell {
                    return false;
                }
            }
        }
        true
    }

    pub fn rows_arc_eq(&self, rows: &std::sync::Arc<[Vec<String>]>) -> bool {
        match &self.rows_arc {
            Some(existing) => std::sync::Arc::ptr_eq(existing, rows),
            None => false,
        }
    }

    pub fn set_rows_arc(&mut self, rows: std::sync::Arc<[Vec<String>]>) {
        self.rows_arc = Some(rows);
    }

    /// Replace cell provider from shared Arc rows (no clone of matrix).
    pub fn apply_rows_arc(&mut self, rows: std::sync::Arc<[Vec<String>]>) {
        if self.rows_arc_eq(&rows) {
            return;
        }
        let row_count = rows.len();
        let cells = rows.clone();
        self.set_rows_preserving_columns(row_count, move |r, c| {
            cells
                .get(r)
                .and_then(|row| row.get(c).cloned())
                .unwrap_or_default()
        });
        self.rows_arc = Some(rows);
    }

    /// External filter predicate over a data row's cells (Phase 26).
    pub fn set_filter_predicate(&mut self, pred: Option<Box<dyn Fn(&[String]) -> bool>>) {
        self.filter_predicate = pred;
        self.reapply_filter();
    }

    /// Merge incoming column defs by id: keep current order/widths; refresh titles.
    /// Returns false if id sets differ (caller should full-replace).
    pub fn patch_column_titles(&mut self, incoming: &[ColumnDef]) -> bool {
        if incoming.len() != self.columns.len() {
            return false;
        }
        let mut by_id: HashMap<&str, &ColumnDef> = HashMap::new();
        for c in incoming {
            by_id.insert(c.id.as_str(), c);
        }
        for c in &self.columns {
            if !by_id.contains_key(c.id.as_str()) {
                return false;
            }
        }
        for c in &mut self.columns {
            if let Some(src) = by_id.get(c.id.as_str()) {
                c.title = src.title.clone();
                c.sortable = src.sortable;
            }
        }
        true
    }

    pub fn column_layout(&self) -> ColumnLayout {
        ColumnLayout {
            columns: self
                .columns
                .iter()
                .map(|c| (c.id.clone(), c.width))
                .collect(),
        }
    }

    pub fn take_layout_dirty(&mut self) -> bool {
        let d = self.layout_dirty;
        self.layout_dirty = false;
        d
    }

    pub fn set_filter_query(&mut self, query: impl Into<String>) {
        self.filter_query = query.into();
        self.reapply_filter();
    }

    pub fn set_column_filter(&mut self, data_col: usize, query: impl Into<String>) {
        if data_col >= self.column_filters.len() {
            self.column_filters.resize(data_col + 1, String::new());
        }
        self.column_filters[data_col] = query.into();
        self.reapply_filter();
    }

    pub fn clear_filters(&mut self) {
        self.filter_query.clear();
        for f in &mut self.column_filters {
            f.clear();
        }
        self.reapply_filter();
    }

    pub fn filtered_count(&self) -> usize {
        self.row_count
    }

    pub fn total_count(&self) -> usize {
        self.data_row_count
    }

    pub fn has_active_filter(&self) -> bool {
        !self.filter_query.is_empty()
            || self.column_filters.iter().any(|f| !f.is_empty())
            || self.filter_predicate.is_some()
    }

    /// Replace merge spans (data coordinates). Invalid spans are dropped.
    pub fn set_spans(&mut self, spans: Vec<CellSpan>) {
        self.spans.clear();
        self.span_cover.clear();
        for span in spans {
            let _ = self.try_add_span(span);
        }
    }

    pub fn add_span(&mut self, span: CellSpan) -> bool {
        self.try_add_span(span)
    }

    pub fn clear_spans(&mut self) {
        self.spans.clear();
        self.span_cover.clear();
    }

    pub fn spans(&self) -> &[CellSpan] {
        &self.spans
    }

    pub fn span_at(&self, data_row: usize, data_col: usize) -> Option<&CellSpan> {
        let idx = *self.span_cover.get(&(data_row, data_col))?;
        self.spans.get(idx)
    }

    pub fn is_covered(&self, data_row: usize, data_col: usize) -> bool {
        match self.span_at(data_row, data_col) {
            Some(s) => !s.is_anchor(data_row, data_col),
            None => false,
        }
    }

    pub fn span_anchor(&self, data_row: usize, data_col: usize) -> (usize, usize) {
        match self.span_at(data_row, data_col) {
            Some(s) => (s.data_row, s.data_col),
            None => (data_row, data_col),
        }
    }

    fn try_add_span(&mut self, span: CellSpan) -> bool {
        if !self.validate_span(&span) {
            return false;
        }
        let idx = self.spans.len();
        for r in span.data_row..span.data_row + span.row_span {
            for c in span.data_col..span.data_col + span.col_span {
                self.span_cover.insert((r, c), idx);
            }
        }
        self.spans.push(span);
        true
    }

    fn validate_span(&self, span: &CellSpan) -> bool {
        if span.row_span == 0 || span.col_span == 0 {
            return false;
        }
        if span.data_row + span.row_span > self.data_row_count {
            return false;
        }
        let n_data_cols = self.columns.len();
        if span.data_col + span.col_span > n_data_cols {
            return false;
        }
        // Overlap with existing spans
        for r in span.data_row..span.data_row + span.row_span {
            for c in span.data_col..span.data_col + span.col_span {
                if self.span_cover.contains_key(&(r, c)) {
                    return false;
                }
            }
        }
        // Freeze zone: all visual cols must stay entirely frozen or entirely scrollable.
        let freeze = self.freeze_leading.min(self.columns.len());
        let mut visuals = Vec::with_capacity(span.col_span);
        for dc in span.data_col..span.data_col + span.col_span {
            let Some(vc) = self.visual_col_of(dc) else {
                return false;
            };
            visuals.push(vc);
        }
        visuals.sort_unstable();
        let all_frozen = visuals.iter().all(|&v| v < freeze);
        let all_scroll = visuals.iter().all(|&v| v >= freeze);
        if freeze > 0 && !(all_frozen || all_scroll) {
            return false;
        }
        // Multi-row spans must remain contiguous in the current visible order.
        if span.row_span > 1 && !self.span_rows_contiguous(span) {
            return false;
        }
        true
    }

    fn span_rows_contiguous(&self, span: &CellSpan) -> bool {
        let mut visuals = Vec::with_capacity(span.row_span);
        for dr in span.data_row..span.data_row + span.row_span {
            let Some(vr) = self.visual_row_of(dr) else {
                return false;
            };
            visuals.push(vr);
        }
        visuals.sort_unstable();
        for w in visuals.windows(2) {
            if w[1] != w[0] + 1 {
                return false;
            }
        }
        true
    }

    fn prune_invalid_spans(&mut self) {
        let old = std::mem::take(&mut self.spans);
        self.span_cover.clear();
        for span in old {
            let _ = self.try_add_span(span);
        }
    }

    pub fn visual_row_of(&self, data_row: usize) -> Option<usize> {
        self.row_order.iter().position(|&r| r == data_row)
    }

    pub fn visual_col_of(&self, data_col: usize) -> Option<usize> {
        self.data_col_of.iter().position(|&d| d == data_col)
    }

    /// Merged cell rect in widget space for an anchor, if currently visible.
    fn merged_cell_rect(&self, span: &CellSpan) -> Option<Rect> {
        let vr0 = self.visual_row_of(span.data_row)?;
        let vc0 = self.visual_col_of(span.data_col)?;
        if span.row_span > 1 && !self.span_rows_contiguous(span) {
            return None;
        }
        let body = self.body_bounds();
        let y0 = body.y0 + self.vp.offset_of(vr0) - self.vp.scroll_y;
        let h = ROW_HEIGHT * span.row_span as f64;
        let x0 = self.column_x(vc0);
        let mut w = 0.0;
        // Sum widths of visual columns covering the data col range (may be reordered).
        let mut visual_cols: Vec<usize> = (span.data_col..span.data_col + span.col_span)
            .filter_map(|dc| self.visual_col_of(dc))
            .collect();
        visual_cols.sort_unstable();
        // Require contiguous visual columns for a single painted rect.
        for w2 in visual_cols.windows(2) {
            if w2[1] != w2[0] + 1 {
                return None;
            }
        }
        for &vc in &visual_cols {
            w += self.columns.get(vc).map(|c| c.width).unwrap_or(0.0);
        }
        Some(Rect::new(x0, y0, x0 + w, y0 + h))
    }

    fn row_matches_filter(&self, data_row: usize) -> bool {
        let global = self.filter_query.trim();
        let global_l = global.to_ascii_lowercase();
        if !global.is_empty() {
            let mut any = false;
            for visual in 0..self.columns.len() {
                let t = self.cell_text(data_row, visual).to_ascii_lowercase();
                if t.contains(&global_l) {
                    any = true;
                    break;
                }
            }
            if !any {
                return false;
            }
        }
        for (data_c, filt) in self.column_filters.iter().enumerate() {
            let f = filt.trim();
            if f.is_empty() {
                continue;
            }
            let f_l = f.to_ascii_lowercase();
            // Find visual col for this data col
            let visual = self
                .data_col_of
                .iter()
                .position(|&d| d == data_c)
                .unwrap_or(data_c);
            let t = self.cell_text(data_row, visual).to_ascii_lowercase();
            if !t.contains(&f_l) {
                return false;
            }
        }
        if let Some(pred) = self.filter_predicate.as_ref() {
            let mut cells = Vec::with_capacity(self.columns.len());
            for visual in 0..self.columns.len() {
                cells.push(self.cell_text(data_row, visual));
            }
            if !pred(&cells) {
                return false;
            }
        }
        true
    }

    pub fn reapply_filter(&mut self) {
        if !self.has_active_filter() {
            self.row_order = self.full_order.clone();
        } else {
            self.row_order = self
                .full_order
                .iter()
                .copied()
                .filter(|&r| self.row_matches_filter(r))
                .collect();
        }
        self.row_count = self.row_order.len();
        self.vp.item_count = self.row_count;
        // Drop selections that are out of range
        self.selected_rows.retain(|&i| i < self.row_count);
        if let Some(a) = self.selection_anchor {
            if a >= self.row_count {
                self.selection_anchor = self.selected_rows.iter().copied().min();
            }
        }
        if let Some((vr, _)) = self.editing {
            if vr >= self.row_count {
                self.editing = None;
            }
        }
        self.prune_invalid_spans();
        self.clamp_scroll();
    }

    pub fn data_col(&self, visual: usize) -> usize {
        self.data_col_of.get(visual).copied().unwrap_or(visual)
    }

    pub fn cell_text(&self, data_row: usize, visual_col: usize) -> String {
        (self.cell_fn)(data_row, self.data_col(visual_col))
    }

    /// Currently mounted `(visual_row, visual_col, widget)` cells.
    pub fn mounted_cells(&self) -> &[(usize, usize, WidgetId)] {
        &self.mounted
    }

    /// Reorder visual column `from` so it lands at index `to` (before that slot).
    pub fn reorder_column(&mut self, from: usize, to: usize) {
        let n = self.columns.len();
        if from >= n || to > n || from == to || from + 1 == to {
            return;
        }
        let freeze = self.freeze_leading.min(n);
        // Stay within freeze zone or outside — never cross the boundary.
        let zone_ok = if from < freeze {
            to <= freeze
        } else {
            to >= freeze
        };
        if !zone_ok {
            return;
        }
        let col = self.columns.remove(from);
        let data = self.data_col_of.remove(from);
        let insert = if to > from { to - 1 } else { to };
        self.columns.insert(insert, col);
        self.data_col_of.insert(insert, data);
        if let Some(sc) = self.sort_column {
            self.sort_column = Some(remap_index_after_move(sc, from, insert));
        }
        self.focused_col = remap_index_after_move(self.focused_col, from, insert);
        if let Some((vr, vc)) = self.editing {
            self.editing = Some((vr, remap_index_after_move(vc, from, insert)));
        }
        self.layout_dirty = true;
        self.clamp_scroll();
    }

    pub fn set_column_width(&mut self, visual_col: usize, width: f64) {
        if let Some(c) = self.columns.get_mut(visual_col) {
            c.width = width.max(MIN_COL_WIDTH);
            self.layout_dirty = true;
            self.clamp_scroll();
        }
    }

    /// Auto-fit column width from currently visible cell strings.
    pub fn auto_fit_column(&mut self, visual_col: usize) {
        if visual_col >= self.columns.len() {
            return;
        }
        let (start, end) = self.visible_range();
        let mut max_chars = self.columns[visual_col].title.len();
        for visual_row in start..end {
            let data_row = self.data_row(visual_row);
            let t = self.cell_text(data_row, visual_col);
            max_chars = max_chars.max(t.len());
        }
        let w = (max_chars as f64 * 7.5 + CELL_PAD_X * 2.0).clamp(MIN_COL_WIDTH, 480.0);
        self.set_column_width(visual_col, w);
    }

    fn hit_resize_edge(&self, x: f64) -> Option<usize> {
        let (col_start, col_end) = self.visible_column_range();
        for col in col_start..col_end {
            let x1 = self.column_x(col) + self.columns[col].width;
            if (x - x1).abs() <= RESIZE_HIT {
                return Some(col);
            }
        }
        None
    }

    fn insert_index_at_x(&self, x: f64, from: usize) -> usize {
        let n = self.columns.len();
        let freeze = self.freeze_leading.min(n);
        let (lo, hi) = if from < freeze {
            (0, freeze)
        } else {
            (freeze, n)
        };
        let mut insert = hi;
        for col in lo..hi {
            let mid = self.column_x(col) + self.columns[col].width * 0.5;
            if x < mid {
                insert = col;
                break;
            }
        }
        insert
    }

    pub fn with_cell_builder(
        mut self,
        f: impl Fn(usize, usize) -> Box<dyn ErasedWidget> + 'static,
    ) -> Self {
        self.set_cell_builder(f);
        self
    }

    pub fn set_cell_builder(
        &mut self,
        f: impl Fn(usize, usize) -> Box<dyn ErasedWidget> + 'static,
    ) {
        self.cell_builder = Some(Box::new(f));
        self.cell_builder_store = None;
        self.use_mount = true;
    }

    pub fn with_cell_builder_store(
        mut self,
        f: impl Fn(&mut WidgetStore, usize, usize) -> WidgetId + 'static,
    ) -> Self {
        self.cell_builder_store = Some(Box::new(f));
        self.cell_builder = None;
        self.use_mount = true;
        self
    }

    pub fn with_freeze_leading(mut self, n: usize) -> Self {
        self.freeze_leading = n;
        self
    }

    pub fn primary_selected(&self) -> Option<usize> {
        self.selection_anchor
            .filter(|i| self.selected_rows.contains(i))
            .or_else(|| self.selected_rows.iter().copied().min())
    }

    fn apply_pointer_selection(&mut self, visual_row: usize, shift: bool, ctrl: bool) {
        if ctrl {
            if self.selected_rows.contains(&visual_row) {
                self.selected_rows.remove(&visual_row);
            } else {
                self.selected_rows.insert(visual_row);
            }
            self.selection_anchor = Some(visual_row);
        } else if shift {
            let anchor = self.selection_anchor.unwrap_or(visual_row);
            let (a, b) = (anchor.min(visual_row), anchor.max(visual_row));
            self.selected_rows = (a..=b).collect();
        } else {
            self.selected_rows.clear();
            self.selected_rows.insert(visual_row);
            self.selection_anchor = Some(visual_row);
        }
    }

    fn move_primary(&mut self, next: usize, shift: bool) {
        if self.row_count == 0 {
            return;
        }
        let next = next.min(self.row_count - 1);
        if shift {
            let anchor = self.selection_anchor.unwrap_or(next);
            let (a, b) = (anchor.min(next), anchor.max(next));
            self.selected_rows = (a..=b).collect();
        } else {
            self.selected_rows.clear();
            self.selected_rows.insert(next);
            self.selection_anchor = Some(next);
        }
    }

    fn freeze_width(&self) -> f64 {
        let n = self.freeze_leading.min(self.columns.len());
        self.columns[..n].iter().map(|c| c.width).sum()
    }

    pub fn with_sort_with(mut self, f: impl Fn(usize, usize, bool) -> Ordering + 'static) -> Self {
        self.sort_with = Some(Box::new(f));
        self
    }

    pub fn set_on_cell_edit(&mut self, f: impl FnMut(usize, usize, String) + 'static) {
        self.on_cell_edit = Some(Box::new(f));
    }

    pub fn take_pending_edit(&mut self) -> Option<(usize, usize, String)> {
        self.pending_edit.take()
    }

    pub fn uses_mount(&self) -> bool {
        self.use_mount
    }

    pub fn mounted_count(&self) -> usize {
        self.mounted.len()
    }

    pub fn data_row(&self, visual_row: usize) -> usize {
        *self.row_order.get(visual_row).unwrap_or(&visual_row)
    }

    pub fn content_width(&self) -> f64 {
        self.columns.iter().map(|c| c.width).sum()
    }

    pub fn content_height(&self) -> f64 {
        self.vp.content_height()
    }

    pub fn scroll_y(&self) -> f64 {
        self.vp.scroll_y
    }

    pub fn set_scroll_y(&mut self, y: f64) {
        self.vp.scroll_y = y;
        self.vp.clamp_scroll();
        crate::virtual_mount::mark_virt_host_dirty(self.id);
    }

    pub fn max_scroll_x(&self) -> f64 {
        (self.content_width() - self.body_bounds().width()).max(0.0)
    }

    pub fn clamp_scroll(&mut self) {
        self.sync_vp_meta();
        self.vp.clamp_scroll();
        self.scroll_x = self.scroll_x.clamp(0.0, self.max_scroll_x());
        crate::virtual_mount::mark_virt_host_dirty(self.id);
    }

    pub fn visible_range(&self) -> (usize, usize) {
        self.vp.visible_range()
    }

    pub fn painted_row_count(&self) -> usize {
        let (a, b) = self.visible_range();
        b.saturating_sub(a)
    }

    fn sync_vp_meta(&mut self) {
        self.vp.item_count = self.row_count;
        self.vp.item_extent = ROW_HEIGHT;
        self.vp.viewport_height = self.body_bounds().height().max(0.0);
    }

    fn body_bounds(&self) -> Rect {
        Rect::new(
            self.bounds.x0,
            self.bounds.y0 + HEADER_HEIGHT,
            self.bounds.x1,
            self.bounds.y1,
        )
    }

    fn header_bounds(&self) -> Rect {
        Rect::new(
            self.bounds.x0,
            self.bounds.y0,
            self.bounds.x1,
            self.bounds.y0 + HEADER_HEIGHT,
        )
    }

    fn column_x(&self, col: usize) -> f64 {
        let freeze = self.freeze_leading.min(self.columns.len());
        let col = col.min(self.columns.len().saturating_sub(1).max(0));
        if self.columns.is_empty() {
            return self.bounds.x0;
        }
        if col < freeze {
            let mut x = self.bounds.x0;
            for c in &self.columns[..col] {
                x += c.width;
            }
            return x;
        }
        let mut x = self.bounds.x0 + self.freeze_width() - self.scroll_x;
        for c in &self.columns[freeze..col] {
            x += c.width;
        }
        x
    }

    fn visible_column_range(&self) -> (usize, usize) {
        if self.columns.is_empty() {
            return (0, 0);
        }
        let freeze = self.freeze_leading.min(self.columns.len());
        let view_x0 = self.bounds.x0 + self.freeze_width();
        let view_x1 = self.bounds.x1 - 14.0;
        let mut start = freeze;
        for i in freeze..self.columns.len() {
            let x1 = self.column_x(i) + self.columns[i].width;
            if x1 > view_x0 {
                start = i;
                break;
            }
            start = i + 1;
        }
        let mut end = start;
        while end < self.columns.len() {
            let x0 = self.column_x(end);
            if x0 >= view_x1 {
                break;
            }
            end += 1;
        }
        let paint_start = if freeze > 0 { 0 } else { start };
        let paint_end = end.max(freeze).max(paint_start + 1).min(self.columns.len());
        (paint_start, paint_end)
    }

    fn hit_cell(&self, position: Point) -> Option<(usize, usize)> {
        let body = self.body_bounds();
        if position.y < body.y0 || position.y >= body.y1 {
            return None;
        }
        // Prefer merged rect hit so rowspan/colspan interiors resolve to the anchor.
        for span in &self.spans {
            if let Some(rect) = self.merged_cell_rect(span) {
                if rect.contains(position) {
                    let vr = self.visual_row_of(span.data_row)?;
                    let vc = self.visual_col_of(span.data_col)?;
                    return Some((vr, vc));
                }
            }
        }
        let local_y = position.y - body.y0 + self.vp.scroll_y;
        let visual_row = self.vp.index_at_offset(local_y)?;
        let (col_start, col_end) = self.visible_column_range();
        for col in col_start..col_end {
            let x0 = self.column_x(col);
            let x1 = x0 + self.columns[col].width;
            if position.x >= x0 && position.x < x1 {
                let data_row = self.data_row(visual_row);
                let data_col = self.data_col(col);
                let (ar, ac) = self.span_anchor(data_row, data_col);
                let vr = self.visual_row_of(ar).unwrap_or(visual_row);
                let vc = self.visual_col_of(ac).unwrap_or(col);
                return Some((vr, vc));
            }
        }
        None
    }

    fn apply_sort(&mut self, col: usize) {
        if col >= self.columns.len() || !self.columns[col].sortable {
            return;
        }
        // Cross-row merges break under reordering — clear spans on sort (Phase 23).
        self.clear_spans();
        if self.sort_column == Some(col) {
            self.sort_asc = !self.sort_asc;
        } else {
            self.sort_column = Some(col);
            self.sort_asc = true;
        }
        let asc = self.sort_asc;
        let data_c = self.data_col(col);
        if let Some(cmp) = self.sort_with.as_ref() {
            self.full_order.sort_by(|&a, &b| cmp(a, b, asc));
        } else {
            self.full_order.sort_by(|&a, &b| {
                let sa = (self.cell_fn)(a, data_c);
                let sb = (self.cell_fn)(b, data_c);
                let ord = sa.cmp(&sb);
                if asc {
                    ord
                } else {
                    ord.reverse()
                }
            });
        }
        self.reapply_filter();
    }

    fn start_edit(&mut self, visual_row: usize, col: usize) {
        let data_row = self.data_row(visual_row);
        self.edit_buffer = self.cell_text(data_row, col);
        self.editing = Some((visual_row, col));
        if !self.selected_rows.contains(&visual_row) {
            self.apply_pointer_selection(visual_row, false, false);
        }
        self.focused_col = col;
    }

    fn commit_edit(&mut self) {
        let Some((visual_row, col)) = self.editing.take() else {
            return;
        };
        let data_row = self.data_row(visual_row);
        let data_c = self.data_col(col);
        let value = self.edit_buffer.clone();
        if let Some(cb) = self.on_cell_edit.as_mut() {
            cb(data_row, data_c, value.clone());
        } else {
            self.pending_edit = Some((data_row, data_c, value));
        }
        self.edit_buffer.clear();
    }

    fn cancel_edit(&mut self) {
        self.editing = None;
        self.edit_buffer.clear();
    }

    fn ensure_selected_visible(&mut self) {
        if let Some(row) = self.primary_selected() {
            self.vp.ensure_index_visible(row);
        }
        self.ensure_col_visible(self.focused_col);
    }

    /// Scroll horizontally so `visual_col` is in the body viewport.
    pub fn ensure_col_visible(&mut self, visual_col: usize) {
        if visual_col < self.freeze_leading || visual_col >= self.columns.len() {
            return;
        }
        let freeze = self.freeze_leading.min(self.columns.len());
        let mut x0 = 0.0;
        for c in &self.columns[freeze..visual_col] {
            x0 += c.width;
        }
        let w = self.columns[visual_col].width;
        let x1 = x0 + w;
        let body = self.body_bounds();
        let view_w = (body.width() - self.freeze_width()).max(1.0);
        if x0 < self.scroll_x {
            self.scroll_x = x0.max(0.0);
        } else if x1 > self.scroll_x + view_w {
            self.scroll_x = (x1 - view_w).max(0.0);
        }
        self.clamp_scroll();
    }

    /// Whether this table currently owns keyboard focus (post-recycle restore).
    pub fn is_focused_host(&self) -> bool {
        self.focused
    }

    fn tick_click(&mut self, now_ms: f64, visual_row: usize, col: usize) -> bool {
        let double = self
            .last_click
            .map(|(t, r, c)| r == visual_row && c == col && now_ms - t <= DOUBLE_CLICK_MS)
            .unwrap_or(false);
        self.last_click = Some((now_ms, visual_row, col));
        self.last_click_time_ms = now_ms;
        double
    }

    pub fn advance_time(&mut self, delta_ms: f32) {
        self.last_click_time_ms += delta_ms as f64;
        if let Some((t, r, c)) = self.last_click {
            if self.last_click_time_ms - t > DOUBLE_CLICK_MS {
                self.last_click = Some((self.last_click_time_ms, r, c));
            }
        }
    }
}

impl Widget for DataTable {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn children(&self) -> &[WidgetId] {
        &self.child_ids
    }

    fn manages_child_layout(&self) -> bool {
        true
    }

    fn layout_style(&self) -> &LayoutStyle {
        use std::sync::OnceLock;
        static STYLE: OnceLock<LayoutStyle> = OnceLock::new();
        STYLE.get_or_init(|| LayoutStyle {
            display: synui_core::Display::Flex,
            width: Dimension::Fill,
            height: Dimension::Fill,
            flex_grow: 1.0,
            flex_shrink: 1.0,
            ..LayoutStyle::base()
        })
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
        self.sync_vp_meta();
        self.clamp_scroll();
    }

    fn measure(&mut self, _ctx: &mut MeasureCtx, constraints: Constraints) -> Size {
        // Unbounded measure must stay finite — returning ∞ collapses sibling Labels
        // via flex-shrink and makes Preview↔Table chrome gaps jump.
        let width = if constraints.max_width.is_finite() {
            constraints.max_width as f64
        } else {
            480.0
        };
        let height = if constraints.max_height.is_finite() {
            constraints.max_height as f64
        } else {
            240.0
        };
        Size::new(width, height.max(120.0))
    }

    fn clip_bounds(&self) -> Option<Rect> {
        Some(self.body_bounds())
    }

    fn paint(&self, ctx: &mut PaintCtx, scene: &mut Scene) {
        let body = self.body_bounds();
        paint_simple_rect(scene, body, ctx.theme.surface, 0.0);
        let (col_start, col_end) = self.visible_column_range();

        if !self.use_mount {
            let (start, end) = self.visible_range();
            if let Some(text) = ctx.text.as_deref_mut() {
                for visual_row in start..end {
                    let y0 = body.y0 + self.vp.offset_of(visual_row) - self.vp.scroll_y;
                    let row_rect = Rect::new(body.x0, y0, body.x1, y0 + ROW_HEIGHT);
                    if row_rect.y1 < body.y0 || row_rect.y0 > body.y1 {
                        continue;
                    }
                    if self.selected_rows.contains(&visual_row) {
                        let mut hl = ctx.theme.primary;
                        hl.components[3] = 0.15;
                        paint_simple_rect(scene, row_rect, hl, 0.0);
                    }
                    let data_row = self.data_row(visual_row);
                    for col in col_start..col_end {
                        let data_c = self.data_col(col);
                        if self.is_covered(data_row, data_c) {
                            continue;
                        }
                        let def = &self.columns[col];
                        let x0 = self.column_x(col);
                        let freeze_edge = self.bounds.x0 + self.freeze_width();
                        if col >= self.freeze_leading && x0 < freeze_edge - 0.5 {
                            continue;
                        }
                        let cell = if let Some(span) = self.span_at(data_row, data_c) {
                            if span.is_anchor(data_row, data_c) {
                                self.merged_cell_rect(span).unwrap_or_else(|| {
                                    Rect::new(x0, y0, x0 + def.width, y0 + ROW_HEIGHT)
                                })
                            } else {
                                continue;
                            }
                        } else {
                            Rect::new(x0, y0, x0 + def.width, y0 + ROW_HEIGHT)
                        };
                        if cell.x1 < body.x0 || cell.x0 > body.x1 {
                            continue;
                        }
                        if cell.y1 < body.y0 || cell.y0 > body.y1 {
                            continue;
                        }
                        let is_editing = self.editing == Some((visual_row, col));
                        let owned = if is_editing {
                            None
                        } else {
                            Some(self.cell_text(data_row, col))
                        };
                        let display = owned.as_deref().unwrap_or(self.edit_buffer.as_str());
                        text.paint_text(
                            scene,
                            display,
                            13.0,
                            ctx.theme.on_surface,
                            Rect::new(cell.x0 + CELL_PAD_X, cell.y0, cell.x1 - 4.0, cell.y1),
                            Some((cell.width() - CELL_PAD_X - 4.0).max(1.0) as f32),
                            TextAlign::Start,
                        );
                        if is_editing {
                            let caret_x = cell.x0 + CELL_PAD_X + (display.len() as f64 * 7.0);
                            paint_simple_rect(
                                scene,
                                Rect::new(caret_x, cell.y0 + 6.0, caret_x + 1.0, cell.y1 - 6.0),
                                ctx.theme.primary,
                                0.0,
                            );
                        }
                    }
                }
            }
        } else {
            for &sel in &self.selected_rows {
                let y0 = body.y0 + self.vp.offset_of(sel) - self.vp.scroll_y;
                let row_rect = Rect::new(body.x0, y0, body.x1, y0 + ROW_HEIGHT);
                if row_rect.y1 < body.y0 || row_rect.y0 > body.y1 {
                    continue;
                }
                let mut hl = ctx.theme.primary;
                hl.components[3] = 0.15;
                paint_simple_rect(scene, row_rect, hl, 0.0);
            }
        }

        let (width, edge) = if self.focused {
            let mut edge = ctx.theme.primary;
            edge.components[3] = 0.5;
            (1.5, edge)
        } else {
            (1.0, ctx.theme.field_border())
        };
        let inset = Rect::new(
            self.bounds.x0 + 1.0,
            self.bounds.y0 + 1.0,
            self.bounds.x1 - 1.0,
            self.bounds.y1 - 1.0,
        );
        use vello::kurbo::{RoundedRect, Stroke};
        scene.stroke(
            &Stroke::new(width),
            vello::kurbo::Affine::IDENTITY,
            edge,
            None,
            &RoundedRect::from_rect(inset, 0.0),
        );

        let mut track = ctx.theme.outline;
        track.components[3] = 0.25;
        let mut thumb = ctx.theme.on_surface;
        thumb.components[3] = if self.scrollbar.is_dragging() {
            0.65
        } else {
            0.45
        };
        paint_v_scrollbar(
            scene,
            body,
            self.vp.scroll_y,
            self.content_height(),
            track,
            thumb,
        );
    }

    fn paint_overlay(&self, ctx: &mut PaintCtx, scene: &mut Scene) {
        let header = self.header_bounds();
        paint_simple_rect(scene, header, ctx.theme.surface_container, 0.0);
        let (col_start, col_end) = self.visible_column_range();
        if let Some(text) = ctx.text.as_deref_mut() {
            for col in col_start..col_end {
                let def = &self.columns[col];
                let x0 = self.column_x(col);
                let cell = Rect::new(x0, header.y0, x0 + def.width, header.y1);
                if cell.x1 < self.bounds.x0 || cell.x0 > self.bounds.x1 {
                    continue;
                }
                let mut title_color = ctx.theme.on_surface;
                if self.sort_column == Some(col) {
                    title_color = ctx.theme.primary;
                }
                text.paint_text(
                    scene,
                    &def.title,
                    13.0,
                    title_color,
                    Rect::new(cell.x0 + CELL_PAD_X, cell.y0, cell.x1 - 4.0, cell.y1),
                    Some((cell.width() - CELL_PAD_X - 4.0).max(1.0) as f32),
                    TextAlign::Start,
                );
                if self.sort_column == Some(col) {
                    let mark = if self.sort_asc { " ▲" } else { " ▼" };
                    text.paint_text(
                        scene,
                        mark,
                        10.0,
                        ctx.theme.primary,
                        Rect::new(cell.x1 - 20.0, cell.y0, cell.x1 - 4.0, cell.y1),
                        Some(16.0),
                        TextAlign::Start,
                    );
                }
            }
        }
        // Insert indicator while reordering
        if let Some(HeaderDrag::Reorder { insert_at, .. }) = &self.header_drag {
            let x = if *insert_at >= self.columns.len() {
                self.column_x(self.columns.len().saturating_sub(1))
                    + self.columns.last().map(|c| c.width).unwrap_or(0.0)
            } else {
                self.column_x(*insert_at)
            };
            paint_simple_rect(
                scene,
                Rect::new(x - 1.0, header.y0, x + 1.0, header.y1),
                ctx.theme.primary,
                0.0,
            );
        }
        // Highlight resize edge
        if let Some(HeaderDrag::Resize { col, .. }) = &self.header_drag {
            let x1 = self.column_x(*col) + self.columns[*col].width;
            paint_simple_rect(
                scene,
                Rect::new(x1 - 1.0, header.y0, x1 + 1.0, header.y1),
                ctx.theme.primary,
                0.0,
            );
        }
    }

    fn event(&mut self, ctx: &mut EventCtx, event: &Event) -> EventResult {
        self.sync_vp_meta();
        let body = self.body_bounds();
        let header = self.header_bounds();
        match event {
            Event::PointerDown {
                button: PointerButton::Primary,
                position,
                shift,
                ctrl,
                ..
            } if self.bounds.contains(*position) => {
                if let Some(y) = self.scrollbar.pointer_down(
                    *position,
                    body,
                    self.vp.scroll_y,
                    self.content_height(),
                ) {
                    self.vp.scroll_y = y;
                    self.clamp_scroll();
                    self.focused = true;
                    ctx.grab_pointer();
                    return EventResult::HANDLED;
                }
                self.focused = true;
                if header.contains(*position) {
                    if let Some(edge) = self.hit_resize_edge(position.x) {
                        // Double-click resize edge → auto-fit
                        let now = self.last_click_time_ms;
                        let dbl = self
                            .last_click
                            .map(|(t, r, c)| {
                                r == usize::MAX && c == edge && now - t <= DOUBLE_CLICK_MS
                            })
                            .unwrap_or(false);
                        self.last_click = Some((now, usize::MAX, edge));
                        if dbl {
                            self.auto_fit_column(edge);
                            return EventResult::HANDLED;
                        }
                        self.header_drag = Some(HeaderDrag::Resize {
                            col: edge,
                            start_x: position.x,
                            start_width: self.columns[edge].width,
                        });
                        ctx.grab_pointer();
                        return EventResult::HANDLED;
                    }
                    let (col_start, col_end) = self.visible_column_range();
                    for col in col_start..col_end {
                        let x0 = self.column_x(col);
                        let x1 = x0 + self.columns[col].width;
                        if position.x >= x0 && position.x < x1 {
                            self.header_drag = Some(HeaderDrag::Pending {
                                col,
                                start_x: position.x,
                            });
                            ctx.grab_pointer();
                            return EventResult::HANDLED;
                        }
                    }
                }
                if let Some((visual_row, col)) = self.hit_cell(*position) {
                    self.apply_pointer_selection(visual_row, *shift, *ctrl);
                    self.focused_col = col;
                    self.cancel_edit();
                    let now = self.last_click_time_ms;
                    if !*shift && !*ctrl && self.tick_click(now, visual_row, col) {
                        self.start_edit(visual_row, col);
                    }
                } else if !*ctrl && !*shift {
                    self.selected_rows.clear();
                    self.selection_anchor = None;
                }
                EventResult::HANDLED
            }
            Event::PointerMove { position, .. } => {
                if let Some(y) = self
                    .scrollbar
                    .pointer_move(*position, body, self.content_height())
                {
                    self.vp.scroll_y = y;
                    self.clamp_scroll();
                    return EventResult::HANDLED;
                }
                match self.header_drag.clone() {
                    Some(HeaderDrag::Pending { col, start_x }) => {
                        if (position.x - start_x).abs() >= REORDER_THRESHOLD {
                            let insert_at = self.insert_index_at_x(position.x, col);
                            self.header_drag = Some(HeaderDrag::Reorder {
                                from: col,
                                insert_at,
                            });
                        }
                        return EventResult::HANDLED;
                    }
                    Some(HeaderDrag::Reorder { from, .. }) => {
                        let insert_at = self.insert_index_at_x(position.x, from);
                        self.header_drag = Some(HeaderDrag::Reorder { from, insert_at });
                        return EventResult::HANDLED;
                    }
                    Some(HeaderDrag::Resize {
                        col,
                        start_x,
                        start_width,
                    }) => {
                        let new_w = start_width + (position.x - start_x);
                        self.set_column_width(col, new_w);
                        self.header_drag = Some(HeaderDrag::Resize {
                            col,
                            start_x,
                            start_width,
                        });
                        return EventResult::HANDLED;
                    }
                    None => {}
                }
                EventResult::IGNORED
            }
            Event::PointerUp { .. } => {
                if self.scrollbar.pointer_up() {
                    ctx.release_pointer();
                    return EventResult::HANDLED;
                }
                if let Some(drag) = self.header_drag.take() {
                    ctx.release_pointer();
                    match drag {
                        HeaderDrag::Pending { col, .. } => {
                            self.apply_sort(col);
                        }
                        HeaderDrag::Reorder { from, insert_at } => {
                            self.reorder_column(from, insert_at);
                        }
                        HeaderDrag::Resize { .. } => {}
                    }
                    return EventResult::HANDLED;
                }
                EventResult::IGNORED
            }
            Event::Scroll {
                position,
                delta_x,
                delta_y,
                ..
            } if self.bounds.contains(*position) || self.focused => {
                let max_y = self.vp.max_scroll();
                if scroll_at_edge(self.vp.scroll_y, max_y, *delta_y as f64) && *delta_x == 0.0 {
                    return EventResult::IGNORED;
                }
                self.vp.scroll_y -= *delta_y as f64;
                self.scroll_x -= *delta_x as f64;
                self.clamp_scroll();
                ctx.stop_propagation();
                EventResult::HANDLED
            }
            Event::FocusGained => {
                self.focused = true;
                EventResult::HANDLED
            }
            Event::FocusLost => {
                self.focused = false;
                if self.editing.is_some() {
                    self.commit_edit();
                }
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::Enter),
                ..
            } if self.focused => {
                if self.editing.is_some() {
                    self.commit_edit();
                } else if let Some(row) = self.primary_selected() {
                    self.start_edit(row, self.focused_col);
                }
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::Escape),
                ..
            } if self.focused && self.editing.is_some() => {
                self.cancel_edit();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::Backspace),
                ..
            } if self.focused && self.editing.is_some() => {
                self.edit_buffer.pop();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::ArrowDown),
                shift,
                ..
            } if self.focused => {
                if self.editing.is_some() {
                    self.commit_edit();
                }
                let cur = self.primary_selected().unwrap_or(0);
                let next = (cur + 1).min(self.row_count.saturating_sub(1));
                self.move_primary(next, *shift);
                self.ensure_selected_visible();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::ArrowUp),
                shift,
                ..
            } if self.focused => {
                if self.editing.is_some() {
                    self.commit_edit();
                }
                let cur = self.primary_selected().unwrap_or(0);
                let next = cur.saturating_sub(1);
                self.move_primary(next, *shift);
                self.ensure_selected_visible();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::ArrowRight),
                ..
            } if self.focused && self.editing.is_none() => {
                let cols = self.columns.len().saturating_sub(1);
                self.focused_col = (self.focused_col + 1).min(cols);
                if let Some(row) = self.primary_selected() {
                    let data_row = self.data_row(row);
                    let data_col = self.data_col(self.focused_col);
                    let (ar, ac) = self.span_anchor(data_row, data_col);
                    if let Some(vr) = self.visual_row_of(ar) {
                        self.move_primary(vr, false);
                    }
                    self.focused_col = self.visual_col_of(ac).unwrap_or(self.focused_col);
                }
                self.ensure_selected_visible();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Named(NamedKey::ArrowLeft),
                ..
            } if self.focused && self.editing.is_none() => {
                self.focused_col = self.focused_col.saturating_sub(1);
                if let Some(row) = self.primary_selected() {
                    let data_row = self.data_row(row);
                    let data_col = self.data_col(self.focused_col);
                    let (ar, ac) = self.span_anchor(data_row, data_col);
                    if let Some(vr) = self.visual_row_of(ar) {
                        self.move_primary(vr, false);
                    }
                    self.focused_col = self.visual_col_of(ac).unwrap_or(self.focused_col);
                }
                self.ensure_selected_visible();
                EventResult::HANDLED
            }
            Event::KeyDown {
                key: Key::Character(c),
                ..
            } if self.focused && self.editing.is_some() => {
                self.edit_buffer.push_str(c);
                EventResult::HANDLED
            }
            _ => EventResult::IGNORED,
        }
    }

    fn a11y(&self) -> A11yProps {
        A11yProps {
            role: A11yRole::Grid,
            label: Some(format!(
                "DataTable ({} of {} rows, {} cols)",
                self.row_count,
                self.data_row_count,
                self.columns.len()
            )),
            focusable: true,
            actions: vec![A11yAction::Focus],
            row_count: Some(self.row_count),
            col_count: Some(self.columns.len()),
            row_index: self.primary_selected(),
            col_index: Some(self.focused_col),
            ..Default::default()
        }
    }

    fn is_focusable(&self) -> bool {
        true
    }

    fn hit_test_prefer_self(&self, point: Point) -> bool {
        let body = self.body_bounds();
        VScrollbarState::hit(body, self.vp.scroll_y, self.content_height(), point)
            || self.scrollbar.is_dragging()
            || self.header_drag.is_some()
    }
}

fn remap_index_after_move(idx: usize, from: usize, insert: usize) -> usize {
    if idx == from {
        return insert;
    }
    let mut i = idx;
    if from < i {
        i -= 1;
    }
    if insert <= i {
        i += 1;
    }
    i
}

/// Mount / recycle DataTable cell widgets for visible rows × columns.
pub fn sync_data_tables(store: &mut WidgetStore, text: &mut dyn TextMeasurer) {
    let ids: Vec<WidgetId> = {
        let mut out = Vec::new();
        if let Some(root) = store.root() {
            fn walk(store: &WidgetStore, id: WidgetId, out: &mut Vec<WidgetId>) {
                out.push(id);
                for &c in store.children_of(id) {
                    walk(store, c, out);
                }
            }
            walk(store, root, &mut out);
        }
        out
    };
    for id in ids {
        sync_data_table(store, text, id);
    }
}

/// Sync a single DataTable host (Phase 26).
pub fn sync_data_table(store: &mut WidgetStore, text: &mut dyn TextMeasurer, id: WidgetId) {
    let needs = store
        .get(id)
        .map(|w| {
            w.as_any()
                .downcast_ref::<DataTable>()
                .map(|dt| dt.use_mount)
                .unwrap_or(false)
        })
        .unwrap_or(false);
    if !needs {
        return;
    }
    {
        let w = store.get_mut(id).unwrap();
        let dt = w.as_any_mut().downcast_mut::<DataTable>().unwrap();
        dt.sync_vp_meta();
    }
    let (row_start, row_end, col_start, col_end, scroll_y, old_mounted) = {
        let w = store.get(id).unwrap();
        let dt = w.as_any().downcast_ref::<DataTable>().unwrap();
        let (rs, re) = dt.visible_range();
        let (cs, ce) = dt.visible_column_range();
        (rs, re, cs, ce, dt.vp.scroll_y, dt.mounted.clone())
    };
    let mut desired: Vec<(usize, usize)> = Vec::new();
    for visual_row in row_start..row_end {
        for col in col_start..col_end {
            desired.push((visual_row, col));
        }
    }
    let desired_set: HashSet<(usize, usize)> = desired.iter().copied().collect();
    let mut keep: Vec<(usize, usize, WidgetId)> = Vec::new();
    let mut to_pool: Vec<(usize, usize, WidgetId)> = Vec::new();
    for (vr, col, wid) in old_mounted {
        if desired_set.contains(&(vr, col)) {
            keep.push((vr, col, wid));
        } else {
            let data_row = store
                .get(id)
                .and_then(|w| {
                    w.as_any()
                        .downcast_ref::<DataTable>()
                        .map(|dt| dt.data_row(vr))
                })
                .unwrap_or(vr);
            to_pool.push((data_row, col, wid));
        }
    }
    let mut remove_ids: Vec<WidgetId> = Vec::new();
    {
        let dt = store
            .get_mut(id)
            .unwrap()
            .as_any_mut()
            .downcast_mut::<DataTable>()
            .unwrap();
        for (data_row, col, wid) in to_pool {
            let key = (data_row, col);
            if let Some(old) = dt.pool.insert(key, wid) {
                remove_ids.push(old);
            }
            dt.pool_order.retain(|k| *k != key);
            dt.pool_order.push_back(key);
        }
        while dt.pool.len() > TABLE_POOL_CAP {
            if let Some(evict) = dt.pool_order.pop_front() {
                if let Some(wid) = dt.pool.remove(&evict) {
                    remove_ids.push(wid);
                }
            } else {
                break;
            }
        }
    }
    for wid in remove_ids {
        store.remove(wid);
    }
    let keep_set: HashSet<(usize, usize)> = keep.iter().map(|(r, c, _)| (*r, *c)).collect();
    for &(visual_row, col) in &desired {
        if keep_set.contains(&(visual_row, col)) {
            continue;
        }
        let data_row = store
            .get(id)
            .and_then(|w| {
                w.as_any()
                    .downcast_ref::<DataTable>()
                    .map(|dt| dt.data_row(visual_row))
            })
            .unwrap_or(visual_row);
        let pooled = {
            let dt = store
                .get_mut(id)
                .unwrap()
                .as_any_mut()
                .downcast_mut::<DataTable>()
                .unwrap();
            let key = (data_row, col);
            dt.pool_order.retain(|k| *k != key);
            dt.pool.remove(&key)
        };
        let wid = if let Some(wid) = pooled {
            wid
        } else {
            let store_builder = {
                let dt = store
                    .get_mut(id)
                    .unwrap()
                    .as_any_mut()
                    .downcast_mut::<DataTable>()
                    .unwrap();
                dt.cell_builder_store.take()
            };
            if let Some(builder) = store_builder {
                let data_c = store
                    .get(id)
                    .and_then(|w| {
                        w.as_any()
                            .downcast_ref::<DataTable>()
                            .map(|dt| dt.data_col(col))
                    })
                    .unwrap_or(col);
                let wid = builder(store, data_row, data_c);
                if let Some(w) = store.get_mut(id) {
                    if let Some(dt) = w.as_any_mut().downcast_mut::<DataTable>() {
                        dt.cell_builder_store = Some(builder);
                    }
                }
                wid
            } else {
                let widget: Box<dyn ErasedWidget> = {
                    let w = store.get(id).unwrap();
                    let dt = w.as_any().downcast_ref::<DataTable>().unwrap();
                    let data_c = dt.data_col(col);
                    if let Some(builder) = dt.cell_builder.as_ref() {
                        builder(data_row, data_c)
                    } else {
                        let text = (dt.cell_fn)(data_row, data_c);
                        Box::new(Label::new(text))
                    }
                };
                store.insert_erased(widget)
            }
        };
        keep.push((visual_row, col, wid));
    }
    keep.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    let child_ids: Vec<WidgetId> = keep.iter().map(|(_, _, wid)| *wid).collect();
    store.set_children(id, child_ids.clone());

    for &(visual_row, col, wid) in &keep {
        let (origin, size) = {
            let w = store.get(id).unwrap();
            let dt = w.as_any().downcast_ref::<DataTable>().unwrap();
            let body = dt.body_bounds();
            let y0 = body.y0 + dt.vp.offset_of(visual_row) - scroll_y;
            let x0 = dt.column_x(col);
            let wcol = dt.columns[col].width;
            (
                Point::new(x0 + 2.0, y0),
                Size::new((wcol - 4.0).max(20.0), ROW_HEIGHT),
            )
        };
        let target = Rect::new(
            origin.x,
            origin.y,
            origin.x + size.width,
            origin.y + size.height,
        );
        let already = store
            .get(wid)
            .map(|w| {
                let b = w.bounds();
                (b.x0 - target.x0).abs() < 0.5
                    && (b.y0 - target.y0).abs() < 0.5
                    && (b.x1 - target.x1).abs() < 0.5
                    && (b.y1 - target.y1).abs() < 0.5
            })
            .unwrap_or(false);
        if !already {
            layout_row_subtree(store, text, wid, origin, size);
        }
    }
    if let Some(w) = store.get_mut(id) {
        if let Some(dt) = w.as_any_mut().downcast_mut::<DataTable>() {
            dt.mounted = keep;
            dt.child_ids = child_ids;
            dt.clamp_scroll();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn painted_rows_bounded_for_10k() {
        let cols = vec![
            ColumnDef::new("a", "A", 100.0),
            ColumnDef::new("b", "B", 100.0),
        ];
        let mut table = DataTable::new(cols, 10_000);
        table.set_bounds(Rect::new(0.0, 0.0, 400.0, 320.0));
        let n = table.painted_row_count();
        assert!(n <= 40, "painted {n} rows for 10k table");
        assert!(n >= 8);
    }

    #[test]
    fn sort_toggles_order() {
        let cols = vec![
            ColumnDef::new("name", "Name", 120.0),
            ColumnDef::new("val", "Value", 80.0),
        ];
        let mut table = DataTable::new(cols, 3).with_cell_fn(|row, col| {
            if col == 0 {
                match row {
                    0 => "c".into(),
                    1 => "a".into(),
                    _ => "b".into(),
                }
            } else {
                row.to_string()
            }
        });
        table.apply_sort(0);
        assert!(table.sort_asc);
        assert_eq!(table.data_row(0), 1);
        assert_eq!(table.data_row(1), 2);
        assert_eq!(table.data_row(2), 0);
        table.apply_sort(0);
        assert!(!table.sort_asc);
        assert_eq!(table.data_row(0), 0);
        assert_eq!(table.data_row(2), 1);
    }

    #[test]
    fn sticky_header_shrinks_body() {
        let cols = vec![ColumnDef::new("a", "A", 100.0)];
        let mut table = DataTable::new(cols, 10);
        table.set_bounds(Rect::new(0.0, 0.0, 300.0, 200.0));
        assert_eq!(table.body_bounds().y0, HEADER_HEIGHT);
        assert!(table.body_bounds().height() < 200.0);
    }

    #[test]
    fn freeze_leading_ignores_scroll_x() {
        let cols = vec![
            ColumnDef::new("id", "Id", 100.0),
            ColumnDef::new("a", "A", 100.0),
            ColumnDef::new("b", "B", 100.0),
        ];
        let mut table = DataTable::new(cols, 5).with_freeze_leading(1);
        table.set_bounds(Rect::new(0.0, 0.0, 250.0, 200.0));
        let x0 = table.column_x(0);
        table.scroll_x = 80.0;
        assert!(
            (table.column_x(0) - x0).abs() < 0.1,
            "frozen col must not move"
        );
        assert!(table.column_x(1) < x0 + 100.0);
    }

    #[test]
    fn multi_select_shift_range() {
        let cols = vec![ColumnDef::new("a", "A", 80.0)];
        let mut table = DataTable::new(cols, 10);
        table.apply_pointer_selection(2, false, false);
        table.apply_pointer_selection(5, true, false);
        assert_eq!(table.selected_rows.len(), 4);
        assert!(table.selected_rows.contains(&2));
        assert!(table.selected_rows.contains(&5));
        table.apply_pointer_selection(3, false, true);
        assert!(!table.selected_rows.contains(&3));
    }

    #[test]
    fn reorder_column_swaps_visual_keeps_data() {
        let cols = vec![
            ColumnDef::new("a", "A", 80.0),
            ColumnDef::new("b", "B", 80.0),
            ColumnDef::new("c", "C", 80.0),
        ];
        let mut table = DataTable::new(cols, 1).with_cell_fn(|_r, c| format!("d{c}"));
        // Move visual 0 (A) to before index 2 → A B C becomes B A C
        table.reorder_column(0, 2);
        assert_eq!(table.columns[0].id, "b");
        assert_eq!(table.columns[1].id, "a");
        assert_eq!(table.columns[2].id, "c");
        assert_eq!(table.cell_text(0, 0), "d1");
        assert_eq!(table.cell_text(0, 1), "d0");
        assert_eq!(table.cell_text(0, 2), "d2");
    }

    #[test]
    fn reorder_cannot_cross_freeze() {
        let cols = vec![
            ColumnDef::new("a", "A", 80.0),
            ColumnDef::new("b", "B", 80.0),
            ColumnDef::new("c", "C", 80.0),
        ];
        let mut table = DataTable::new(cols, 1).with_freeze_leading(1);
        table.reorder_column(0, 2); // try to move frozen into scroll zone
        assert_eq!(table.columns[0].id, "a");
        table.reorder_column(2, 0); // try to move scroll into freeze
        assert_eq!(table.columns[2].id, "c");
        table.reorder_column(1, 3); // within scroll zone: b,c → c,b
        assert_eq!(table.columns[1].id, "c");
        assert_eq!(table.columns[2].id, "b");
    }

    #[test]
    fn resize_clamps_min_width() {
        let cols = vec![ColumnDef::new("a", "A", 100.0)];
        let mut table = DataTable::new(cols, 1);
        table.set_column_width(0, 10.0);
        assert!((table.columns[0].width - MIN_COL_WIDTH).abs() < 0.1);
    }

    #[test]
    fn filter_query_reduces_visible_rows() {
        let cols = vec![
            ColumnDef::new("name", "Name", 80.0),
            ColumnDef::new("flag", "Flag", 80.0),
        ];
        let mut table = DataTable::new(cols, 5).with_cell_fn(|r, c| match (r, c) {
            (0, 0) => "alpha".into(),
            (1, 0) => "beta".into(),
            (2, 0) => "alpha2".into(),
            (_, 1) => "pinned".into(),
            _ => format!("r{r}"),
        });
        // Fix flags: only some pinned
        table.set_cell_fn(|r, c| match (r, c) {
            (_, 0) => format!("name{r}"),
            (0, 1) | (2, 1) | (4, 1) => "pinned".into(),
            (_, 1) => "ok".into(),
            _ => String::new(),
        });
        table.set_filter_query("pinned");
        assert_eq!(table.filtered_count(), 3);
        assert_eq!(table.total_count(), 5);
        table.clear_filters();
        assert_eq!(table.filtered_count(), 5);
        table.set_column_filter(1, "ok");
        assert_eq!(table.filtered_count(), 2);
    }

    #[test]
    fn filter_predicate_ands_with_query() {
        let cols = vec![ColumnDef::new("n", "N", 80.0)];
        let mut table = DataTable::new(cols, 4).with_cell_fn(|r, _| format!("r{r}"));
        table.set_filter_predicate(Some(Box::new(|cells| {
            cells
                .first()
                .map(|s| s.ends_with('1') || s.ends_with('3'))
                .unwrap_or(false)
        })));
        assert_eq!(table.filtered_count(), 2);
        table.set_filter_query("r1");
        assert_eq!(table.filtered_count(), 1);
    }

    #[test]
    fn merge_span_paint_and_hit_anchor() {
        let cols = vec![
            ColumnDef::new("a", "A", 80.0),
            ColumnDef::new("b", "B", 80.0),
            ColumnDef::new("c", "C", 80.0),
        ];
        let mut table = DataTable::new(cols, 4).with_cell_fn(|r, c| format!("{r}-{c}"));
        table.set_bounds(Rect::new(0.0, 0.0, 400.0, 300.0));
        table.sync_vp_meta();
        assert!(table.add_span(CellSpan::new(1, 0, 2, 2)));
        assert!(table.is_covered(1, 1));
        assert!(table.is_covered(2, 0));
        assert!(!table.is_covered(1, 0));
        assert_eq!(table.span_anchor(2, 1), (1, 0));

        let body = table.body_bounds();
        // Hit covered cell (row 2, col 1) → anchor (visual 1, col 0).
        let y = body.y0 + ROW_HEIGHT * 2.0 + 4.0;
        let x = 80.0 + 10.0; // col 1
        let hit = table.hit_cell(Point::new(x, y)).unwrap();
        assert_eq!(hit, (1, 0));
    }

    #[test]
    fn merge_rejects_freeze_cross() {
        let cols = vec![
            ColumnDef::new("a", "A", 80.0),
            ColumnDef::new("b", "B", 80.0),
            ColumnDef::new("c", "C", 80.0),
        ];
        let mut table = DataTable::new(cols, 3).with_freeze_leading(1);
        assert!(!table.add_span(CellSpan::new(0, 0, 1, 2))); // crosses freeze
        assert!(table.add_span(CellSpan::new(0, 1, 1, 2))); // both scrollable
    }

    #[test]
    fn merge_cleared_on_sort() {
        let cols = vec![ColumnDef::new("a", "A", 80.0)];
        let mut table = DataTable::new(cols, 3).with_cell_fn(|r, _| format!("{r}"));
        assert!(table.add_span(CellSpan::new(0, 0, 2, 1)));
        assert_eq!(table.spans().len(), 1);
        table.apply_sort(0);
        assert!(table.spans().is_empty());
    }
}
