//! Stable ownership and diagnostics foundation for the SynUI production C ABI.
//!
//! Opaque pointers are non-dereferenceable lease tokens backed by private
//! allocations. Every operation resolves a token through an internal table and
//! validates its type and resource generation before touching resource state.
//! Token allocations become process-lifetime tombstones after release; this
//! deliberately trades memory proportional to issued leases for stale-pointer
//! safety without dereferencing caller memory.

// Pointer validity and ownership are the C contract documented in
// `include/synui_v1.h`; Rust callers are not the supported ABI surface.
#![allow(clippy::missing_safety_doc)]

use core::{ffi::c_void, mem, ptr};
use std::{
    collections::{HashMap, VecDeque},
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{Arc, Mutex, MutexGuard, OnceLock},
    thread::{self, ThreadId},
};
use synui::{
    app::{Action, App as FacadeApp, AppConfig, AppContext, State},
    model::{
        CellCoordinate, CellRange, CellRequest, CellValue, Model as FacadeModel, PrefetchBand,
        SelectionChange, SortDescriptor, SortDirection, TableDimensions, VirtualDataTableAdapter,
        VirtualDataTableModel, VirtualModelError,
    },
    view::{Element, LanguageHost, Node},
    widgets::Button,
};

pub const SYNUI_ABI_MAJOR: u32 = 1;
pub const SYNUI_ABI_MINOR: u32 = 1;

pub type SynuiCapabilities = u64;
pub const SYNUI_CAPABILITY_BASE: SynuiCapabilities = 1 << 0;
pub const SYNUI_CAPABILITY_VIRTUAL_MODEL: SynuiCapabilities = 1 << 1;
pub const SYNUI_CAPABILITIES: SynuiCapabilities =
    SYNUI_CAPABILITY_BASE | SYNUI_CAPABILITY_VIRTUAL_MODEL;

/// Status is an integer at the Rust boundary so untrusted C values can never
/// create an invalid Rust enum discriminant.
pub type SynuiStatus = u32;
pub const SYNUI_OK: SynuiStatus = 0;
pub const SYNUI_UNSUPPORTED_ABI: SynuiStatus = 1;
pub const SYNUI_UNSUPPORTED_CAPABILITY: SynuiStatus = 2;
pub const SYNUI_INVALID_ARGUMENT: SynuiStatus = 3;
pub const SYNUI_CONTEXT_MISMATCH: SynuiStatus = 4;
pub const SYNUI_CONTEXT_DESTROYED: SynuiStatus = 5;
pub const SYNUI_STALE_HANDLE: SynuiStatus = 6;
pub const SYNUI_THREAD_AFFINITY: SynuiStatus = 7;
pub const SYNUI_POST_CANCELLED: SynuiStatus = 8;
pub const SYNUI_CALLBACK_ERROR: SynuiStatus = 9;
pub const SYNUI_PANIC_CONTAINED: SynuiStatus = 10;
pub const SYNUI_INTERNAL_ERROR: SynuiStatus = 11;

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SynuiAbiVersion {
    pub major: u32,
    pub minor: u32,
}

pub const SYNUI_ABI_VERSION: SynuiAbiVersion = SynuiAbiVersion {
    major: SYNUI_ABI_MAJOR,
    minor: SYNUI_ABI_MINOR,
};

macro_rules! opaque_handle {
    ($name:ident) => {
        #[repr(C)]
        pub struct $name {
            _private: [u8; 0],
        }
    };
}

opaque_handle!(SynuiRuntime);
opaque_handle!(SynuiApp);
opaque_handle!(SynuiState);
opaque_handle!(SynuiWindow);
opaque_handle!(SynuiElement);
opaque_handle!(SynuiModel);
opaque_handle!(SynuiCallback);
opaque_handle!(SynuiPostToken);

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SynuiTextView {
    pub ptr: *const u8,
    pub len: usize,
}

impl SynuiTextView {
    const fn empty() -> Self {
        Self {
            ptr: ptr::null(),
            len: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SynuiBytesView {
    pub ptr: *const u8,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SynuiAbiInfo {
    pub struct_size: u32,
    pub version: SynuiAbiVersion,
    pub capabilities: SynuiCapabilities,
}

impl SynuiAbiInfo {
    pub const STRUCT_SIZE: u32 = mem::size_of::<Self>() as u32;
}

/// Required runtime creation options. Future fields may be appended.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SynuiRuntimeOptions {
    pub struct_size: u32,
    pub requested_version: SynuiAbiVersion,
    pub requested_capabilities: SynuiCapabilities,
}

impl SynuiRuntimeOptions {
    pub const STRUCT_SIZE: u32 = mem::size_of::<Self>() as u32;
}

/// Stable, read-only runtime handle information.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SynuiRuntimeInfo {
    pub struct_size: u32,
    pub version: SynuiAbiVersion,
    pub capabilities: SynuiCapabilities,
    pub resource_slot: u64,
    pub generation: u64,
    pub reference_count: u64,
}

impl SynuiRuntimeInfo {
    pub const STRUCT_SIZE: u32 = mem::size_of::<Self>() as u32;
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SynuiErrorRecord {
    pub struct_size: u32,
    pub status: SynuiStatus,
    pub code: u32,
    pub message: SynuiTextView,
    pub detail: SynuiTextView,
}

impl SynuiErrorRecord {
    pub const STRUCT_SIZE: u32 = mem::size_of::<Self>() as u32;
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SynuiBuffer {
    pub struct_size: u32,
    pub ptr: *mut u8,
    pub len: usize,
    pub capacity: usize,
}

impl SynuiBuffer {
    pub const STRUCT_SIZE: u32 = mem::size_of::<Self>() as u32;

    const fn empty() -> Self {
        Self {
            struct_size: Self::STRUCT_SIZE,
            ptr: ptr::null_mut(),
            len: 0,
            capacity: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SynuiCallbackPayload {
    pub struct_size: u32,
    pub kind: u32,
    pub bytes: SynuiBytesView,
}

pub type SynuiActionCallback = unsafe extern "C-unwind" fn(user_data: *mut c_void) -> SynuiStatus;

pub type SynuiPostCallback = unsafe extern "C-unwind" fn(
    user_data: *mut c_void,
    payload: *const SynuiCallbackPayload,
) -> SynuiStatus;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SynuiPost {
    pub struct_size: u32,
    pub callback: Option<SynuiPostCallback>,
    pub user_data: *mut c_void,
    pub payload: SynuiBytesView,
}

impl SynuiPost {
    pub const STRUCT_SIZE: u32 = mem::size_of::<Self>() as u32;
}

pub type SynuiPostState = u32;
pub const SYNUI_POST_ACCEPTED: SynuiPostState = 0;
pub const SYNUI_POST_RUNNING: SynuiPostState = 1;
pub const SYNUI_POST_EXECUTED: SynuiPostState = 2;
pub const SYNUI_POST_CANCELLED_STATE: SynuiPostState = 3;
pub const SYNUI_POST_FAILED: SynuiPostState = 4;

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SynuiPostTokenInfo {
    pub struct_size: u32,
    pub state: SynuiPostState,
    pub callback_status: SynuiStatus,
}

impl SynuiPostTokenInfo {
    pub const STRUCT_SIZE: u32 = mem::size_of::<Self>() as u32;
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SynuiTableDimensions {
    pub struct_size: u32,
    pub rows: u64,
    pub columns: u64,
}

impl SynuiTableDimensions {
    pub const STRUCT_SIZE: u32 = mem::size_of::<Self>() as u32;
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SynuiCellRange {
    pub struct_size: u32,
    pub row_start: u64,
    pub row_end: u64,
    pub column_start: u64,
    pub column_end: u64,
}

impl SynuiCellRange {
    pub const STRUCT_SIZE: u32 = mem::size_of::<Self>() as u32;
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SynuiCellRequest {
    pub struct_size: u32,
    pub request_id: u64,
    pub range: SynuiCellRange,
}

impl SynuiCellRequest {
    pub const STRUCT_SIZE: u32 = mem::size_of::<Self>() as u32;
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SynuiCellCoordinate {
    pub struct_size: u32,
    pub row: u64,
    pub column: u64,
}

impl SynuiCellCoordinate {
    pub const STRUCT_SIZE: u32 = mem::size_of::<Self>() as u32;
}

pub const SYNUI_CELL_EMPTY: u32 = 0;
pub const SYNUI_CELL_TEXT: u32 = 1;
pub const SYNUI_CELL_BOOL: u32 = 2;
pub const SYNUI_CELL_NUMBER: u32 = 3;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SynuiCellValue {
    pub struct_size: u32,
    pub kind: u32,
    pub text: SynuiTextView,
    pub boolean: u32,
    pub number: f64,
}

impl SynuiCellValue {
    pub const STRUCT_SIZE: u32 = mem::size_of::<Self>() as u32;
}

pub const SYNUI_SORT_ASCENDING: u32 = 0;
pub const SYNUI_SORT_DESCENDING: u32 = 1;

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SynuiSortDescriptor {
    pub struct_size: u32,
    pub column: u64,
    pub direction: u32,
}

impl SynuiSortDescriptor {
    pub const STRUCT_SIZE: u32 = mem::size_of::<Self>() as u32;
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SynuiPrefetchBand {
    pub struct_size: u32,
    pub rows_before: u64,
    pub rows_after: u64,
    pub columns_before: u64,
    pub columns_after: u64,
}

impl SynuiPrefetchBand {
    pub const STRUCT_SIZE: u32 = mem::size_of::<Self>() as u32;
}

pub type SynuiModelDimensionsCallback = unsafe extern "C-unwind" fn(
    user_data: *mut c_void,
    out_dimensions: *mut SynuiTableDimensions,
) -> SynuiStatus;
pub type SynuiModelRequestCellsCallback = unsafe extern "C-unwind" fn(
    user_data: *mut c_void,
    request: *const SynuiCellRequest,
) -> SynuiStatus;
pub type SynuiModelApplyCellUpdateCallback = unsafe extern "C-unwind" fn(
    user_data: *mut c_void,
    coordinate: *const SynuiCellCoordinate,
    value: *const SynuiCellValue,
) -> SynuiStatus;
pub type SynuiModelSortChangedCallback = unsafe extern "C-unwind" fn(
    user_data: *mut c_void,
    sort: *const SynuiSortDescriptor,
    sort_len: usize,
) -> SynuiStatus;
pub type SynuiModelSelectionChangedCallback = unsafe extern "C-unwind" fn(
    user_data: *mut c_void,
    ranges: *const SynuiCellRange,
    ranges_len: usize,
) -> SynuiStatus;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SynuiVirtualDataTableModelVtable {
    pub struct_size: u32,
    pub dimensions: Option<SynuiModelDimensionsCallback>,
    pub request_cells: Option<SynuiModelRequestCellsCallback>,
    pub apply_cell_update: Option<SynuiModelApplyCellUpdateCallback>,
    pub sort_changed: Option<SynuiModelSortChangedCallback>,
    pub selection_changed: Option<SynuiModelSelectionChangedCallback>,
}

impl SynuiVirtualDataTableModelVtable {
    pub const STRUCT_SIZE: u32 = mem::size_of::<Self>() as u32;
}

#[derive(Clone, Copy)]
struct AbiVirtualModel {
    dimensions: TableDimensions,
    vtable: SynuiVirtualDataTableModelVtable,
    user_data: usize,
}

impl FacadeModel for AbiVirtualModel {}

impl VirtualDataTableModel for AbiVirtualModel {
    fn dimensions(&self) -> TableDimensions {
        self.dimensions
    }

    fn request_cells(&mut self, request: CellRequest) -> Result<(), VirtualModelError> {
        let range = cell_range_to_abi(request.range);
        let request = SynuiCellRequest {
            struct_size: SynuiCellRequest::STRUCT_SIZE,
            request_id: request.id,
            range,
        };
        let status = unsafe {
            (self.vtable.request_cells.expect("validated model vtable"))(
                self.user_data as *mut c_void,
                &request,
            )
        };
        callback_model_result(status)
    }

    fn apply_cell_update(
        &mut self,
        coordinate: CellCoordinate,
        value: CellValue,
    ) -> Result<(), VirtualModelError> {
        let coordinate = SynuiCellCoordinate {
            struct_size: SynuiCellCoordinate::STRUCT_SIZE,
            row: coordinate.row as u64,
            column: coordinate.column as u64,
        };
        let (kind, text, boolean, number) = match &value {
            CellValue::Empty => (SYNUI_CELL_EMPTY, SynuiTextView::empty(), 0, 0.0),
            CellValue::Text(value) => (
                SYNUI_CELL_TEXT,
                SynuiTextView {
                    ptr: value.as_ptr(),
                    len: value.len(),
                },
                0,
                0.0,
            ),
            CellValue::Bool(value) => (
                SYNUI_CELL_BOOL,
                SynuiTextView::empty(),
                u32::from(*value),
                0.0,
            ),
            CellValue::Number(value) => (SYNUI_CELL_NUMBER, SynuiTextView::empty(), 0, *value),
        };
        let value = SynuiCellValue {
            struct_size: SynuiCellValue::STRUCT_SIZE,
            kind,
            text,
            boolean,
            number,
        };
        let status = unsafe {
            (self
                .vtable
                .apply_cell_update
                .expect("validated model vtable"))(
                self.user_data as *mut c_void,
                &coordinate,
                &value,
            )
        };
        callback_model_result(status)
    }

    fn sort_changed(&mut self, sort: &[SortDescriptor]) -> Result<(), VirtualModelError> {
        let sort: Vec<_> = sort
            .iter()
            .map(|item| SynuiSortDescriptor {
                struct_size: SynuiSortDescriptor::STRUCT_SIZE,
                column: item.column as u64,
                direction: match item.direction {
                    SortDirection::Ascending => SYNUI_SORT_ASCENDING,
                    SortDirection::Descending => SYNUI_SORT_DESCENDING,
                },
            })
            .collect();
        let status = unsafe {
            (self.vtable.sort_changed.expect("validated model vtable"))(
                self.user_data as *mut c_void,
                if sort.is_empty() {
                    ptr::null()
                } else {
                    sort.as_ptr()
                },
                sort.len(),
            )
        };
        callback_model_result(status)
    }

    fn selection_changed(&mut self, selection: &SelectionChange) -> Result<(), VirtualModelError> {
        let ranges: Vec<_> = selection
            .ranges
            .iter()
            .copied()
            .map(cell_range_to_abi)
            .collect();
        let status = unsafe {
            (self
                .vtable
                .selection_changed
                .expect("validated model vtable"))(
                self.user_data as *mut c_void,
                if ranges.is_empty() {
                    ptr::null()
                } else {
                    ranges.as_ptr()
                },
                ranges.len(),
            )
        };
        callback_model_result(status)
    }
}

fn callback_model_result(status: SynuiStatus) -> Result<(), VirtualModelError> {
    if status == SYNUI_OK {
        Ok(())
    } else {
        Err(VirtualModelError::App(synui::app::AppError::StaleHandle))
    }
}

fn cell_range_to_abi(range: CellRange) -> SynuiCellRange {
    SynuiCellRange {
        struct_size: SynuiCellRange::STRUCT_SIZE,
        row_start: range.row_start as u64,
        row_end: range.row_end as u64,
        column_start: range.column_start as u64,
        column_end: range.column_end as u64,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HandleKind {
    Runtime,
    App,
    State,
    Window,
    Element,
    Model,
    Callback,
    PostToken,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct ResourceKey {
    slot: u64,
    generation: u64,
}

#[derive(Clone, Copy, Debug)]
struct HandleLease {
    kind: HandleKind,
    key: ResourceKey,
}

/// A real allocation backs every pointer-shaped token. The allocation is never
/// dereferenced during lookup; its address is only a process-local capability.
#[repr(C)]
struct TokenAllocation {
    _marker: usize,
}

struct LiveHandle {
    lease: HandleLease,
    _allocation: Box<TokenAllocation>,
}

struct Tombstone {
    kind: HandleKind,
    // Retaining the allocation prevents an allocator from reusing the same raw
    // pointer for another lease while callers may still hold the old value.
    _allocation: Box<TokenAllocation>,
}

#[derive(Clone, Copy, Debug)]
struct ResourceRecord {
    kind: HandleKind,
    generation: u64,
    references: u64,
    version: SynuiAbiVersion,
    capabilities: SynuiCapabilities,
}

struct AppRecord {
    facade: FacadeApp,
    host: Option<LanguageHost>,
    rendered_elements: Vec<ResourceKey>,
    ui_thread: ThreadId,
    callback_depth: usize,
    queue: VecDeque<ResourceKey>,
}

struct StateRecord {
    app: ResourceKey,
    state: State<i64>,
}

struct CallbackRecord {
    app: ResourceKey,
    action: Action<()>,
    last_status: Arc<Mutex<SynuiStatus>>,
}

enum ElementData {
    Text {
        text: String,
        rendered: String,
    },
    Button {
        action: Action<()>,
        last_status: Arc<Mutex<SynuiStatus>>,
    },
}

struct ElementRecord {
    app: ResourceKey,
    data: ElementData,
}

struct ModelRecord {
    app: ResourceKey,
    context: AppContext,
    adapter: VirtualDataTableAdapter<AbiVirtualModel>,
}

struct PostRecord {
    app: ResourceKey,
    state: SynuiPostState,
    callback_status: SynuiStatus,
    callback: SynuiPostCallback,
    user_data: usize,
    payload: Vec<u8>,
}

struct HandleRegistry {
    next_slot: u64,
    free_slots: Vec<u64>,
    slot_generations: HashMap<u64, u64>,
    handles: HashMap<usize, LiveHandle>,
    tombstones: HashMap<usize, Tombstone>,
    resources: HashMap<u64, ResourceRecord>,
    apps: HashMap<ResourceKey, AppRecord>,
    states: HashMap<ResourceKey, StateRecord>,
    elements: HashMap<ResourceKey, ElementRecord>,
    callbacks: HashMap<ResourceKey, CallbackRecord>,
    models: HashMap<ResourceKey, ModelRecord>,
    posts: HashMap<ResourceKey, PostRecord>,
}

impl Default for HandleRegistry {
    fn default() -> Self {
        Self {
            next_slot: 1,
            free_slots: Vec::new(),
            slot_generations: HashMap::new(),
            handles: HashMap::new(),
            tombstones: HashMap::new(),
            resources: HashMap::new(),
            apps: HashMap::new(),
            states: HashMap::new(),
            elements: HashMap::new(),
            callbacks: HashMap::new(),
            models: HashMap::new(),
            posts: HashMap::new(),
        }
    }
}

const TOKEN_ALIGNMENT: usize = mem::align_of::<TokenAllocation>();

impl HandleRegistry {
    fn issue_token(&mut self, lease: HandleLease) -> Result<*mut c_void, SynuiStatus> {
        let mut allocation = Box::new(TokenAllocation { _marker: 0 });
        let token = ptr::from_mut(allocation.as_mut()).cast::<c_void>();
        let token_key = token as usize;
        if self.handles.contains_key(&token_key) || self.tombstones.contains_key(&token_key) {
            return Err(SYNUI_INTERNAL_ERROR);
        }
        self.handles.insert(
            token_key,
            LiveHandle {
                lease,
                _allocation: allocation,
            },
        );
        Ok(token)
    }

    fn create_resource(
        &mut self,
        kind: HandleKind,
        references: u64,
        version: SynuiAbiVersion,
        capabilities: SynuiCapabilities,
    ) -> Result<(ResourceKey, *mut c_void), SynuiStatus> {
        let slot = if let Some(slot) = self.free_slots.pop() {
            slot
        } else {
            let slot = self.next_slot;
            self.next_slot = self.next_slot.checked_add(1).ok_or(SYNUI_INTERNAL_ERROR)?;
            slot
        };
        let previous = self.slot_generations.get(&slot).copied().unwrap_or(0);
        let generation = previous.checked_add(1).ok_or(SYNUI_INTERNAL_ERROR)?;
        self.slot_generations.insert(slot, generation);
        let key = ResourceKey { slot, generation };
        self.resources.insert(
            slot,
            ResourceRecord {
                kind,
                generation,
                references,
                version,
                capabilities,
            },
        );
        match self.issue_token(HandleLease { kind, key }) {
            Ok(token) => Ok((key, token)),
            Err(status) => {
                self.resources.remove(&slot);
                self.free_slots.push(slot);
                Err(status)
            }
        }
    }

    fn create_runtime(
        &mut self,
        version: SynuiAbiVersion,
        capabilities: SynuiCapabilities,
    ) -> Result<*mut c_void, SynuiStatus> {
        self.create_resource(HandleKind::Runtime, 1, version, capabilities)
            .map(|(_, token)| token)
    }

    fn create_app(
        &mut self,
        version: SynuiAbiVersion,
        capabilities: SynuiCapabilities,
    ) -> Result<*mut c_void, SynuiStatus> {
        let facade = FacadeApp::new(AppConfig::default()).map_err(|_| SYNUI_INTERNAL_ERROR)?;
        let context = facade.context();
        let host = LanguageHost::new(context);
        let (key, token) = self.create_resource(HandleKind::App, 1, version, capabilities)?;
        self.apps.insert(
            key,
            AppRecord {
                facade,
                host: Some(host),
                rendered_elements: Vec::new(),
                ui_thread: thread::current().id(),
                callback_depth: 0,
                queue: VecDeque::new(),
            },
        );
        Ok(token)
    }

    fn app_resource(&self, app: ResourceKey) -> Result<ResourceRecord, SynuiStatus> {
        self.resources
            .get(&app.slot)
            .copied()
            .filter(|resource| {
                resource.kind == HandleKind::App && resource.generation == app.generation
            })
            .ok_or(SYNUI_CONTEXT_DESTROYED)
    }

    fn create_state_i64(
        &mut self,
        app: ResourceKey,
        initial: i64,
    ) -> Result<*mut c_void, SynuiStatus> {
        let resource = self.app_resource(app)?;
        let context = self
            .apps
            .get(&app)
            .ok_or(SYNUI_CONTEXT_DESTROYED)?
            .facade
            .context();
        let state = context.state(initial).map_err(|_| SYNUI_INTERNAL_ERROR)?;
        let (key, token) = self.create_resource(
            HandleKind::State,
            1,
            resource.version,
            resource.capabilities,
        )?;
        self.states.insert(key, StateRecord { app, state });
        Ok(token)
    }

    fn create_callback(
        &mut self,
        app: ResourceKey,
        callback: SynuiActionCallback,
        user_data: usize,
    ) -> Result<*mut c_void, SynuiStatus> {
        let resource = self.app_resource(app)?;
        let context = self
            .apps
            .get(&app)
            .ok_or(SYNUI_CONTEXT_DESTROYED)?
            .facade
            .context();
        let last_status = Arc::new(Mutex::new(SYNUI_OK));
        let callback_status = Arc::clone(&last_status);
        let action = context
            .action(move |_, (): ()| {
                let status = catch_unwind(AssertUnwindSafe(|| unsafe {
                    callback(user_data as *mut c_void)
                }))
                .unwrap_or(SYNUI_PANIC_CONTAINED);
                *callback_status
                    .lock()
                    .expect("callback status lock poisoned") = status;
            })
            .map_err(|_| SYNUI_INTERNAL_ERROR)?;
        let (key, token) = self.create_resource(
            HandleKind::Callback,
            1,
            resource.version,
            resource.capabilities,
        )?;
        self.callbacks.insert(
            key,
            CallbackRecord {
                app,
                action,
                last_status,
            },
        );
        Ok(token)
    }

    fn create_element(
        &mut self,
        app: ResourceKey,
        data: ElementData,
    ) -> Result<*mut c_void, SynuiStatus> {
        let resource = self.app_resource(app)?;
        let (key, token) = self.create_resource(
            HandleKind::Element,
            1,
            resource.version,
            resource.capabilities,
        )?;
        self.elements.insert(key, ElementRecord { app, data });
        Ok(token)
    }

    fn create_model(
        &mut self,
        app: ResourceKey,
        model: AbiVirtualModel,
    ) -> Result<*mut c_void, SynuiStatus> {
        let app_record = self.apps.get(&app).ok_or(SYNUI_CONTEXT_DESTROYED)?;
        let app_resource = self
            .resources
            .get(&app.slot)
            .copied()
            .ok_or(SYNUI_CONTEXT_DESTROYED)?;
        let context = app_record.facade.context();
        let handle = context.model(model).map_err(|_| SYNUI_INTERNAL_ERROR)?;
        let adapter = VirtualDataTableAdapter::new(handle);
        let (key, token) = self.create_resource(
            HandleKind::Model,
            1,
            app_resource.version,
            app_resource.capabilities,
        )?;
        self.models.insert(
            key,
            ModelRecord {
                app,
                context,
                adapter,
            },
        );
        Ok(token)
    }

    fn create_post(
        &mut self,
        app: ResourceKey,
        callback: SynuiPostCallback,
        user_data: usize,
        payload: Vec<u8>,
    ) -> Result<*mut c_void, SynuiStatus> {
        let app_record = self.apps.get_mut(&app).ok_or(SYNUI_CONTEXT_DESTROYED)?;
        app_record
            .queue
            .try_reserve(1)
            .map_err(|_| SYNUI_INTERNAL_ERROR)?;
        let (key, token) = self.create_resource(HandleKind::PostToken, 2, SYNUI_ABI_VERSION, 0)?;
        self.posts.insert(
            key,
            PostRecord {
                app,
                state: SYNUI_POST_ACCEPTED,
                callback_status: SYNUI_OK,
                callback,
                user_data,
                payload,
            },
        );
        self.apps
            .get_mut(&app)
            .ok_or(SYNUI_CONTEXT_DESTROYED)?
            .queue
            .push_back(key);
        Ok(token)
    }

    fn validate(
        &self,
        token: usize,
        expected: HandleKind,
    ) -> Result<(HandleLease, ResourceRecord), SynuiStatus> {
        if let Some(handle) = self.handles.get(&token) {
            let lease = handle.lease;
            if lease.kind != expected {
                return Err(SYNUI_INVALID_ARGUMENT);
            }
            let resource = self
                .resources
                .get(&lease.key.slot)
                .copied()
                .ok_or(SYNUI_STALE_HANDLE)?;
            if resource.kind != expected || resource.generation != lease.key.generation {
                return Err(SYNUI_STALE_HANDLE);
            }
            return Ok((lease, resource));
        }
        if let Some(tombstone) = self.tombstones.get(&token) {
            return if tombstone.kind == expected {
                Err(SYNUI_STALE_HANDLE)
            } else {
                Err(SYNUI_INVALID_ARGUMENT)
            };
        }
        Err(SYNUI_STALE_HANDLE)
    }

    fn retain(&mut self, token: usize, expected: HandleKind) -> Result<*mut c_void, SynuiStatus> {
        let (lease, resource) = self.validate(token, expected)?;
        let references = resource
            .references
            .checked_add(1)
            .ok_or(SYNUI_INTERNAL_ERROR)?;
        let new_token = self.issue_token(lease)?;
        self.resources
            .get_mut(&lease.key.slot)
            .ok_or(SYNUI_STALE_HANDLE)?
            .references = references;
        Ok(new_token)
    }

    fn consume_token(&mut self, token: usize, lease: HandleLease) -> SynuiStatus {
        let Some(handle) = self.handles.remove(&token) else {
            return SYNUI_STALE_HANDLE;
        };
        self.tombstones.insert(
            token,
            Tombstone {
                kind: lease.kind,
                _allocation: handle._allocation,
            },
        );
        self.decrement_resource(lease.key)
    }

    fn decrement_resource(&mut self, key: ResourceKey) -> SynuiStatus {
        let Some(resource) = self.resources.get_mut(&key.slot) else {
            return SYNUI_STALE_HANDLE;
        };
        if resource.generation != key.generation || resource.references == 0 {
            return SYNUI_STALE_HANDLE;
        }
        resource.references -= 1;
        if resource.references == 0 {
            let kind = resource.kind;
            self.resources.remove(&key.slot);
            if kind == HandleKind::App {
                self.apps.remove(&key);
            } else if kind == HandleKind::State {
                self.states.remove(&key);
            } else if kind == HandleKind::Element {
                self.elements.remove(&key);
            } else if kind == HandleKind::Callback {
                self.callbacks.remove(&key);
            } else if kind == HandleKind::Model {
                self.models.remove(&key);
            } else if kind == HandleKind::PostToken {
                self.posts.remove(&key);
            }
            self.free_slots.push(key.slot);
        }
        SYNUI_OK
    }

    fn release_app(&mut self, token: usize) -> SynuiStatus {
        let (lease, resource) = match self.validate(token, HandleKind::App) {
            Ok(value) => value,
            Err(status) => return status,
        };
        if resource.references == 1 {
            let Some(app) = self.apps.get(&lease.key) else {
                return SYNUI_CONTEXT_DESTROYED;
            };
            if app.callback_depth != 0 {
                return SYNUI_CALLBACK_ERROR;
            }
            let queued = mem::take(
                &mut self
                    .apps
                    .get_mut(&lease.key)
                    .expect("validated app missing")
                    .queue,
            );
            for post_key in queued {
                if let Some(post) = self.posts.get_mut(&post_key) {
                    if post.state == SYNUI_POST_ACCEPTED {
                        post.state = SYNUI_POST_CANCELLED_STATE;
                        post.callback_status = SYNUI_POST_CANCELLED;
                    }
                }
                let _ = self.decrement_resource(post_key);
            }
            self.apps.remove(&lease.key);
        }
        self.consume_token(token, lease)
    }

    fn release(&mut self, token: usize, expected: HandleKind) -> SynuiStatus {
        if expected == HandleKind::App {
            return self.release_app(token);
        }
        let (lease, _) = match self.validate(token, expected) {
            Ok(value) => value,
            Err(status) => return status,
        };
        self.consume_token(token, lease)
    }

    fn cancel_post(&mut self, token: usize) -> SynuiStatus {
        let (lease, _) = match self.validate(token, HandleKind::PostToken) {
            Ok(value) => value,
            Err(status) => return status,
        };
        let Some(post) = self.posts.get_mut(&lease.key) else {
            return SYNUI_STALE_HANDLE;
        };
        if post.state != SYNUI_POST_ACCEPTED {
            return if post.state == SYNUI_POST_CANCELLED_STATE {
                SYNUI_POST_CANCELLED
            } else {
                SYNUI_CALLBACK_ERROR
            };
        }
        let app_key = post.app;
        post.state = SYNUI_POST_CANCELLED_STATE;
        post.callback_status = SYNUI_POST_CANCELLED;
        if let Some(app) = self.apps.get_mut(&app_key) {
            app.queue.retain(|key| *key != lease.key);
        }
        let _ = self.decrement_resource(lease.key);
        SYNUI_OK
    }
}

static HANDLES: OnceLock<Mutex<HandleRegistry>> = OnceLock::new();

fn handles() -> MutexGuard<'static, HandleRegistry> {
    HANDLES
        .get_or_init(|| Mutex::new(HandleRegistry::default()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[repr(C)]
struct OwnedErrorRecord {
    public: SynuiErrorRecord,
    message: Box<[u8]>,
    detail: Box<[u8]>,
}

// The raw views point only into the two owned immutable byte boxes. The whole
// allocation moves between threads only while protected by ERROR_RECORDS.
unsafe impl Send for OwnedErrorRecord {}

static ERROR_RECORDS: OnceLock<Mutex<HashMap<usize, Box<OwnedErrorRecord>>>> = OnceLock::new();

fn error_records() -> MutexGuard<'static, HashMap<usize, Box<OwnedErrorRecord>>> {
    ERROR_RECORDS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

static ABI_BUFFERS: OnceLock<Mutex<HashMap<usize, Box<[u8]>>>> = OnceLock::new();

fn abi_buffers() -> MutexGuard<'static, HashMap<usize, Box<[u8]>>> {
    ABI_BUFFERS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn allocate_abi_buffer(bytes: &[u8]) -> SynuiBuffer {
    if bytes.is_empty() {
        return SynuiBuffer::empty();
    }
    let mut owned: Box<[u8]> = bytes.into();
    let pointer = owned.as_mut_ptr();
    let len = owned.len();
    abi_buffers().insert(pointer as usize, owned);
    SynuiBuffer {
        struct_size: SynuiBuffer::STRUCT_SIZE,
        ptr: pointer,
        len,
        capacity: len,
    }
}

fn free_abi_buffer(buffer: SynuiBuffer) {
    if !buffer.ptr.is_null() {
        abi_buffers().remove(&(buffer.ptr as usize));
    }
}

fn status_message(status: SynuiStatus) -> &'static str {
    match status {
        SYNUI_OK => "",
        SYNUI_UNSUPPORTED_ABI => "unsupported ABI version",
        SYNUI_UNSUPPORTED_CAPABILITY => "unsupported ABI capability",
        SYNUI_INVALID_ARGUMENT => "invalid argument",
        SYNUI_CONTEXT_MISMATCH => "context mismatch",
        SYNUI_CONTEXT_DESTROYED => "context destroyed",
        SYNUI_STALE_HANDLE => "stale or released handle",
        SYNUI_THREAD_AFFINITY => "UI thread affinity violation",
        SYNUI_POST_CANCELLED => "post operation cancelled",
        SYNUI_CALLBACK_ERROR => "callback failed",
        SYNUI_PANIC_CONTAINED => "panic contained",
        _ => "internal ABI error",
    }
}

fn allocate_error_record(status: SynuiStatus, detail: &str) -> *mut SynuiErrorRecord {
    let message: Box<[u8]> = status_message(status).as_bytes().into();
    let detail = if status == SYNUI_OK { "" } else { detail };
    let detail: Box<[u8]> = detail.as_bytes().into();
    let mut owned = Box::new(OwnedErrorRecord {
        public: SynuiErrorRecord {
            struct_size: SynuiErrorRecord::STRUCT_SIZE,
            status,
            code: status,
            message: SynuiTextView {
                ptr: ptr::null(),
                len: 0,
            },
            detail: SynuiTextView {
                ptr: ptr::null(),
                len: 0,
            },
        },
        message,
        detail,
    });
    if !owned.message.is_empty() {
        owned.public.message = SynuiTextView {
            ptr: owned.message.as_ptr(),
            len: owned.message.len(),
        };
    }
    if !owned.detail.is_empty() {
        owned.public.detail = SynuiTextView {
            ptr: owned.detail.as_ptr(),
            len: owned.detail.len(),
        };
    }
    let record = ptr::addr_of_mut!(owned.public);
    error_records().insert(record as usize, owned);
    record
}

struct StaticErrorRecord(SynuiErrorRecord);

// The fallback is immutable process-lifetime storage. Freeing it is a no-op.
unsafe impl Sync for StaticErrorRecord {}

static INTERNAL_MESSAGE: &[u8] = b"internal ABI error";
static INTERNAL_DETAIL: &[u8] = b"error record fallback";
static INTERNAL_ERROR_FALLBACK: StaticErrorRecord = StaticErrorRecord(SynuiErrorRecord {
    struct_size: SynuiErrorRecord::STRUCT_SIZE,
    status: SYNUI_INTERNAL_ERROR,
    code: SYNUI_INTERNAL_ERROR,
    message: SynuiTextView {
        ptr: INTERNAL_MESSAGE.as_ptr(),
        len: INTERNAL_MESSAGE.len(),
    },
    detail: SynuiTextView {
        ptr: INTERNAL_DETAIL.as_ptr(),
        len: INTERNAL_DETAIL.len(),
    },
});

fn fallback_error_record() -> *mut SynuiErrorRecord {
    ptr::from_ref(&INTERNAL_ERROR_FALLBACK.0).cast_mut()
}

unsafe fn complete(
    status: SynuiStatus,
    detail: &str,
    out_error: *mut *mut SynuiErrorRecord,
) -> SynuiStatus {
    if out_error.is_null() {
        return status;
    }
    match catch_unwind(AssertUnwindSafe(|| allocate_error_record(status, detail))) {
        Ok(record) => {
            unsafe { ptr::write(out_error, record) };
            status
        }
        Err(_) => {
            unsafe { ptr::write(out_error, fallback_error_record()) };
            SYNUI_INTERNAL_ERROR
        }
    }
}

fn ffi_guard(
    out_error: *mut *mut SynuiErrorRecord,
    operation: impl FnOnce() -> SynuiStatus,
) -> SynuiStatus {
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(status) => status,
        Err(_) if valid_out_error(out_error) => unsafe {
            complete(SYNUI_PANIC_CONTAINED, "panic at ABI boundary", out_error)
        },
        // Never write a fallback record through an invalid caller address.
        Err(_) => SYNUI_PANIC_CONTAINED,
    }
}

fn ffi_guard_void(operation: impl FnOnce()) {
    let _ = catch_unwind(AssertUnwindSafe(operation));
}

fn aligned<T>(pointer: *const T) -> bool {
    (pointer as usize).is_multiple_of(mem::align_of::<T>())
}

fn valid_out_error(out_error: *mut *mut SynuiErrorRecord) -> bool {
    out_error.is_null() || aligned(out_error)
}

unsafe fn string_from_text_view(view: SynuiTextView) -> Result<String, SynuiStatus> {
    if (view.ptr.is_null() && view.len != 0) || view.len > isize::MAX as usize {
        return Err(SYNUI_INVALID_ARGUMENT);
    }
    let bytes = if view.len == 0 {
        &[][..]
    } else {
        unsafe { core::slice::from_raw_parts(view.ptr, view.len) }
    };
    core::str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|_| SYNUI_INVALID_ARGUMENT)
}

fn output_ranges_overlap<T, U>(first: *const T, second: *const U) -> bool {
    if first.is_null() || second.is_null() {
        return false;
    }
    let first_start = first as usize;
    let second_start = second as usize;
    let Some(first_end) = first_start.checked_add(mem::size_of::<T>()) else {
        return true;
    };
    let Some(second_end) = second_start.checked_add(mem::size_of::<U>()) else {
        return true;
    };
    first_start < second_end && second_start < first_end
}

fn error_output_overlaps<T>(output: *const T, out_error: *mut *mut SynuiErrorRecord) -> bool {
    output_ranges_overlap(output, out_error)
}

fn valid_token(pointer: *const c_void) -> Result<usize, SynuiStatus> {
    if pointer.is_null() {
        return Err(SYNUI_INVALID_ARGUMENT);
    }
    let token = pointer as usize;
    if !token.is_multiple_of(TOKEN_ALIGNMENT) {
        return Err(SYNUI_INVALID_ARGUMENT);
    }
    Ok(token)
}

unsafe fn read_struct_size<T>(value: *const T) -> Result<u32, SynuiStatus> {
    if value.is_null() || !aligned(value) {
        return Err(SYNUI_INVALID_ARGUMENT);
    }
    Ok(unsafe { value.cast::<u32>().read() })
}

fn negotiate(requested: SynuiAbiVersion, capabilities: SynuiCapabilities) -> SynuiStatus {
    if requested.major != SYNUI_ABI_MAJOR || requested.minor > SYNUI_ABI_MINOR {
        SYNUI_UNSUPPORTED_ABI
    } else if capabilities & !SYNUI_CAPABILITIES != 0 {
        SYNUI_UNSUPPORTED_CAPABILITY
    } else {
        SYNUI_OK
    }
}

/// Frees an ABI-owned record. NULL and unknown pointers are ignored without
/// dereferencing them. Passing a record more than once remains caller misuse.
///
/// # Safety
/// `record` must be NULL or a record returned by this loaded ABI instance that
/// the caller has not already freed.
unsafe fn synui_error_record_free_impl(record: *mut SynuiErrorRecord) {
    if record.is_null() || !aligned(record) || record == fallback_error_record() {
        return;
    }
    error_records().remove(&(record as usize));
}

/// Negotiates production ABI v1. When `out_error` is non-NULL, every result
/// owns one matching Error Record, including successful results.
///
/// # Safety
/// Non-NULL outputs must point to aligned writable storage for their declared
/// types; `out_info` must have its `struct_size` initialized as documented.
unsafe fn synui_abi_query_impl(
    requested: SynuiAbiVersion,
    requested_capabilities: SynuiCapabilities,
    out_info: *mut SynuiAbiInfo,
    out_error: *mut *mut SynuiErrorRecord,
) -> SynuiStatus {
    if !valid_out_error(out_error) {
        return SYNUI_INVALID_ARGUMENT;
    }
    if error_output_overlaps(out_info, out_error) {
        return SYNUI_INVALID_ARGUMENT;
    }
    let output_size = match unsafe { read_struct_size(out_info) } {
        Ok(size) if size >= SynuiAbiInfo::STRUCT_SIZE => size,
        _ => return unsafe { complete(SYNUI_INVALID_ARGUMENT, "invalid out_info", out_error) },
    };
    let _ = output_size;
    let status = negotiate(requested, requested_capabilities);
    if status == SYNUI_OK {
        unsafe {
            ptr::write(
                out_info,
                SynuiAbiInfo {
                    struct_size: SynuiAbiInfo::STRUCT_SIZE,
                    version: SYNUI_ABI_VERSION,
                    capabilities: SYNUI_CAPABILITIES,
                },
            )
        };
    }
    unsafe { complete(status, "ABI negotiation", out_error) }
}

/// Creates a runtime after validating options and all output addresses.
///
/// # Safety
/// `options` must be readable through its declared `struct_size`; non-NULL
/// outputs must point to aligned writable storage for their declared types.
unsafe fn synui_runtime_create_impl(
    options: *const SynuiRuntimeOptions,
    out_runtime: *mut *mut SynuiRuntime,
    out_error: *mut *mut SynuiErrorRecord,
) -> SynuiStatus {
    if !valid_out_error(out_error) {
        return SYNUI_INVALID_ARGUMENT;
    }
    if out_runtime.is_null() || !aligned(out_runtime) {
        return unsafe { complete(SYNUI_INVALID_ARGUMENT, "invalid out_runtime", out_error) };
    }
    if error_output_overlaps(out_runtime, out_error) {
        return SYNUI_INVALID_ARGUMENT;
    }
    let options_size = match unsafe { read_struct_size(options) } {
        Ok(size) if size >= SynuiRuntimeOptions::STRUCT_SIZE => size,
        _ => {
            return unsafe {
                complete(SYNUI_INVALID_ARGUMENT, "invalid runtime options", out_error)
            };
        }
    };
    let _ = options_size;
    let options = unsafe { ptr::read(options) };
    let status = negotiate(options.requested_version, options.requested_capabilities);
    if status != SYNUI_OK {
        return unsafe { complete(status, "runtime negotiation", out_error) };
    }
    let token =
        match handles().create_runtime(options.requested_version, options.requested_capabilities) {
            Ok(token) => token,
            Err(status) => return unsafe { complete(status, "runtime allocation", out_error) },
        };
    unsafe { ptr::write(out_runtime, token.cast::<SynuiRuntime>()) };
    unsafe { complete(SYNUI_OK, "", out_error) }
}

/// Returns stable metadata for a live runtime lease.
///
/// # Safety
/// `runtime` must be a token passed back unchanged from this ABI instance;
/// non-NULL outputs must be aligned writable storage and `out_info` must have
/// its `struct_size` initialized as documented.
unsafe fn synui_runtime_query_impl(
    runtime: *const SynuiRuntime,
    out_info: *mut SynuiRuntimeInfo,
    out_error: *mut *mut SynuiErrorRecord,
) -> SynuiStatus {
    if !valid_out_error(out_error) {
        return SYNUI_INVALID_ARGUMENT;
    }
    if error_output_overlaps(out_info, out_error) {
        return SYNUI_INVALID_ARGUMENT;
    }
    let output_size = match unsafe { read_struct_size(out_info) } {
        Ok(size) if size >= SynuiRuntimeInfo::STRUCT_SIZE => size,
        _ => {
            return unsafe { complete(SYNUI_INVALID_ARGUMENT, "invalid runtime info", out_error) };
        }
    };
    let _ = output_size;
    let token = match valid_token(runtime.cast()) {
        Ok(token) => token,
        Err(status) => return unsafe { complete(status, "invalid runtime handle", out_error) },
    };
    let (lease, resource) = match handles().validate(token, HandleKind::Runtime) {
        Ok(value) => value,
        Err(status) => return unsafe { complete(status, "runtime query", out_error) },
    };
    unsafe {
        ptr::write(
            out_info,
            SynuiRuntimeInfo {
                struct_size: SynuiRuntimeInfo::STRUCT_SIZE,
                version: resource.version,
                capabilities: resource.capabilities,
                resource_slot: lease.key.slot,
                generation: lease.key.generation,
                reference_count: resource.references,
            },
        )
    };
    unsafe { complete(SYNUI_OK, "", out_error) }
}

#[no_mangle]
pub unsafe extern "C" fn synui_error_record_free(record: *mut SynuiErrorRecord) {
    ffi_guard_void(|| unsafe { synui_error_record_free_impl(record) });
}

#[no_mangle]
pub extern "C" fn synui_buffer_free(buffer: SynuiBuffer) {
    ffi_guard_void(|| free_abi_buffer(buffer));
}

#[no_mangle]
pub unsafe extern "C" fn synui_abi_query(
    requested: SynuiAbiVersion,
    requested_capabilities: SynuiCapabilities,
    out_info: *mut SynuiAbiInfo,
    out_error: *mut *mut SynuiErrorRecord,
) -> SynuiStatus {
    ffi_guard(out_error, || unsafe {
        synui_abi_query_impl(requested, requested_capabilities, out_info, out_error)
    })
}

#[no_mangle]
pub unsafe extern "C" fn synui_runtime_create(
    options: *const SynuiRuntimeOptions,
    out_runtime: *mut *mut SynuiRuntime,
    out_error: *mut *mut SynuiErrorRecord,
) -> SynuiStatus {
    ffi_guard(out_error, || unsafe {
        synui_runtime_create_impl(options, out_runtime, out_error)
    })
}

#[no_mangle]
pub unsafe extern "C" fn synui_runtime_query(
    runtime: *const SynuiRuntime,
    out_info: *mut SynuiRuntimeInfo,
    out_error: *mut *mut SynuiErrorRecord,
) -> SynuiStatus {
    ffi_guard(out_error, || unsafe {
        synui_runtime_query_impl(runtime, out_info, out_error)
    })
}

unsafe fn retain_impl<T>(
    handle: *const T,
    out_retained: *mut *mut T,
    out_error: *mut *mut SynuiErrorRecord,
    kind: HandleKind,
) -> SynuiStatus {
    if !valid_out_error(out_error) {
        return SYNUI_INVALID_ARGUMENT;
    }
    if out_retained.is_null() || !aligned(out_retained) {
        return unsafe {
            complete(
                SYNUI_INVALID_ARGUMENT,
                "invalid retained-handle output",
                out_error,
            )
        };
    }
    if error_output_overlaps(out_retained, out_error) {
        return SYNUI_INVALID_ARGUMENT;
    }
    let token = match valid_token(handle.cast()) {
        Ok(token) => token,
        Err(status) => return unsafe { complete(status, "invalid handle token", out_error) },
    };
    let retained = match handles().retain(token, kind) {
        Ok(retained) => retained,
        Err(status) => return unsafe { complete(status, "handle retain", out_error) },
    };
    unsafe { ptr::write(out_retained, retained.cast::<T>()) };
    unsafe { complete(SYNUI_OK, "", out_error) }
}

unsafe fn release_impl<T>(
    handle: *mut T,
    out_error: *mut *mut SynuiErrorRecord,
    kind: HandleKind,
) -> SynuiStatus {
    if !valid_out_error(out_error) {
        return SYNUI_INVALID_ARGUMENT;
    }
    if handle.is_null() {
        return unsafe { complete(SYNUI_OK, "", out_error) };
    }
    let token = match valid_token(handle.cast()) {
        Ok(token) => token,
        Err(status) => return unsafe { complete(status, "invalid handle token", out_error) },
    };
    let status = handles().release(token, kind);
    unsafe {
        complete(
            status,
            if status == SYNUI_OK {
                ""
            } else {
                "handle release"
            },
            out_error,
        )
    }
}

macro_rules! handle_ownership_api {
    ($retain:ident, $release:ident, $ty:ty, $kind:expr) => {
        #[no_mangle]
        pub unsafe extern "C" fn $retain(
            handle: *const $ty,
            out_retained: *mut *mut $ty,
            out_error: *mut *mut SynuiErrorRecord,
        ) -> SynuiStatus {
            ffi_guard(out_error, || unsafe {
                retain_impl(handle, out_retained, out_error, $kind)
            })
        }

        #[no_mangle]
        pub unsafe extern "C" fn $release(
            handle: *mut $ty,
            out_error: *mut *mut SynuiErrorRecord,
        ) -> SynuiStatus {
            ffi_guard(out_error, || unsafe {
                release_impl(handle, out_error, $kind)
            })
        }
    };
}

handle_ownership_api!(
    synui_runtime_retain,
    synui_runtime_release,
    SynuiRuntime,
    HandleKind::Runtime
);
handle_ownership_api!(
    synui_app_retain,
    synui_app_release,
    SynuiApp,
    HandleKind::App
);
handle_ownership_api!(
    synui_state_retain,
    synui_state_release,
    SynuiState,
    HandleKind::State
);
handle_ownership_api!(
    synui_window_retain,
    synui_window_release,
    SynuiWindow,
    HandleKind::Window
);
handle_ownership_api!(
    synui_element_retain,
    synui_element_release,
    SynuiElement,
    HandleKind::Element
);
handle_ownership_api!(
    synui_model_retain,
    synui_model_release,
    SynuiModel,
    HandleKind::Model
);
handle_ownership_api!(
    synui_callback_retain,
    synui_callback_release,
    SynuiCallback,
    HandleKind::Callback
);
handle_ownership_api!(
    synui_post_token_retain,
    synui_post_token_release,
    SynuiPostToken,
    HandleKind::PostToken
);

unsafe fn app_create_impl(
    runtime: *const SynuiRuntime,
    out_app: *mut *mut SynuiApp,
    out_error: *mut *mut SynuiErrorRecord,
) -> SynuiStatus {
    if !valid_out_error(out_error) {
        return SYNUI_INVALID_ARGUMENT;
    }
    if out_app.is_null() || !aligned(out_app) {
        return unsafe { complete(SYNUI_INVALID_ARGUMENT, "invalid out_app", out_error) };
    }
    if error_output_overlaps(out_app, out_error) {
        return SYNUI_INVALID_ARGUMENT;
    }
    let runtime_token = match valid_token(runtime.cast()) {
        Ok(token) => token,
        Err(status) => return unsafe { complete(status, "invalid runtime handle", out_error) },
    };
    let token = {
        let mut registry = handles();
        let (_, runtime_resource) = match registry.validate(runtime_token, HandleKind::Runtime) {
            Ok(value) => value,
            Err(status) => return unsafe { complete(status, "app runtime", out_error) },
        };
        match registry.create_app(runtime_resource.version, runtime_resource.capabilities) {
            Ok(token) => token,
            Err(status) => return unsafe { complete(status, "app allocation", out_error) },
        }
    };
    unsafe { ptr::write(out_app, token.cast::<SynuiApp>()) };
    unsafe { complete(SYNUI_OK, "", out_error) }
}

#[no_mangle]
pub unsafe extern "C" fn synui_app_create(
    runtime: *const SynuiRuntime,
    out_app: *mut *mut SynuiApp,
    out_error: *mut *mut SynuiErrorRecord,
) -> SynuiStatus {
    ffi_guard(out_error, || unsafe {
        app_create_impl(runtime, out_app, out_error)
    })
}

fn ensure_app_ui_thread(registry: &HandleRegistry, app: ResourceKey) -> Result<(), SynuiStatus> {
    let app = registry.apps.get(&app).ok_or(SYNUI_CONTEXT_DESTROYED)?;
    if app.ui_thread == thread::current().id() {
        Ok(())
    } else {
        Err(SYNUI_THREAD_AFFINITY)
    }
}

unsafe fn state_i64_create_impl(
    app: *const SynuiApp,
    initial: i64,
    out_state: *mut *mut SynuiState,
    out_error: *mut *mut SynuiErrorRecord,
) -> SynuiStatus {
    if !valid_out_error(out_error) {
        return SYNUI_INVALID_ARGUMENT;
    }
    if out_state.is_null() || !aligned(out_state) {
        return unsafe { complete(SYNUI_INVALID_ARGUMENT, "invalid out_state", out_error) };
    }
    let app_token = match valid_token(app.cast()) {
        Ok(token) => token,
        Err(status) => return unsafe { complete(status, "invalid App", out_error) },
    };
    let token = {
        let mut registry = handles();
        let (lease, _) = match registry.validate(app_token, HandleKind::App) {
            Ok(value) => value,
            Err(status) => return unsafe { complete(status, "state App", out_error) },
        };
        if let Err(status) = ensure_app_ui_thread(&registry, lease.key) {
            return unsafe { complete(status, "state creation thread", out_error) };
        }
        match registry.create_state_i64(lease.key, initial) {
            Ok(token) => token,
            Err(status) => return unsafe { complete(status, "state allocation", out_error) },
        }
    };
    unsafe { ptr::write(out_state, token.cast::<SynuiState>()) };
    unsafe { complete(SYNUI_OK, "", out_error) }
}

#[no_mangle]
pub unsafe extern "C" fn synui_state_i64_create(
    app: *const SynuiApp,
    initial: i64,
    out_state: *mut *mut SynuiState,
    out_error: *mut *mut SynuiErrorRecord,
) -> SynuiStatus {
    ffi_guard(out_error, || unsafe {
        state_i64_create_impl(app, initial, out_state, out_error)
    })
}

unsafe fn state_i64_access_impl(
    state: *const SynuiState,
    value: *mut i64,
    write: bool,
    out_error: *mut *mut SynuiErrorRecord,
) -> SynuiStatus {
    if !valid_out_error(out_error) || value.is_null() || !aligned(value) {
        return unsafe { complete(SYNUI_INVALID_ARGUMENT, "invalid state value", out_error) };
    }
    let token = match valid_token(state.cast()) {
        Ok(token) => token,
        Err(status) => return unsafe { complete(status, "invalid state", out_error) },
    };
    let state = {
        let registry = handles();
        let (lease, _) = match registry.validate(token, HandleKind::State) {
            Ok(value) => value,
            Err(status) => return unsafe { complete(status, "state access", out_error) },
        };
        let Some(record) = registry.states.get(&lease.key) else {
            return unsafe { complete(SYNUI_STALE_HANDLE, "state access", out_error) };
        };
        if let Err(status) = ensure_app_ui_thread(&registry, record.app) {
            return unsafe { complete(status, "state access thread", out_error) };
        }
        record.state.clone()
    };
    let result = if write {
        state.set(unsafe { ptr::read(value) }).map(|_| 0)
    } else {
        state.get()
    };
    match result {
        Ok(current) => {
            if !write {
                unsafe { ptr::write(value, current) };
            }
            unsafe { complete(SYNUI_OK, "", out_error) }
        }
        Err(error) => unsafe {
            complete(
                match error {
                    synui::app::AppError::ContextMismatch => SYNUI_CONTEXT_MISMATCH,
                    synui::app::AppError::ContextDestroyed => SYNUI_CONTEXT_DESTROYED,
                    synui::app::AppError::StaleHandle => SYNUI_STALE_HANDLE,
                    synui::app::AppError::ThreadAffinity => SYNUI_THREAD_AFFINITY,
                },
                "state access failed",
                out_error,
            )
        },
    }
}

#[no_mangle]
pub unsafe extern "C" fn synui_state_i64_get(
    state: *const SynuiState,
    out_value: *mut i64,
    out_error: *mut *mut SynuiErrorRecord,
) -> SynuiStatus {
    ffi_guard(out_error, || unsafe {
        state_i64_access_impl(state, out_value, false, out_error)
    })
}

#[no_mangle]
pub unsafe extern "C" fn synui_state_i64_set(
    state: *const SynuiState,
    value: i64,
    out_error: *mut *mut SynuiErrorRecord,
) -> SynuiStatus {
    ffi_guard(out_error, || unsafe {
        let mut value = value;
        state_i64_access_impl(state, &mut value, true, out_error)
    })
}

#[no_mangle]
pub unsafe extern "C" fn synui_callback_create(
    app: *const SynuiApp,
    callback: Option<SynuiActionCallback>,
    user_data: *mut c_void,
    out_callback: *mut *mut SynuiCallback,
    out_error: *mut *mut SynuiErrorRecord,
) -> SynuiStatus {
    ffi_guard(out_error, || unsafe {
        if !valid_out_error(out_error) || out_callback.is_null() || !aligned(out_callback) {
            return complete(SYNUI_INVALID_ARGUMENT, "invalid callback output", out_error);
        }
        let Some(callback) = callback else {
            return complete(SYNUI_INVALID_ARGUMENT, "NULL callback", out_error);
        };
        let app_token = match valid_token(app.cast()) {
            Ok(token) => token,
            Err(status) => return complete(status, "invalid App", out_error),
        };
        let token = {
            let mut registry = handles();
            let (lease, _) = match registry.validate(app_token, HandleKind::App) {
                Ok(value) => value,
                Err(status) => return complete(status, "callback App", out_error),
            };
            if let Err(status) = ensure_app_ui_thread(&registry, lease.key) {
                return complete(status, "callback creation thread", out_error);
            }
            match registry.create_callback(lease.key, callback, user_data as usize) {
                Ok(token) => token,
                Err(status) => return complete(status, "callback allocation", out_error),
            }
        };
        ptr::write(out_callback, token.cast::<SynuiCallback>());
        complete(SYNUI_OK, "", out_error)
    })
}

#[no_mangle]
pub unsafe extern "C" fn synui_text_element_create(
    app: *const SynuiApp,
    text: SynuiTextView,
    out_element: *mut *mut SynuiElement,
    out_error: *mut *mut SynuiErrorRecord,
) -> SynuiStatus {
    ffi_guard(out_error, || unsafe {
        if !valid_out_error(out_error)
            || out_element.is_null()
            || !aligned(out_element)
            || error_output_overlaps(out_element, out_error)
        {
            return complete(SYNUI_INVALID_ARGUMENT, "invalid element output", out_error);
        }
        let text = match string_from_text_view(text) {
            Ok(text) => text,
            Err(status) => return complete(status, "invalid UTF-8 text", out_error),
        };
        let app_token = match valid_token(app.cast()) {
            Ok(token) => token,
            Err(status) => return complete(status, "invalid App", out_error),
        };
        let token = {
            let mut registry = handles();
            let (app_lease, _) = match registry.validate(app_token, HandleKind::App) {
                Ok(value) => value,
                Err(status) => return complete(status, "element App", out_error),
            };
            if let Err(status) = ensure_app_ui_thread(&registry, app_lease.key) {
                return complete(status, "element creation thread", out_error);
            }
            match registry.create_element(
                app_lease.key,
                ElementData::Text {
                    text,
                    rendered: String::new(),
                },
            ) {
                Ok(token) => token,
                Err(status) => return complete(status, "element allocation", out_error),
            }
        };
        ptr::write(out_element, token.cast::<SynuiElement>());
        complete(SYNUI_OK, "", out_error)
    })
}

#[no_mangle]
pub unsafe extern "C" fn synui_text_element_set_text(
    element: *const SynuiElement,
    text: SynuiTextView,
    out_error: *mut *mut SynuiErrorRecord,
) -> SynuiStatus {
    ffi_guard(out_error, || unsafe {
        if !valid_out_error(out_error) {
            return SYNUI_INVALID_ARGUMENT;
        }
        let text = match string_from_text_view(text) {
            Ok(text) => text,
            Err(status) => return complete(status, "invalid UTF-8 text", out_error),
        };
        let token = match valid_token(element.cast()) {
            Ok(token) => token,
            Err(status) => return complete(status, "invalid element", out_error),
        };
        let mut registry = handles();
        let (lease, _) = match registry.validate(token, HandleKind::Element) {
            Ok(value) => value,
            Err(status) => return complete(status, "text element", out_error),
        };
        let Some(element) = registry.elements.get(&lease.key) else {
            return complete(SYNUI_STALE_HANDLE, "text element", out_error);
        };
        if let Err(status) = ensure_app_ui_thread(&registry, element.app) {
            return complete(status, "text update thread", out_error);
        }
        let Some(element) = registry.elements.get_mut(&lease.key) else {
            return complete(SYNUI_STALE_HANDLE, "text element", out_error);
        };
        match &mut element.data {
            ElementData::Text { text: current, .. } => *current = text,
            ElementData::Button { .. } => {
                return complete(SYNUI_INVALID_ARGUMENT, "element is not text", out_error)
            }
        }
        complete(SYNUI_OK, "", out_error)
    })
}

#[no_mangle]
pub unsafe extern "C" fn synui_button_element_create(
    app: *const SynuiApp,
    callback: *const SynuiCallback,
    out_element: *mut *mut SynuiElement,
    out_error: *mut *mut SynuiErrorRecord,
) -> SynuiStatus {
    ffi_guard(out_error, || unsafe {
        if !valid_out_error(out_error) || out_element.is_null() || !aligned(out_element) {
            return complete(SYNUI_INVALID_ARGUMENT, "invalid element output", out_error);
        }
        let app_token = match valid_token(app.cast()) {
            Ok(token) => token,
            Err(status) => return complete(status, "invalid App", out_error),
        };
        let callback_token = match valid_token(callback.cast()) {
            Ok(token) => token,
            Err(status) => return complete(status, "invalid callback", out_error),
        };
        let token = {
            let mut registry = handles();
            let (app_lease, _) = match registry.validate(app_token, HandleKind::App) {
                Ok(value) => value,
                Err(status) => return complete(status, "element App", out_error),
            };
            let (callback_lease, _) = match registry.validate(callback_token, HandleKind::Callback)
            {
                Ok(value) => value,
                Err(status) => return complete(status, "element callback", out_error),
            };
            let Some(callback) = registry.callbacks.get(&callback_lease.key) else {
                return complete(SYNUI_STALE_HANDLE, "element callback", out_error);
            };
            if callback.app != app_lease.key {
                return complete(SYNUI_CONTEXT_MISMATCH, "callback owner", out_error);
            }
            let action = callback.action.clone();
            let last_status = Arc::clone(&callback.last_status);
            match registry.create_element(
                app_lease.key,
                ElementData::Button {
                    action,
                    last_status,
                },
            ) {
                Ok(token) => token,
                Err(status) => return complete(status, "element allocation", out_error),
            }
        };
        ptr::write(out_element, token.cast::<SynuiElement>());
        complete(SYNUI_OK, "", out_error)
    })
}

#[no_mangle]
pub unsafe extern "C" fn synui_app_render(
    app: *const SynuiApp,
    elements: *const *const SynuiElement,
    elements_len: usize,
    out_error: *mut *mut SynuiErrorRecord,
) -> SynuiStatus {
    ffi_guard(out_error, || unsafe {
        if !valid_out_error(out_error)
            || (elements.is_null() && elements_len != 0)
            || elements_len > isize::MAX as usize / mem::size_of::<*const SynuiElement>()
        {
            return complete(SYNUI_INVALID_ARGUMENT, "invalid render elements", out_error);
        }
        let app_token = match valid_token(app.cast()) {
            Ok(token) => token,
            Err(status) => return complete(status, "invalid App", out_error),
        };
        let inputs = if elements_len == 0 {
            &[][..]
        } else {
            core::slice::from_raw_parts(elements, elements_len)
        };
        let (app_key, mut host, nodes, keys) = {
            let mut registry = handles();
            let (app_lease, _) = match registry.validate(app_token, HandleKind::App) {
                Ok(value) => value,
                Err(status) => return complete(status, "render App", out_error),
            };
            if let Err(status) = ensure_app_ui_thread(&registry, app_lease.key) {
                return complete(status, "render thread", out_error);
            }
            let mut nodes = Vec::with_capacity(inputs.len());
            let mut keys = Vec::with_capacity(inputs.len());
            for element in inputs {
                let token = match valid_token(element.cast()) {
                    Ok(token) => token,
                    Err(status) => return complete(status, "render element", out_error),
                };
                let (lease, _) = match registry.validate(token, HandleKind::Element) {
                    Ok(value) => value,
                    Err(status) => return complete(status, "render element", out_error),
                };
                let Some(element) = registry.elements.get_mut(&lease.key) else {
                    return complete(SYNUI_STALE_HANDLE, "render element", out_error);
                };
                if element.app != app_lease.key {
                    return complete(SYNUI_CONTEXT_MISMATCH, "render element owner", out_error);
                }
                let node = match &mut element.data {
                    ElementData::Text { text, .. } => Node::text(text.clone()),
                    ElementData::Button {
                        action,
                        last_status,
                    } => {
                        *last_status.lock().expect("callback status lock poisoned") = SYNUI_OK;
                        Button::new("+1").on_press(action.clone()).node()
                    }
                };
                nodes.push(node);
                keys.push(lease.key);
            }
            let Some(app) = registry.apps.get_mut(&app_lease.key) else {
                return complete(SYNUI_CONTEXT_DESTROYED, "render App", out_error);
            };
            if app.callback_depth != 0 {
                return complete(SYNUI_CALLBACK_ERROR, "render during callback", out_error);
            }
            app.callback_depth += 1;
            let Some(host) = app.host.take() else {
                app.callback_depth -= 1;
                return complete(SYNUI_CALLBACK_ERROR, "render reentrancy", out_error);
            };
            (app_lease.key, host, nodes, keys)
        };
        let result = catch_unwind(AssertUnwindSafe(|| host.render(Node::container(nodes))));
        {
            let mut registry = handles();
            let render_succeeded = matches!(&result, Ok(Ok(())));
            if render_succeeded {
                for key in &keys {
                    if let Some(element) = registry.elements.get_mut(key) {
                        if let ElementData::Text { text, rendered } = &mut element.data {
                            *rendered = text.clone();
                        }
                    }
                }
            }
            if let Some(app) = registry.apps.get_mut(&app_key) {
                app.host = Some(host);
                app.callback_depth = app.callback_depth.saturating_sub(1);
                if render_succeeded {
                    app.rendered_elements = keys;
                }
            }
        }
        match result {
            Ok(Ok(())) => complete(SYNUI_OK, "", out_error),
            Ok(Err(_)) => complete(SYNUI_INTERNAL_ERROR, "facade render failed", out_error),
            Err(_) => complete(SYNUI_PANIC_CONTAINED, "facade render panic", out_error),
        }
    })
}

#[no_mangle]
pub unsafe extern "C" fn synui_element_press(
    element: *const SynuiElement,
    out_error: *mut *mut SynuiErrorRecord,
) -> SynuiStatus {
    ffi_guard(out_error, || unsafe {
        if !valid_out_error(out_error) {
            return SYNUI_INVALID_ARGUMENT;
        }
        let token = match valid_token(element.cast()) {
            Ok(token) => token,
            Err(status) => return complete(status, "invalid element", out_error),
        };
        let (app_key, host, index, callback_status) = {
            let mut registry = handles();
            let (lease, _) = match registry.validate(token, HandleKind::Element) {
                Ok(value) => value,
                Err(status) => return complete(status, "press element", out_error),
            };
            let (app_key, callback_status) = {
                let Some(element) = registry.elements.get(&lease.key) else {
                    return complete(SYNUI_STALE_HANDLE, "press element", out_error);
                };
                let ElementData::Button { last_status, .. } = &element.data else {
                    return complete(SYNUI_INVALID_ARGUMENT, "element is not a button", out_error);
                };
                (element.app, Arc::clone(last_status))
            };
            if let Err(status) = ensure_app_ui_thread(&registry, app_key) {
                return complete(status, "press thread", out_error);
            }
            let Some(app) = registry.apps.get_mut(&app_key) else {
                return complete(SYNUI_CONTEXT_DESTROYED, "press App", out_error);
            };
            let Some(index) = app
                .rendered_elements
                .iter()
                .position(|rendered| *rendered == lease.key)
            else {
                return complete(SYNUI_STALE_HANDLE, "button is not rendered", out_error);
            };
            if app.callback_depth != 0 {
                return complete(SYNUI_CALLBACK_ERROR, "press during callback", out_error);
            }
            app.callback_depth += 1;
            let Some(host) = app.host.take() else {
                app.callback_depth -= 1;
                return complete(SYNUI_CALLBACK_ERROR, "press reentrancy", out_error);
            };
            (app_key, host, index, callback_status)
        };
        let result = catch_unwind(AssertUnwindSafe(|| host.press_root_child(index)));
        {
            let mut registry = handles();
            if let Some(app) = registry.apps.get_mut(&app_key) {
                app.host = Some(host);
                app.callback_depth = app.callback_depth.saturating_sub(1);
            }
        }
        match result {
            Ok(Ok(())) => {
                let status = *callback_status
                    .lock()
                    .expect("callback status lock poisoned");
                if status == SYNUI_OK {
                    complete(SYNUI_OK, "", out_error)
                } else if status == SYNUI_PANIC_CONTAINED {
                    complete(status, "button callback panic", out_error)
                } else {
                    complete(SYNUI_CALLBACK_ERROR, "button callback failed", out_error)
                }
            }
            Ok(Err(_)) => complete(SYNUI_STALE_HANDLE, "button route missing", out_error),
            Err(_) => complete(SYNUI_PANIC_CONTAINED, "button dispatch panic", out_error),
        }
    })
}

#[no_mangle]
pub unsafe extern "C" fn synui_text_element_get_rendered_text(
    element: *const SynuiElement,
    out_text: *mut SynuiBuffer,
    out_error: *mut *mut SynuiErrorRecord,
) -> SynuiStatus {
    ffi_guard(out_error, || unsafe {
        if !valid_out_error(out_error)
            || out_text.is_null()
            || !aligned(out_text)
            || error_output_overlaps(out_text, out_error)
        {
            return complete(
                SYNUI_INVALID_ARGUMENT,
                "invalid rendered text output",
                out_error,
            );
        }
        let output = ptr::read(out_text);
        if output.struct_size < SynuiBuffer::STRUCT_SIZE
            || !output.ptr.is_null()
            || output.len != 0
            || output.capacity != 0
        {
            return complete(
                SYNUI_INVALID_ARGUMENT,
                "rendered text output must be size-initialized and empty",
                out_error,
            );
        }
        let token = match valid_token(element.cast()) {
            Ok(token) => token,
            Err(status) => return complete(status, "invalid element", out_error),
        };
        let rendered = {
            let registry = handles();
            let (lease, _) = match registry.validate(token, HandleKind::Element) {
                Ok(value) => value,
                Err(status) => return complete(status, "rendered element", out_error),
            };
            let Some(element) = registry.elements.get(&lease.key) else {
                return complete(SYNUI_STALE_HANDLE, "rendered element", out_error);
            };
            if let Err(status) = ensure_app_ui_thread(&registry, element.app) {
                return complete(status, "rendered text thread", out_error);
            }
            match &element.data {
                ElementData::Text { rendered, .. } => rendered.clone(),
                ElementData::Button { .. } => {
                    return complete(SYNUI_INVALID_ARGUMENT, "element is not text", out_error)
                }
            }
        };
        let buffer = allocate_abi_buffer(rendered.as_bytes());
        let status = complete(SYNUI_OK, "", out_error);
        if status == SYNUI_OK {
            ptr::write(out_text, buffer);
        } else {
            free_abi_buffer(buffer);
        }
        status
    })
}

unsafe fn app_post_impl(
    app: *const SynuiApp,
    work: *const SynuiPost,
    out_token: *mut *mut SynuiPostToken,
    out_error: *mut *mut SynuiErrorRecord,
) -> SynuiStatus {
    if !valid_out_error(out_error) {
        return SYNUI_INVALID_ARGUMENT;
    }
    if out_token.is_null() || !aligned(out_token) {
        return unsafe { complete(SYNUI_INVALID_ARGUMENT, "invalid out_token", out_error) };
    }
    if error_output_overlaps(out_token, out_error) {
        return SYNUI_INVALID_ARGUMENT;
    }
    match unsafe { read_struct_size(work) } {
        Ok(size) if size >= SynuiPost::STRUCT_SIZE => {}
        _ => return unsafe { complete(SYNUI_INVALID_ARGUMENT, "invalid post struct", out_error) },
    }
    let work = unsafe { ptr::read(work) };
    let Some(callback) = work.callback else {
        return unsafe { complete(SYNUI_INVALID_ARGUMENT, "NULL post callback", out_error) };
    };
    if (work.payload.ptr.is_null() && work.payload.len != 0)
        || work.payload.len > isize::MAX as usize
    {
        return unsafe { complete(SYNUI_INVALID_ARGUMENT, "invalid post payload", out_error) };
    }
    let app_token = match valid_token(app.cast()) {
        Ok(token) => token,
        Err(status) => return unsafe { complete(status, "invalid app handle", out_error) },
    };
    let app_key = {
        let registry = handles();
        match registry.validate(app_token, HandleKind::App) {
            Ok((lease, _)) if registry.apps.contains_key(&lease.key) => lease.key,
            Ok(_) => return unsafe { complete(SYNUI_CONTEXT_DESTROYED, "app closed", out_error) },
            Err(status) => return unsafe { complete(status, "app post", out_error) },
        }
    };
    let payload = if work.payload.len == 0 {
        Vec::new()
    } else {
        unsafe { core::slice::from_raw_parts(work.payload.ptr, work.payload.len) }.to_vec()
    };
    let token = {
        let mut registry = handles();
        if !registry.apps.contains_key(&app_key) {
            return unsafe { complete(SYNUI_CONTEXT_DESTROYED, "app closed", out_error) };
        }
        match registry.create_post(app_key, callback, work.user_data as usize, payload) {
            Ok(token) => token,
            Err(status) => return unsafe { complete(status, "post allocation", out_error) },
        }
    };
    unsafe { ptr::write(out_token, token.cast::<SynuiPostToken>()) };
    unsafe { complete(SYNUI_OK, "", out_error) }
}

#[no_mangle]
pub unsafe extern "C" fn synui_app_post(
    app: *const SynuiApp,
    work: *const SynuiPost,
    out_token: *mut *mut SynuiPostToken,
    out_error: *mut *mut SynuiErrorRecord,
) -> SynuiStatus {
    ffi_guard(out_error, || unsafe {
        app_post_impl(app, work, out_token, out_error)
    })
}

unsafe fn app_drain_posts_impl(
    app: *const SynuiApp,
    out_drained: *mut usize,
    out_error: *mut *mut SynuiErrorRecord,
) -> SynuiStatus {
    if !valid_out_error(out_error) {
        return SYNUI_INVALID_ARGUMENT;
    }
    if out_drained.is_null() || !aligned(out_drained) {
        return unsafe { complete(SYNUI_INVALID_ARGUMENT, "invalid out_drained", out_error) };
    }
    if error_output_overlaps(out_drained, out_error) {
        return SYNUI_INVALID_ARGUMENT;
    }
    let app_token = match valid_token(app.cast()) {
        Ok(token) => token,
        Err(status) => return unsafe { complete(status, "invalid app handle", out_error) },
    };
    let (app_key, limit) = {
        let registry = handles();
        let (lease, _) = match registry.validate(app_token, HandleKind::App) {
            Ok(value) => value,
            Err(status) => return unsafe { complete(status, "app drain", out_error) },
        };
        let Some(app_record) = registry.apps.get(&lease.key) else {
            return unsafe { complete(SYNUI_CONTEXT_DESTROYED, "app closed", out_error) };
        };
        if app_record.ui_thread != thread::current().id() {
            return unsafe {
                complete(
                    SYNUI_THREAD_AFFINITY,
                    "drain requires App UI thread",
                    out_error,
                )
            };
        }
        if app_record.callback_depth != 0 {
            return unsafe {
                complete(
                    SYNUI_CALLBACK_ERROR,
                    "synchronous drain during callback",
                    out_error,
                )
            };
        }
        (lease.key, app_record.queue.len())
    };

    let mut drained = 0usize;
    let mut operation_status = SYNUI_OK;
    for _ in 0..limit {
        let invocation = {
            let mut registry = handles();
            let post_key = match registry
                .apps
                .get_mut(&app_key)
                .and_then(|app| app.queue.pop_front())
            {
                Some(key) => key,
                None => break,
            };
            let Some(post) = registry.posts.get_mut(&post_key) else {
                let _ = registry.decrement_resource(post_key);
                continue;
            };
            if post.state != SYNUI_POST_ACCEPTED {
                let _ = registry.decrement_resource(post_key);
                continue;
            }
            post.state = SYNUI_POST_RUNNING;
            let invocation = (
                post_key,
                post.callback,
                post.user_data,
                post.payload.as_ptr(),
                post.payload.len(),
            );
            registry
                .apps
                .get_mut(&app_key)
                .expect("active app disappeared")
                .callback_depth += 1;
            invocation
        };
        let payload = SynuiCallbackPayload {
            struct_size: mem::size_of::<SynuiCallbackPayload>() as u32,
            kind: 0,
            bytes: SynuiBytesView {
                ptr: if invocation.4 == 0 {
                    ptr::null()
                } else {
                    invocation.3
                },
                len: invocation.4,
            },
        };
        let callback_result = catch_unwind(AssertUnwindSafe(|| unsafe {
            (invocation.1)(invocation.2 as *mut c_void, &payload)
        }));
        let (state, callback_status, drain_status) = match callback_result {
            Ok(SYNUI_OK) => (SYNUI_POST_EXECUTED, SYNUI_OK, SYNUI_OK),
            Ok(status) => (SYNUI_POST_FAILED, status, SYNUI_CALLBACK_ERROR),
            Err(_) => (
                SYNUI_POST_FAILED,
                SYNUI_PANIC_CONTAINED,
                SYNUI_PANIC_CONTAINED,
            ),
        };
        {
            let mut registry = handles();
            if let Some(post) = registry.posts.get_mut(&invocation.0) {
                post.state = state;
                post.callback_status = callback_status;
            }
            if let Some(app_record) = registry.apps.get_mut(&app_key) {
                app_record.callback_depth = app_record.callback_depth.saturating_sub(1);
            }
            let _ = registry.decrement_resource(invocation.0);
        }
        drained += 1;
        if drain_status != SYNUI_OK {
            operation_status = drain_status;
            break;
        }
    }
    unsafe { ptr::write(out_drained, drained) };
    unsafe {
        complete(
            operation_status,
            if operation_status == SYNUI_OK {
                ""
            } else {
                "post callback failed"
            },
            out_error,
        )
    }
}

#[no_mangle]
pub unsafe extern "C" fn synui_app_drain_posts(
    app: *const SynuiApp,
    out_drained: *mut usize,
    out_error: *mut *mut SynuiErrorRecord,
) -> SynuiStatus {
    ffi_guard(out_error, || unsafe {
        app_drain_posts_impl(app, out_drained, out_error)
    })
}

unsafe fn post_token_query_impl(
    token: *const SynuiPostToken,
    out_info: *mut SynuiPostTokenInfo,
    out_error: *mut *mut SynuiErrorRecord,
) -> SynuiStatus {
    if !valid_out_error(out_error) {
        return SYNUI_INVALID_ARGUMENT;
    }
    if error_output_overlaps(out_info, out_error) {
        return SYNUI_INVALID_ARGUMENT;
    }
    match unsafe { read_struct_size(out_info) } {
        Ok(size) if size >= SynuiPostTokenInfo::STRUCT_SIZE => {}
        _ => return unsafe { complete(SYNUI_INVALID_ARGUMENT, "invalid post info", out_error) },
    }
    let token = match valid_token(token.cast()) {
        Ok(token) => token,
        Err(status) => return unsafe { complete(status, "invalid post token", out_error) },
    };
    let info = {
        let registry = handles();
        let (lease, _) = match registry.validate(token, HandleKind::PostToken) {
            Ok(value) => value,
            Err(status) => return unsafe { complete(status, "post query", out_error) },
        };
        let Some(post) = registry.posts.get(&lease.key) else {
            return unsafe { complete(SYNUI_STALE_HANDLE, "post query", out_error) };
        };
        SynuiPostTokenInfo {
            struct_size: SynuiPostTokenInfo::STRUCT_SIZE,
            state: post.state,
            callback_status: post.callback_status,
        }
    };
    unsafe { ptr::write(out_info, info) };
    unsafe { complete(SYNUI_OK, "", out_error) }
}

#[no_mangle]
pub unsafe extern "C" fn synui_post_token_query(
    token: *const SynuiPostToken,
    out_info: *mut SynuiPostTokenInfo,
    out_error: *mut *mut SynuiErrorRecord,
) -> SynuiStatus {
    ffi_guard(out_error, || unsafe {
        post_token_query_impl(token, out_info, out_error)
    })
}

unsafe fn post_token_cancel_impl(
    token: *const SynuiPostToken,
    out_error: *mut *mut SynuiErrorRecord,
) -> SynuiStatus {
    if !valid_out_error(out_error) {
        return SYNUI_INVALID_ARGUMENT;
    }
    let token = match valid_token(token.cast()) {
        Ok(token) => token,
        Err(status) => return unsafe { complete(status, "invalid post token", out_error) },
    };
    let status = handles().cancel_post(token);
    unsafe {
        complete(
            status,
            if status == SYNUI_OK {
                ""
            } else {
                "post cancel"
            },
            out_error,
        )
    }
}

#[no_mangle]
pub unsafe extern "C" fn synui_post_token_cancel(
    token: *const SynuiPostToken,
    out_error: *mut *mut SynuiErrorRecord,
) -> SynuiStatus {
    ffi_guard(out_error, || unsafe {
        post_token_cancel_impl(token, out_error)
    })
}

fn usize_from_u64(value: u64) -> Result<usize, SynuiStatus> {
    usize::try_from(value).map_err(|_| SYNUI_INVALID_ARGUMENT)
}

fn cell_range_from_abi(range: SynuiCellRange) -> Result<CellRange, SynuiStatus> {
    if range.struct_size < SynuiCellRange::STRUCT_SIZE {
        return Err(SYNUI_INVALID_ARGUMENT);
    }
    CellRange::new(
        usize_from_u64(range.row_start)?,
        usize_from_u64(range.row_end)?,
        usize_from_u64(range.column_start)?,
        usize_from_u64(range.column_end)?,
    )
    .map_err(|_| SYNUI_INVALID_ARGUMENT)
}

fn prefetch_from_abi(prefetch: SynuiPrefetchBand) -> Result<PrefetchBand, SynuiStatus> {
    if prefetch.struct_size < SynuiPrefetchBand::STRUCT_SIZE {
        return Err(SYNUI_INVALID_ARGUMENT);
    }
    Ok(PrefetchBand {
        rows_before: usize_from_u64(prefetch.rows_before)?,
        rows_after: usize_from_u64(prefetch.rows_after)?,
        columns_before: usize_from_u64(prefetch.columns_before)?,
        columns_after: usize_from_u64(prefetch.columns_after)?,
    })
}

unsafe fn cell_value_from_abi(value: *const SynuiCellValue) -> Result<CellValue, SynuiStatus> {
    match unsafe { read_struct_size(value) } {
        Ok(size) if size >= SynuiCellValue::STRUCT_SIZE => {}
        _ => return Err(SYNUI_INVALID_ARGUMENT),
    }
    let value = unsafe { ptr::read(value) };
    match value.kind {
        SYNUI_CELL_EMPTY => Ok(CellValue::Empty),
        SYNUI_CELL_TEXT => unsafe { string_from_text_view(value.text) }.map(CellValue::Text),
        SYNUI_CELL_BOOL if value.boolean <= 1 => Ok(CellValue::Bool(value.boolean != 0)),
        SYNUI_CELL_NUMBER if value.number.is_finite() => Ok(CellValue::Number(value.number)),
        _ => Err(SYNUI_INVALID_ARGUMENT),
    }
}

fn model_error_status(error: VirtualModelError) -> SynuiStatus {
    match error {
        VirtualModelError::InvalidRange | VirtualModelError::OutOfBounds => SYNUI_INVALID_ARGUMENT,
        VirtualModelError::RequestIdOverflow => SYNUI_INTERNAL_ERROR,
        VirtualModelError::App(synui::app::AppError::ContextMismatch) => SYNUI_CONTEXT_MISMATCH,
        VirtualModelError::App(synui::app::AppError::ContextDestroyed) => SYNUI_CONTEXT_DESTROYED,
        VirtualModelError::App(synui::app::AppError::ThreadAffinity) => SYNUI_THREAD_AFFINITY,
        VirtualModelError::App(synui::app::AppError::StaleHandle) => SYNUI_CALLBACK_ERROR,
    }
}

fn take_model_for_call(token: usize) -> Result<(ResourceKey, ModelRecord), SynuiStatus> {
    let mut registry = handles();
    let (lease, resource) = registry.validate(token, HandleKind::Model)?;
    if resource.capabilities & SYNUI_CAPABILITY_VIRTUAL_MODEL == 0 {
        return Err(SYNUI_UNSUPPORTED_CAPABILITY);
    }
    let app_key = registry
        .models
        .get(&lease.key)
        .map(|model| model.app)
        .ok_or(SYNUI_STALE_HANDLE)?;
    let app = registry
        .apps
        .get_mut(&app_key)
        .ok_or(SYNUI_CONTEXT_DESTROYED)?;
    if app.ui_thread != thread::current().id() {
        return Err(SYNUI_THREAD_AFFINITY);
    }
    if app.callback_depth != 0 {
        return Err(SYNUI_CALLBACK_ERROR);
    }
    app.callback_depth += 1;
    let model = registry
        .models
        .remove(&lease.key)
        .ok_or(SYNUI_STALE_HANDLE)?;
    Ok((lease.key, model))
}

fn restore_model_after_call(key: ResourceKey, model: ModelRecord) {
    let mut registry = handles();
    if let Some(app) = registry.apps.get_mut(&model.app) {
        app.callback_depth = app.callback_depth.saturating_sub(1);
    }
    if registry.resources.get(&key.slot).is_some_and(|resource| {
        resource.kind == HandleKind::Model && resource.generation == key.generation
    }) {
        registry.models.insert(key, model);
    }
}

unsafe fn virtual_model_create_impl(
    app: *const SynuiApp,
    vtable: *const SynuiVirtualDataTableModelVtable,
    user_data: *mut c_void,
    out_model: *mut *mut SynuiModel,
    out_error: *mut *mut SynuiErrorRecord,
) -> SynuiStatus {
    if !valid_out_error(out_error) {
        return SYNUI_INVALID_ARGUMENT;
    }
    if out_model.is_null() || !aligned(out_model) {
        return unsafe { complete(SYNUI_INVALID_ARGUMENT, "invalid out_model", out_error) };
    }
    if error_output_overlaps(out_model, out_error) {
        return SYNUI_INVALID_ARGUMENT;
    }
    match unsafe { read_struct_size(vtable) } {
        Ok(size) if size >= SynuiVirtualDataTableModelVtable::STRUCT_SIZE => {}
        _ => return unsafe { complete(SYNUI_INVALID_ARGUMENT, "invalid model vtable", out_error) },
    }
    let vtable = unsafe { ptr::read(vtable) };
    if vtable.dimensions.is_none()
        || vtable.request_cells.is_none()
        || vtable.apply_cell_update.is_none()
        || vtable.sort_changed.is_none()
        || vtable.selection_changed.is_none()
    {
        return unsafe { complete(SYNUI_INVALID_ARGUMENT, "incomplete model vtable", out_error) };
    }
    let app_token = match valid_token(app.cast()) {
        Ok(token) => token,
        Err(status) => return unsafe { complete(status, "invalid app handle", out_error) },
    };
    let app_key = {
        let mut registry = handles();
        let (lease, resource) = match registry.validate(app_token, HandleKind::App) {
            Ok(value) => value,
            Err(status) => return unsafe { complete(status, "model App", out_error) },
        };
        if resource.capabilities & SYNUI_CAPABILITY_VIRTUAL_MODEL == 0 {
            return unsafe {
                complete(
                    SYNUI_UNSUPPORTED_CAPABILITY,
                    "virtual model capability not enabled",
                    out_error,
                )
            };
        }
        let Some(app) = registry.apps.get_mut(&lease.key) else {
            return unsafe { complete(SYNUI_CONTEXT_DESTROYED, "App closed", out_error) };
        };
        if app.ui_thread != thread::current().id() {
            return unsafe {
                complete(
                    SYNUI_THREAD_AFFINITY,
                    "model creation requires App UI thread",
                    out_error,
                )
            };
        }
        if app.callback_depth != 0 {
            return unsafe {
                complete(
                    SYNUI_CALLBACK_ERROR,
                    "model creation during callback",
                    out_error,
                )
            };
        }
        app.callback_depth += 1;
        lease.key
    };

    let mut dimensions = SynuiTableDimensions {
        struct_size: SynuiTableDimensions::STRUCT_SIZE,
        rows: 0,
        columns: 0,
    };
    let callback = catch_unwind(AssertUnwindSafe(|| unsafe {
        (vtable.dimensions.expect("validated model vtable"))(user_data, &mut dimensions)
    }));
    {
        let mut registry = handles();
        if let Some(app) = registry.apps.get_mut(&app_key) {
            app.callback_depth = app.callback_depth.saturating_sub(1);
        }
    }
    let callback_status = match callback {
        Ok(status) => status,
        Err(_) => {
            return unsafe { complete(SYNUI_PANIC_CONTAINED, "model dimensions panic", out_error) }
        }
    };
    if callback_status != SYNUI_OK {
        return unsafe { complete(SYNUI_CALLBACK_ERROR, "model dimensions failed", out_error) };
    }
    if dimensions.struct_size < SynuiTableDimensions::STRUCT_SIZE {
        return unsafe {
            complete(
                SYNUI_INVALID_ARGUMENT,
                "invalid model dimensions",
                out_error,
            )
        };
    }
    let dimensions = TableDimensions::new(
        match usize_from_u64(dimensions.rows) {
            Ok(value) => value,
            Err(status) => return unsafe { complete(status, "model row count", out_error) },
        },
        match usize_from_u64(dimensions.columns) {
            Ok(value) => value,
            Err(status) => return unsafe { complete(status, "model column count", out_error) },
        },
    );
    let token = {
        let mut registry = handles();
        if !registry.apps.contains_key(&app_key) {
            return unsafe { complete(SYNUI_CONTEXT_DESTROYED, "App closed", out_error) };
        }
        match registry.create_model(
            app_key,
            AbiVirtualModel {
                dimensions,
                vtable,
                user_data: user_data as usize,
            },
        ) {
            Ok(token) => token,
            Err(status) => return unsafe { complete(status, "model allocation", out_error) },
        }
    };
    unsafe { ptr::write(out_model, token.cast::<SynuiModel>()) };
    unsafe { complete(SYNUI_OK, "", out_error) }
}

#[no_mangle]
pub unsafe extern "C" fn synui_virtual_data_table_model_create(
    app: *const SynuiApp,
    vtable: *const SynuiVirtualDataTableModelVtable,
    user_data: *mut c_void,
    out_model: *mut *mut SynuiModel,
    out_error: *mut *mut SynuiErrorRecord,
) -> SynuiStatus {
    ffi_guard(out_error, || unsafe {
        virtual_model_create_impl(app, vtable, user_data, out_model, out_error)
    })
}

unsafe fn model_request_visible_impl(
    model: *const SynuiModel,
    viewport: SynuiCellRange,
    prefetch: SynuiPrefetchBand,
    out_requested: *mut u32,
    out_request: *mut SynuiCellRequest,
    out_error: *mut *mut SynuiErrorRecord,
) -> SynuiStatus {
    if !valid_out_error(out_error) {
        return SYNUI_INVALID_ARGUMENT;
    }
    if out_requested.is_null()
        || !aligned(out_requested)
        || out_request.is_null()
        || !aligned(out_request)
    {
        return unsafe { complete(SYNUI_INVALID_ARGUMENT, "invalid request output", out_error) };
    }
    match unsafe { read_struct_size(out_request) } {
        Ok(size) if size >= SynuiCellRequest::STRUCT_SIZE => {}
        _ => {
            return unsafe { complete(SYNUI_INVALID_ARGUMENT, "invalid request output", out_error) }
        }
    }
    if error_output_overlaps(out_requested, out_error)
        || error_output_overlaps(out_request, out_error)
    {
        return SYNUI_INVALID_ARGUMENT;
    }
    let viewport = match cell_range_from_abi(viewport) {
        Ok(value) => value,
        Err(status) => return unsafe { complete(status, "invalid viewport", out_error) },
    };
    let prefetch = match prefetch_from_abi(prefetch) {
        Ok(value) => value,
        Err(status) => return unsafe { complete(status, "invalid prefetch", out_error) },
    };
    let token = match valid_token(model.cast()) {
        Ok(token) => token,
        Err(status) => return unsafe { complete(status, "invalid model handle", out_error) },
    };
    let (key, mut record) = match take_model_for_call(token) {
        Ok(value) => value,
        Err(status) => return unsafe { complete(status, "model request", out_error) },
    };
    record.adapter.set_prefetch(prefetch);
    let result = catch_unwind(AssertUnwindSafe(|| {
        record.adapter.request_visible(&record.context, viewport)
    }));
    restore_model_after_call(key, record);
    let request = match result {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => {
            let status = model_error_status(error);
            return unsafe { complete(status, "model request failed", out_error) };
        }
        Err(_) => {
            return unsafe { complete(SYNUI_PANIC_CONTAINED, "model callback panic", out_error) }
        }
    };
    unsafe { ptr::write(out_requested, u32::from(request.is_some())) };
    if let Some(request) = request {
        unsafe {
            ptr::write(
                out_request,
                SynuiCellRequest {
                    struct_size: SynuiCellRequest::STRUCT_SIZE,
                    request_id: request.id,
                    range: cell_range_to_abi(request.range),
                },
            )
        };
    }
    unsafe { complete(SYNUI_OK, "", out_error) }
}

#[no_mangle]
pub unsafe extern "C" fn synui_virtual_data_table_model_request_visible(
    model: *const SynuiModel,
    viewport: SynuiCellRange,
    prefetch: SynuiPrefetchBand,
    out_requested: *mut u32,
    out_request: *mut SynuiCellRequest,
    out_error: *mut *mut SynuiErrorRecord,
) -> SynuiStatus {
    ffi_guard(out_error, || unsafe {
        model_request_visible_impl(
            model,
            viewport,
            prefetch,
            out_requested,
            out_request,
            out_error,
        )
    })
}

#[no_mangle]
pub unsafe extern "C" fn synui_virtual_data_table_model_apply_cell_update(
    model: *const SynuiModel,
    coordinate: *const SynuiCellCoordinate,
    value: *const SynuiCellValue,
    out_error: *mut *mut SynuiErrorRecord,
) -> SynuiStatus {
    ffi_guard(out_error, || unsafe {
        if !valid_out_error(out_error) {
            return SYNUI_INVALID_ARGUMENT;
        }
        match read_struct_size(coordinate) {
            Ok(size) if size >= SynuiCellCoordinate::STRUCT_SIZE => {}
            _ => return complete(SYNUI_INVALID_ARGUMENT, "invalid cell coordinate", out_error),
        }
        let coordinate = ptr::read(coordinate);
        let coordinate = CellCoordinate {
            row: match usize_from_u64(coordinate.row) {
                Ok(value) => value,
                Err(status) => return complete(status, "invalid cell row", out_error),
            },
            column: match usize_from_u64(coordinate.column) {
                Ok(value) => value,
                Err(status) => return complete(status, "invalid cell column", out_error),
            },
        };
        let value = match cell_value_from_abi(value) {
            Ok(value) => value,
            Err(status) => return complete(status, "invalid cell value", out_error),
        };
        let token = match valid_token(model.cast()) {
            Ok(token) => token,
            Err(status) => return complete(status, "invalid model handle", out_error),
        };
        let (key, record) = match take_model_for_call(token) {
            Ok(value) => value,
            Err(status) => return complete(status, "model update", out_error),
        };
        let result = catch_unwind(AssertUnwindSafe(|| {
            record
                .adapter
                .apply_cell_update(&record.context, coordinate, value)
        }));
        restore_model_after_call(key, record);
        match result {
            Ok(Ok(())) => complete(SYNUI_OK, "", out_error),
            Ok(Err(error)) => complete(model_error_status(error), "model update failed", out_error),
            Err(_) => complete(SYNUI_PANIC_CONTAINED, "model callback panic", out_error),
        }
    })
}

#[no_mangle]
pub unsafe extern "C" fn synui_virtual_data_table_model_notify_sort(
    model: *const SynuiModel,
    sort: *const SynuiSortDescriptor,
    sort_len: usize,
    out_error: *mut *mut SynuiErrorRecord,
) -> SynuiStatus {
    ffi_guard(out_error, || unsafe {
        if !valid_out_error(out_error) {
            return SYNUI_INVALID_ARGUMENT;
        }
        if (sort.is_null() && sort_len != 0)
            || sort_len > isize::MAX as usize / mem::size_of::<SynuiSortDescriptor>()
        {
            return complete(SYNUI_INVALID_ARGUMENT, "invalid sort array", out_error);
        }
        let input = if sort_len == 0 {
            &[][..]
        } else {
            core::slice::from_raw_parts(sort, sort_len)
        };
        let mut parsed = Vec::with_capacity(input.len());
        for item in input {
            if item.struct_size < SynuiSortDescriptor::STRUCT_SIZE {
                return complete(SYNUI_INVALID_ARGUMENT, "invalid sort descriptor", out_error);
            }
            let direction = match item.direction {
                SYNUI_SORT_ASCENDING => SortDirection::Ascending,
                SYNUI_SORT_DESCENDING => SortDirection::Descending,
                _ => return complete(SYNUI_INVALID_ARGUMENT, "invalid sort direction", out_error),
            };
            parsed.push(SortDescriptor {
                column: match usize_from_u64(item.column) {
                    Ok(value) => value,
                    Err(status) => return complete(status, "invalid sort column", out_error),
                },
                direction,
            });
        }
        let token = match valid_token(model.cast()) {
            Ok(token) => token,
            Err(status) => return complete(status, "invalid model handle", out_error),
        };
        let (key, record) = match take_model_for_call(token) {
            Ok(value) => value,
            Err(status) => return complete(status, "model sort", out_error),
        };
        let result = catch_unwind(AssertUnwindSafe(|| {
            record.adapter.notify_sort(&record.context, &parsed)
        }));
        restore_model_after_call(key, record);
        match result {
            Ok(Ok(())) => complete(SYNUI_OK, "", out_error),
            Ok(Err(error)) => complete(model_error_status(error), "model sort failed", out_error),
            Err(_) => complete(SYNUI_PANIC_CONTAINED, "model callback panic", out_error),
        }
    })
}

#[no_mangle]
pub unsafe extern "C" fn synui_virtual_data_table_model_notify_selection(
    model: *const SynuiModel,
    ranges: *const SynuiCellRange,
    ranges_len: usize,
    out_error: *mut *mut SynuiErrorRecord,
) -> SynuiStatus {
    ffi_guard(out_error, || unsafe {
        if !valid_out_error(out_error) {
            return SYNUI_INVALID_ARGUMENT;
        }
        if (ranges.is_null() && ranges_len != 0)
            || ranges_len > isize::MAX as usize / mem::size_of::<SynuiCellRange>()
        {
            return complete(SYNUI_INVALID_ARGUMENT, "invalid selection array", out_error);
        }
        let input = if ranges_len == 0 {
            &[][..]
        } else {
            core::slice::from_raw_parts(ranges, ranges_len)
        };
        let mut parsed = Vec::with_capacity(input.len());
        for range in input {
            parsed.push(match cell_range_from_abi(*range) {
                Ok(value) => value,
                Err(status) => return complete(status, "invalid selection range", out_error),
            });
        }
        let token = match valid_token(model.cast()) {
            Ok(token) => token,
            Err(status) => return complete(status, "invalid model handle", out_error),
        };
        let (key, record) = match take_model_for_call(token) {
            Ok(value) => value,
            Err(status) => return complete(status, "model selection", out_error),
        };
        let selection = SelectionChange { ranges: parsed };
        let result = catch_unwind(AssertUnwindSafe(|| {
            record.adapter.notify_selection(&record.context, &selection)
        }));
        restore_model_after_call(key, record);
        match result {
            Ok(Ok(())) => complete(SYNUI_OK, "", out_error),
            Ok(Err(error)) => complete(
                model_error_status(error),
                "model selection failed",
                out_error,
            ),
            Err(_) => complete(SYNUI_PANIC_CONTAINED, "model callback panic", out_error),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn options() -> SynuiRuntimeOptions {
        SynuiRuntimeOptions {
            struct_size: SynuiRuntimeOptions::STRUCT_SIZE,
            requested_version: SYNUI_ABI_VERSION,
            requested_capabilities: SYNUI_CAPABILITY_BASE,
        }
    }

    unsafe fn assert_record(record: *mut SynuiErrorRecord, status: SynuiStatus) {
        assert!(!record.is_null());
        let record_ref = unsafe { &*record };
        assert_eq!(record_ref.struct_size, SynuiErrorRecord::STRUCT_SIZE);
        assert_eq!(record_ref.status, status);
        assert_eq!(record_ref.code, status);
        if status == SYNUI_OK {
            assert!(record_ref.message.ptr.is_null());
            assert_eq!(record_ref.message.len, 0);
            assert!(record_ref.detail.ptr.is_null());
            assert_eq!(record_ref.detail.len, 0);
        } else {
            assert!(!record_ref.message.ptr.is_null());
            let message = unsafe {
                core::slice::from_raw_parts(record_ref.message.ptr, record_ref.message.len)
            };
            assert!(core::str::from_utf8(message).is_ok());
            assert!(!message.is_empty());
            assert!(!record_ref.detail.ptr.is_null());
            let detail = unsafe {
                core::slice::from_raw_parts(record_ref.detail.ptr, record_ref.detail.len)
            };
            assert!(core::str::from_utf8(detail).is_ok());
            assert!(!detail.is_empty());
        }
        unsafe { synui_error_record_free(record) };
    }

    unsafe fn create_runtime() -> *mut SynuiRuntime {
        let mut runtime = ptr::null_mut();
        let mut error = ptr::null_mut();
        let status = unsafe { synui_runtime_create(&options(), &mut runtime, &mut error) };
        assert_eq!(status, SYNUI_OK);
        unsafe { assert_record(error, SYNUI_OK) };
        assert!(!runtime.is_null());
        runtime
    }

    // Feature: synui-api-abi-reset, Property 8: ABI 协商不产生部分初始化
    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 100,
            max_shrink_iters: 256,
            .. ProptestConfig::default()
        })]
        #[test]
        fn unsupported_negotiation_leaves_runtime_output_unchanged(
            major in any::<u32>(),
            minor in any::<u32>(),
            capabilities in any::<SynuiCapabilities>(),
        ) {
            let options = SynuiRuntimeOptions {
                struct_size: SynuiRuntimeOptions::STRUCT_SIZE,
                requested_version: SynuiAbiVersion { major, minor },
                requested_capabilities: capabilities,
            };
            let sentinel = TOKEN_ALIGNMENT as *mut SynuiRuntime;
            let mut runtime = sentinel;
            let mut error = ptr::null_mut();
            let status = unsafe { synui_runtime_create(&options, &mut runtime, &mut error) };
            unsafe { assert_record(error, status) };

            if status == SYNUI_OK {
                prop_assert_ne!(runtime, sentinel);
                prop_assert_eq!(options.requested_version.major, SYNUI_ABI_MAJOR);
                prop_assert!(
                    capabilities & !SYNUI_CAPABILITIES == 0,
                    "successful negotiation must accept only published capabilities"
                );
                let release_status = unsafe { synui_runtime_release(runtime, ptr::null_mut()) };
                prop_assert_eq!(release_status, SYNUI_OK);
            } else {
                prop_assert_eq!(runtime, sentinel);
                prop_assert!(
                    status == SYNUI_UNSUPPORTED_ABI
                        || status == SYNUI_UNSUPPORTED_CAPABILITY
                );
            }
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 100,
            max_shrink_iters: 256,
            .. ProptestConfig::default()
        })]
        #[test]
        fn complete_always_returns_a_matching_error_record(
            status in any::<SynuiStatus>(),
            detail_byte in any::<u8>(),
        ) {
            let detail = format!("generated-detail-{detail_byte}");
            let mut record = ptr::null_mut();
            let returned = unsafe { complete(status, &detail, &mut record) };
            prop_assert_eq!(returned, status);
            prop_assert!(!record.is_null());
            let record_ref = unsafe { &*record };
            prop_assert_eq!(record_ref.status, status);
            prop_assert_eq!(record_ref.code, status);
            if status == SYNUI_OK {
                prop_assert!(record_ref.message.ptr.is_null());
                prop_assert_eq!(record_ref.message.len, 0);
                prop_assert!(record_ref.detail.ptr.is_null());
                prop_assert_eq!(record_ref.detail.len, 0);
            } else {
                prop_assert!(record_ref.message.len > 0);
                prop_assert!(record_ref.detail.len > 0);
                let message = unsafe {
                    core::slice::from_raw_parts(record_ref.message.ptr, record_ref.message.len)
                };
                let detail = unsafe {
                    core::slice::from_raw_parts(record_ref.detail.ptr, record_ref.detail.len)
                };
                prop_assert!(core::str::from_utf8(message).is_ok());
                prop_assert!(core::str::from_utf8(detail).is_ok());
            }
            unsafe { synui_error_record_free(record) };
        }
    }

    #[test]
    fn query_always_returns_matching_owned_record_when_requested() {
        let mut info = SynuiAbiInfo {
            struct_size: SynuiAbiInfo::STRUCT_SIZE,
            version: SynuiAbiVersion {
                major: 99,
                minor: 99,
            },
            capabilities: u64::MAX,
        };
        let mut error = ptr::null_mut();
        let status = unsafe {
            synui_abi_query(
                SynuiAbiVersion {
                    major: 99,
                    minor: 0,
                },
                0,
                &mut info,
                &mut error,
            )
        };
        assert_eq!(status, SYNUI_UNSUPPORTED_ABI);
        unsafe { assert_record(error, status) };

        let status = unsafe {
            synui_abi_query(
                SYNUI_ABI_VERSION,
                SYNUI_CAPABILITY_BASE,
                &mut info,
                &mut error,
            )
        };
        assert_eq!(status, SYNUI_OK);
        assert_eq!(info.version, SYNUI_ABI_VERSION);
        unsafe { assert_record(error, status) };
    }

    #[test]
    fn runtime_create_retain_query_release_is_a_complete_ownership_chain() {
        let runtime = unsafe { create_runtime() };
        let mut retained = ptr::null_mut();
        let mut error = ptr::null_mut();
        let status = unsafe { synui_runtime_retain(runtime, &mut retained, &mut error) };
        assert_eq!(status, SYNUI_OK);
        assert_ne!(retained, runtime);
        unsafe { assert_record(error, status) };

        let mut info = SynuiRuntimeInfo {
            struct_size: SynuiRuntimeInfo::STRUCT_SIZE,
            version: SynuiAbiVersion { major: 0, minor: 0 },
            capabilities: 0,
            resource_slot: 0,
            generation: 0,
            reference_count: 0,
        };
        let status = unsafe { synui_runtime_query(retained, &mut info, &mut error) };
        assert_eq!(status, SYNUI_OK);
        assert_eq!(info.reference_count, 2);
        assert!(info.generation > 0);
        unsafe { assert_record(error, status) };

        let status = unsafe { synui_runtime_release(runtime, &mut error) };
        assert_eq!(status, SYNUI_OK);
        unsafe { assert_record(error, status) };
        let status = unsafe { synui_runtime_query(retained, &mut info, &mut error) };
        assert_eq!(status, SYNUI_OK);
        assert_eq!(info.reference_count, 1);
        unsafe { assert_record(error, status) };
        let status = unsafe { synui_runtime_release(retained, &mut error) };
        assert_eq!(status, SYNUI_OK);
        unsafe { assert_record(error, status) };
    }

    #[test]
    fn duplicate_release_and_wrong_type_never_dereference_tokens() {
        let runtime = unsafe { create_runtime() };
        let mut error = ptr::null_mut();
        let wrong_status = unsafe { synui_app_release(runtime.cast::<SynuiApp>(), &mut error) };
        assert_eq!(wrong_status, SYNUI_INVALID_ARGUMENT);
        unsafe { assert_record(error, wrong_status) };

        let status = unsafe { synui_runtime_release(runtime, &mut error) };
        assert_eq!(status, SYNUI_OK);
        unsafe { assert_record(error, status) };
        let status = unsafe { synui_runtime_release(runtime, &mut error) };
        assert_eq!(status, SYNUI_STALE_HANDLE);
        unsafe { assert_record(error, status) };

        let status = unsafe { synui_runtime_release(ptr::null_mut(), &mut error) };
        assert_eq!(status, SYNUI_OK);
        unsafe { assert_record(error, status) };
    }

    #[test]
    fn validation_precedes_runtime_creation_and_output_writes() {
        let mut invalid = options();
        invalid.struct_size = SynuiRuntimeOptions::STRUCT_SIZE - 1;
        let sentinel = TOKEN_ALIGNMENT as *mut SynuiRuntime;
        let mut output = sentinel;
        let mut error = ptr::null_mut();
        let status = unsafe { synui_runtime_create(&invalid, &mut output, &mut error) };
        assert_eq!(status, SYNUI_INVALID_ARGUMENT);
        assert_eq!(output, sentinel);
        unsafe { assert_record(error, status) };

        let mut unsupported = options();
        unsupported.requested_capabilities = 1 << 63;
        let status = unsafe { synui_runtime_create(&unsupported, &mut output, &mut error) };
        assert_eq!(status, SYNUI_UNSUPPORTED_CAPABILITY);
        assert_eq!(output, sentinel);
        unsafe { assert_record(error, status) };
    }

    #[test]
    fn registry_reuses_slots_only_with_a_new_generation_and_never_reuses_tokens() {
        let mut registry = HandleRegistry::default();
        let first = registry
            .create_runtime(SYNUI_ABI_VERSION, SYNUI_CAPABILITY_BASE)
            .unwrap();
        let first_token = first as usize;
        let first_key = registry.handles[&first_token].lease.key;
        assert_eq!(registry.release(first_token, HandleKind::Runtime), SYNUI_OK);
        let second = registry
            .create_runtime(SYNUI_ABI_VERSION, SYNUI_CAPABILITY_BASE)
            .unwrap();
        let second_token = second as usize;
        let second_key = registry.handles[&second_token].lease.key;
        assert_ne!(first_token, second_token);
        assert_eq!(first_key.slot, second_key.slot);
        assert!(second_key.generation > first_key.generation);
        assert!(matches!(
            registry.validate(first_token, HandleKind::Runtime),
            Err(SYNUI_STALE_HANDLE)
        ));
    }

    #[test]
    fn released_runtime_is_stale_for_query_and_retain_without_output_writes() {
        let runtime = unsafe { create_runtime() };
        let mut error = ptr::null_mut();
        assert_eq!(
            unsafe { synui_runtime_release(runtime, &mut error) },
            SYNUI_OK
        );
        unsafe { assert_record(error, SYNUI_OK) };

        let original_info = SynuiRuntimeInfo {
            struct_size: SynuiRuntimeInfo::STRUCT_SIZE,
            version: SynuiAbiVersion {
                major: 77,
                minor: 88,
            },
            capabilities: u64::MAX,
            resource_slot: u64::MAX,
            generation: u64::MAX,
            reference_count: u64::MAX,
        };
        let mut info = original_info;
        let status = unsafe { synui_runtime_query(runtime, &mut info, &mut error) };
        assert_eq!(status, SYNUI_STALE_HANDLE);
        assert_eq!(info, original_info);
        unsafe { assert_record(error, status) };

        let sentinel = TOKEN_ALIGNMENT as *mut SynuiRuntime;
        let mut retained = sentinel;
        let status = unsafe { synui_runtime_retain(runtime, &mut retained, &mut error) };
        assert_eq!(status, SYNUI_STALE_HANDLE);
        assert_eq!(retained, sentinel);
        unsafe { assert_record(error, status) };
    }

    #[test]
    fn invalid_and_overlapping_outputs_are_rejected_before_writes() {
        let original_info = SynuiAbiInfo {
            struct_size: SynuiAbiInfo::STRUCT_SIZE,
            version: SynuiAbiVersion {
                major: u32::MAX,
                minor: u32::MAX,
            },
            capabilities: u64::MAX,
        };
        let mut info = original_info;
        let invalid_error = ptr::without_provenance_mut::<*mut SynuiErrorRecord>(1);
        let status = unsafe { synui_abi_query(SYNUI_ABI_VERSION, 0, &mut info, invalid_error) };
        assert_eq!(status, SYNUI_INVALID_ARGUMENT);
        assert_eq!(info, original_info);

        let overlapping_error = ptr::from_mut(&mut info).cast::<*mut SynuiErrorRecord>();
        let status = unsafe { synui_abi_query(SYNUI_ABI_VERSION, 0, &mut info, overlapping_error) };
        assert_eq!(status, SYNUI_INVALID_ARGUMENT);
        assert_eq!(info, original_info);

        let mut error = ptr::null_mut();
        let misaligned_runtime_output = ptr::without_provenance_mut::<*mut SynuiRuntime>(1);
        let status =
            unsafe { synui_runtime_create(&options(), misaligned_runtime_output, &mut error) };
        assert_eq!(status, SYNUI_INVALID_ARGUMENT);
        unsafe { assert_record(error, status) };

        let mut runtime: *mut SynuiRuntime = ptr::null_mut();
        let overlapping_error = ptr::from_mut(&mut runtime).cast::<*mut SynuiErrorRecord>();
        let status = unsafe { synui_runtime_create(&options(), &mut runtime, overlapping_error) };
        assert_eq!(status, SYNUI_INVALID_ARGUMENT);
        assert!(runtime.is_null());
    }

    #[test]
    fn status_is_a_stable_integer_and_error_record_free_never_dereferences_input() {
        assert_eq!(mem::size_of::<SynuiStatus>(), mem::size_of::<u32>());
        assert_eq!(
            [
                SYNUI_OK,
                SYNUI_UNSUPPORTED_ABI,
                SYNUI_UNSUPPORTED_CAPABILITY,
                SYNUI_INVALID_ARGUMENT,
                SYNUI_CONTEXT_MISMATCH,
                SYNUI_CONTEXT_DESTROYED,
                SYNUI_STALE_HANDLE,
                SYNUI_THREAD_AFFINITY,
                SYNUI_POST_CANCELLED,
                SYNUI_CALLBACK_ERROR,
                SYNUI_PANIC_CONTAINED,
                SYNUI_INTERNAL_ERROR,
            ],
            [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]
        );

        let mut info = SynuiAbiInfo {
            struct_size: SynuiAbiInfo::STRUCT_SIZE,
            version: SynuiAbiVersion { major: 0, minor: 0 },
            capabilities: 0,
        };
        let mut record = ptr::null_mut();
        assert_eq!(
            unsafe { synui_abi_query(SYNUI_ABI_VERSION, 0, &mut info, &mut record) },
            SYNUI_OK
        );
        assert!(!record.is_null());
        unsafe {
            synui_error_record_free(record);
            synui_error_record_free(record);
            synui_error_record_free(ptr::null_mut());
            synui_error_record_free(TOKEN_ALIGNMENT as *mut SynuiErrorRecord);
        }
    }

    #[test]
    fn placeholder_handle_apis_are_null_safe_and_reject_unknown_tokens() {
        let mut error = ptr::null_mut();
        let status = unsafe { synui_window_release(ptr::null_mut(), &mut error) };
        assert_eq!(status, SYNUI_OK);
        unsafe { assert_record(error, status) };

        let unknown = (TOKEN_ALIGNMENT * 999_999) as *mut SynuiWindow;
        let status = unsafe { synui_window_release(unknown, &mut error) };
        assert_eq!(status, SYNUI_STALE_HANDLE);
        unsafe { assert_record(error, status) };
    }

    unsafe fn create_app(runtime: *mut SynuiRuntime) -> *mut SynuiApp {
        let mut app = ptr::null_mut();
        assert_eq!(
            unsafe { synui_app_create(runtime, &mut app, ptr::null_mut()) },
            SYNUI_OK
        );
        app
    }

    unsafe fn query_post(token: *mut SynuiPostToken) -> SynuiPostTokenInfo {
        let mut info = SynuiPostTokenInfo {
            struct_size: SynuiPostTokenInfo::STRUCT_SIZE,
            state: u32::MAX,
            callback_status: u32::MAX,
        };
        assert_eq!(
            unsafe { synui_post_token_query(token, &mut info, ptr::null_mut()) },
            SYNUI_OK
        );
        info
    }

    unsafe extern "C-unwind" fn record_payload(
        user_data: *mut c_void,
        payload: *const SynuiCallbackPayload,
    ) -> SynuiStatus {
        let output = unsafe { &mut *user_data.cast::<(ThreadId, Vec<u8>)>() };
        output.0 = thread::current().id();
        let bytes = unsafe { &(*payload).bytes };
        output.1 = if bytes.len == 0 {
            Vec::new()
        } else {
            unsafe { core::slice::from_raw_parts(bytes.ptr, bytes.len) }.to_vec()
        };
        SYNUI_OK
    }

    unsafe extern "C-unwind" fn no_op_callback(
        _user_data: *mut c_void,
        _payload: *const SynuiCallbackPayload,
    ) -> SynuiStatus {
        SYNUI_OK
    }

    // Feature: synui-api-abi-reset, Property 10: 线程门、post 与销毁队列安全
    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 100,
            max_shrink_iters: 256,
            .. ProptestConfig::default()
        })]
        #[test]
        fn thread_gate_post_and_app_close_never_run_work_after_destroy(
            operations in prop::collection::vec(0u8..6, 1..=32),
        ) {
            let runtime = unsafe { create_runtime() };
            let app = unsafe { create_app(runtime) };
            let app_address = app as usize;
            let mut tokens = Vec::<*mut SynuiPostToken>::new();
            let mut app_live = true;

            for operation in operations {
                match operation {
                    0 if app_live => {
                        let mut drained = usize::MAX;
                        let status = unsafe {
                            synui_app_drain_posts(app, &mut drained, ptr::null_mut())
                        };
                        prop_assert_eq!(status, SYNUI_OK);
                        prop_assert!(drained != usize::MAX);
                    }
                    1 => {
                        let result = thread::spawn(move || {
                            let mut drained = usize::MAX;
                            let status = unsafe {
                                synui_app_drain_posts(
                                    app_address as *const SynuiApp,
                                    &mut drained,
                                    ptr::null_mut(),
                                )
                            };
                            (status, drained)
                        })
                        .join()
                        .unwrap();
                        if app_live {
                            prop_assert_eq!(result.0, SYNUI_THREAD_AFFINITY);
                        } else {
                            prop_assert_eq!(result.0, SYNUI_STALE_HANDLE);
                        }
                        prop_assert_eq!(result.1, usize::MAX);
                    }
                    2 => {
                        let result = thread::spawn(move || {
                            let work = SynuiPost {
                                struct_size: SynuiPost::STRUCT_SIZE,
                                callback: Some(no_op_callback),
                                user_data: ptr::null_mut(),
                                payload: SynuiBytesView {
                                    ptr: ptr::null(),
                                    len: 0,
                                },
                            };
                            let mut token = ptr::null_mut();
                            let status = unsafe {
                                synui_app_post(
                                    app_address as *const SynuiApp,
                                    &work,
                                    &mut token,
                                    ptr::null_mut(),
                                )
                            };
                            (status, token as usize)
                        })
                        .join()
                        .unwrap();
                        if result.0 == SYNUI_OK {
                            prop_assert_ne!(result.1, 0);
                            tokens.push(result.1 as *mut SynuiPostToken);
                        } else {
                            prop_assert_eq!(result.1, 0);
                        }
                    }
                    3 if !tokens.is_empty() => {
                        let index = (tokens.len() - 1) % tokens.len();
                        let status = unsafe {
                            synui_post_token_cancel(tokens[index], ptr::null_mut())
                        };
                        prop_assert!(
                            status == SYNUI_OK
                                || status == SYNUI_POST_CANCELLED
                                || status == SYNUI_CALLBACK_ERROR
                        );
                    }
                    4 if !tokens.is_empty() => {
                        let token = tokens[(tokens.len() - 1) % tokens.len()];
                        let info = unsafe { query_post(token) };
                        prop_assert!(
                            info.state == SYNUI_POST_ACCEPTED
                                || info.state == SYNUI_POST_EXECUTED
                                || info.state == SYNUI_POST_CANCELLED_STATE
                                || info.state == SYNUI_POST_FAILED
                        );
                    }
                    5 if app_live => {
                        let status = unsafe { synui_app_release(app, ptr::null_mut()) };
                        prop_assert_eq!(status, SYNUI_OK);
                        app_live = false;
                    }
                    _ => {}
                }
            }

            if app_live {
                let status = unsafe { synui_app_release(app, ptr::null_mut()) };
                prop_assert_eq!(status, SYNUI_OK);
            }
            for token in tokens {
                let info = unsafe { query_post(token) };
                prop_assert!(info.state != SYNUI_POST_RUNNING);
                prop_assert_eq!(
                    unsafe { synui_post_token_release(token, ptr::null_mut()) },
                    SYNUI_OK
                );
            }
            prop_assert_eq!(
                unsafe { synui_runtime_release(runtime, ptr::null_mut()) },
                SYNUI_OK
            );
        }
    }

    #[test]
    fn ffi_guard_contains_panics_and_returns_a_matching_record() {
        let mut error = ptr::null_mut();
        let status = ffi_guard(&mut error, || panic!("guard test"));
        assert_eq!(status, SYNUI_PANIC_CONTAINED);
        unsafe { assert_record(error, status) };
        ffi_guard_void(|| panic!("void guard test"));
        unsafe { synui_error_record_free(fallback_error_record()) };
    }

    #[test]
    fn worker_post_is_copied_and_only_ui_thread_drain_mutates_queue() {
        let runtime = unsafe { create_runtime() };
        let app = unsafe { create_app(runtime) };
        let ui_thread = thread::current().id();
        let mut observed = (ui_thread, Vec::<u8>::new());
        let observed_ptr = ptr::from_mut(&mut observed) as usize;
        let app_address = app as usize;
        let (worker_status, token_address, affinity_status, untouched) = thread::spawn(move || {
            let mut source = [1_u8, 2, 3, 4];
            let work = SynuiPost {
                struct_size: SynuiPost::STRUCT_SIZE,
                callback: Some(record_payload),
                user_data: observed_ptr as *mut c_void,
                payload: SynuiBytesView {
                    ptr: source.as_ptr(),
                    len: source.len(),
                },
            };
            let mut token = ptr::null_mut();
            let status = unsafe {
                synui_app_post(
                    app_address as *const SynuiApp,
                    &work,
                    &mut token,
                    ptr::null_mut(),
                )
            };
            source.fill(9);
            let mut drained = usize::MAX;
            let affinity = unsafe {
                synui_app_drain_posts(
                    app_address as *const SynuiApp,
                    &mut drained,
                    ptr::null_mut(),
                )
            };
            (status, token as usize, affinity, drained)
        })
        .join()
        .unwrap();
        assert_eq!(worker_status, SYNUI_OK);
        assert_eq!(affinity_status, SYNUI_THREAD_AFFINITY);
        assert_eq!(untouched, usize::MAX);
        let token = token_address as *mut SynuiPostToken;
        assert_eq!(unsafe { query_post(token) }.state, SYNUI_POST_ACCEPTED);
        let mut drained = 0;
        assert_eq!(
            unsafe { synui_app_drain_posts(app, &mut drained, ptr::null_mut()) },
            SYNUI_OK
        );
        assert_eq!(drained, 1);
        assert_eq!(observed.0, ui_thread);
        assert_eq!(observed.1, [1, 2, 3, 4]);
        assert_eq!(unsafe { query_post(token) }.state, SYNUI_POST_EXECUTED);
        assert_eq!(
            unsafe { synui_post_token_release(token, ptr::null_mut()) },
            SYNUI_OK
        );
        assert_eq!(unsafe { synui_app_release(app, ptr::null_mut()) }, SYNUI_OK);
        assert_eq!(
            unsafe { synui_runtime_release(runtime, ptr::null_mut()) },
            SYNUI_OK
        );
    }

    #[test]
    fn cancel_and_app_close_publish_cancelled_terminal_state() {
        let runtime = unsafe { create_runtime() };
        let app = unsafe { create_app(runtime) };
        let work = SynuiPost {
            struct_size: SynuiPost::STRUCT_SIZE,
            callback: Some(no_op_callback),
            user_data: ptr::null_mut(),
            payload: SynuiBytesView {
                ptr: ptr::null(),
                len: 0,
            },
        };
        let mut cancelled = ptr::null_mut();
        assert_eq!(
            unsafe { synui_app_post(app, &work, &mut cancelled, ptr::null_mut()) },
            SYNUI_OK
        );
        assert_eq!(
            unsafe { synui_post_token_cancel(cancelled, ptr::null_mut()) },
            SYNUI_OK
        );
        let info = unsafe { query_post(cancelled) };
        assert_eq!(info.state, SYNUI_POST_CANCELLED_STATE);
        assert_eq!(info.callback_status, SYNUI_POST_CANCELLED);

        let mut closed = ptr::null_mut();
        assert_eq!(
            unsafe { synui_app_post(app, &work, &mut closed, ptr::null_mut()) },
            SYNUI_OK
        );
        assert_eq!(unsafe { synui_app_release(app, ptr::null_mut()) }, SYNUI_OK);
        let info = unsafe { query_post(closed) };
        assert_eq!(info.state, SYNUI_POST_CANCELLED_STATE);
        assert_eq!(info.callback_status, SYNUI_POST_CANCELLED);
        assert_eq!(
            unsafe { synui_post_token_release(cancelled, ptr::null_mut()) },
            SYNUI_OK
        );
        assert_eq!(
            unsafe { synui_post_token_release(closed, ptr::null_mut()) },
            SYNUI_OK
        );
        assert_eq!(
            unsafe { synui_runtime_release(runtime, ptr::null_mut()) },
            SYNUI_OK
        );
    }

    #[test]
    fn releasing_caller_token_does_not_cancel_queued_work() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        unsafe extern "C-unwind" fn increment(
            user_data: *mut c_void,
            _payload: *const SynuiCallbackPayload,
        ) -> SynuiStatus {
            unsafe { &*user_data.cast::<AtomicUsize>() }.fetch_add(1, Ordering::SeqCst);
            SYNUI_OK
        }
        let runtime = unsafe { create_runtime() };
        let app = unsafe { create_app(runtime) };
        let calls = AtomicUsize::new(0);
        let work = SynuiPost {
            struct_size: SynuiPost::STRUCT_SIZE,
            callback: Some(increment),
            user_data: ptr::from_ref(&calls).cast_mut().cast(),
            payload: SynuiBytesView {
                ptr: ptr::null(),
                len: 0,
            },
        };
        let mut token = ptr::null_mut();
        assert_eq!(
            unsafe { synui_app_post(app, &work, &mut token, ptr::null_mut()) },
            SYNUI_OK
        );
        assert_eq!(
            unsafe { synui_post_token_release(token, ptr::null_mut()) },
            SYNUI_OK
        );
        let mut drained = 0;
        assert_eq!(
            unsafe { synui_app_drain_posts(app, &mut drained, ptr::null_mut()) },
            SYNUI_OK
        );
        assert_eq!(drained, 1);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(unsafe { synui_app_release(app, ptr::null_mut()) }, SYNUI_OK);
        assert_eq!(
            unsafe { synui_runtime_release(runtime, ptr::null_mut()) },
            SYNUI_OK
        );
    }

    struct ReentrantContext {
        app: *mut SynuiApp,
        drain_status: SynuiStatus,
        release_status: SynuiStatus,
        post_status: SynuiStatus,
    }

    unsafe extern "C-unwind" fn reentrant_callback(
        user_data: *mut c_void,
        _payload: *const SynuiCallbackPayload,
    ) -> SynuiStatus {
        let context = unsafe { &mut *user_data.cast::<ReentrantContext>() };
        let mut drained = usize::MAX;
        context.drain_status =
            unsafe { synui_app_drain_posts(context.app, &mut drained, ptr::null_mut()) };
        context.release_status = unsafe { synui_app_release(context.app, ptr::null_mut()) };
        let work = SynuiPost {
            struct_size: SynuiPost::STRUCT_SIZE,
            callback: Some(no_op_callback),
            user_data: ptr::null_mut(),
            payload: SynuiBytesView {
                ptr: ptr::null(),
                len: 0,
            },
        };
        let mut token = ptr::null_mut();
        context.post_status =
            unsafe { synui_app_post(context.app, &work, &mut token, ptr::null_mut()) };
        if context.post_status == SYNUI_OK {
            let _ = unsafe { synui_post_token_release(token, ptr::null_mut()) };
        }
        SYNUI_OK
    }

    #[test]
    fn callback_reentrancy_rejects_drain_and_final_app_release_but_allows_post() {
        let runtime = unsafe { create_runtime() };
        let app = unsafe { create_app(runtime) };
        let mut context = ReentrantContext {
            app,
            drain_status: SYNUI_OK,
            release_status: SYNUI_OK,
            post_status: SYNUI_INTERNAL_ERROR,
        };
        let work = SynuiPost {
            struct_size: SynuiPost::STRUCT_SIZE,
            callback: Some(reentrant_callback),
            user_data: ptr::from_mut(&mut context).cast(),
            payload: SynuiBytesView {
                ptr: ptr::null(),
                len: 0,
            },
        };
        let mut token = ptr::null_mut();
        assert_eq!(
            unsafe { synui_app_post(app, &work, &mut token, ptr::null_mut()) },
            SYNUI_OK
        );
        let mut drained = 0;
        assert_eq!(
            unsafe { synui_app_drain_posts(app, &mut drained, ptr::null_mut()) },
            SYNUI_OK
        );
        assert_eq!(context.drain_status, SYNUI_CALLBACK_ERROR);
        assert_eq!(context.release_status, SYNUI_CALLBACK_ERROR);
        assert_eq!(context.post_status, SYNUI_OK);
        assert_eq!(unsafe { query_post(token) }.state, SYNUI_POST_EXECUTED);
        assert_eq!(
            unsafe { synui_app_drain_posts(app, &mut drained, ptr::null_mut()) },
            SYNUI_OK
        );
        assert_eq!(drained, 1);
        assert_eq!(
            unsafe { synui_post_token_release(token, ptr::null_mut()) },
            SYNUI_OK
        );
        assert_eq!(unsafe { synui_app_release(app, ptr::null_mut()) }, SYNUI_OK);
        assert_eq!(
            unsafe { synui_runtime_release(runtime, ptr::null_mut()) },
            SYNUI_OK
        );
    }

    #[test]
    fn callback_error_and_panic_become_failed_token_states() {
        unsafe extern "C-unwind" fn error_callback(
            _user_data: *mut c_void,
            _payload: *const SynuiCallbackPayload,
        ) -> SynuiStatus {
            SYNUI_INVALID_ARGUMENT
        }
        unsafe extern "C-unwind" fn panic_callback(
            _user_data: *mut c_void,
            _payload: *const SynuiCallbackPayload,
        ) -> SynuiStatus {
            panic!("callback panic")
        }
        let runtime = unsafe { create_runtime() };
        let app = unsafe { create_app(runtime) };
        for (callback, expected_drain, expected_callback) in [
            (
                error_callback as SynuiPostCallback,
                SYNUI_CALLBACK_ERROR,
                SYNUI_INVALID_ARGUMENT,
            ),
            (
                panic_callback as SynuiPostCallback,
                SYNUI_PANIC_CONTAINED,
                SYNUI_PANIC_CONTAINED,
            ),
        ] {
            let work = SynuiPost {
                struct_size: SynuiPost::STRUCT_SIZE,
                callback: Some(callback),
                user_data: ptr::null_mut(),
                payload: SynuiBytesView {
                    ptr: ptr::null(),
                    len: 0,
                },
            };
            let mut token = ptr::null_mut();
            assert_eq!(
                unsafe { synui_app_post(app, &work, &mut token, ptr::null_mut()) },
                SYNUI_OK
            );
            let mut drained = 0;
            assert_eq!(
                unsafe { synui_app_drain_posts(app, &mut drained, ptr::null_mut()) },
                expected_drain
            );
            let info = unsafe { query_post(token) };
            assert_eq!(info.state, SYNUI_POST_FAILED);
            assert_eq!(info.callback_status, expected_callback);
            assert_eq!(
                unsafe { synui_post_token_release(token, ptr::null_mut()) },
                SYNUI_OK
            );
        }
        assert_eq!(unsafe { synui_app_release(app, ptr::null_mut()) }, SYNUI_OK);
        assert_eq!(
            unsafe { synui_runtime_release(runtime, ptr::null_mut()) },
            SYNUI_OK
        );
    }
}
