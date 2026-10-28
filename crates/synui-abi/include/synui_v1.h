#ifndef SYNUI_V1_H
#define SYNUI_V1_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#if defined(_WIN32)
#define SYNUI_ABI_EXPORT __declspec(dllimport)
#else
#define SYNUI_ABI_EXPORT
#endif

/*
 * Opaque lease tokens: never construct, dereference, or perform arithmetic.
 * Each value is the address of a private allocation used only as a registry
 * capability; released allocations and type tombstones are retained until the
 * library is unloaded so stale raw values cannot be recycled as new handles.
 */
typedef struct synui_runtime synui_runtime_t;
typedef struct synui_app synui_app_t;
typedef struct synui_state synui_state_t;
typedef struct synui_window synui_window_t;
typedef struct synui_element synui_element_t;
typedef struct synui_model synui_model_t;
typedef struct synui_callback synui_callback_t;
typedef struct synui_post_token synui_post_token_t;

typedef struct synui_abi_version {
    uint32_t major;
    uint32_t minor;
} synui_abi_version_t;
#define SYNUI_ABI_VERSION_MAJOR UINT32_C(1)
#define SYNUI_ABI_VERSION_MINOR UINT32_C(1)

typedef uint64_t synui_capabilities_t;
#define SYNUI_CAPABILITY_BASE (UINT64_C(1) << 0)
#define SYNUI_CAPABILITY_VIRTUAL_MODEL (UINT64_C(1) << 1)
#define SYNUI_CAPABILITIES_SUPPORTED \
    (SYNUI_CAPABILITY_BASE | SYNUI_CAPABILITY_VIRTUAL_MODEL)

/* Fixed-width integer status; unknown input values remain safe integers. */
typedef uint32_t synui_status_t;
#define SYNUI_OK UINT32_C(0)
#define SYNUI_UNSUPPORTED_ABI UINT32_C(1)
#define SYNUI_UNSUPPORTED_CAPABILITY UINT32_C(2)
#define SYNUI_INVALID_ARGUMENT UINT32_C(3)
#define SYNUI_CONTEXT_MISMATCH UINT32_C(4)
#define SYNUI_CONTEXT_DESTROYED UINT32_C(5)
#define SYNUI_STALE_HANDLE UINT32_C(6)
#define SYNUI_THREAD_AFFINITY UINT32_C(7)
#define SYNUI_POST_CANCELLED UINT32_C(8)
#define SYNUI_CALLBACK_ERROR UINT32_C(9)
#define SYNUI_PANIC_CONTAINED UINT32_C(10)
#define SYNUI_INTERNAL_ERROR UINT32_C(11)


/* Borrowed UTF-8, not NUL terminated. NULL is valid exactly when len is 0. */
typedef struct synui_text_view {
    const uint8_t *ptr;
    size_t len;
} synui_text_view_t;

/* Borrowed uninterpreted bytes. NULL is valid exactly when len is 0. */
typedef struct synui_bytes_view {
    const uint8_t *ptr;
    size_t len;
} synui_bytes_view_t;

/* Extensible structures begin with struct_size; unknown tails are preserved. */
typedef struct synui_abi_info {
    uint32_t struct_size;
    synui_abi_version_t version;
    synui_capabilities_t capabilities;
} synui_abi_info_t;

typedef struct synui_runtime_options {
    uint32_t struct_size;
    synui_abi_version_t requested_version;
    synui_capabilities_t requested_capabilities;
} synui_runtime_options_t;

typedef struct synui_runtime_info {
    uint32_t struct_size;
    synui_abi_version_t version;
    synui_capabilities_t capabilities;
    uint64_t resource_slot;
    uint64_t generation;
    uint64_t reference_count;
} synui_runtime_info_t;

typedef struct synui_error_record {
    uint32_t struct_size;
    synui_status_t status;
    uint32_t code; /* Equal to status in ABI v1. */
    synui_text_view_t message;
    synui_text_view_t detail;
} synui_error_record_t;

/* ABI-owned bytes, not NUL-terminated. Release with synui_buffer_free. */
typedef struct synui_buffer {
    uint32_t struct_size;
    uint8_t *ptr;
    size_t len;
    size_t capacity;
} synui_buffer_t;

typedef struct synui_callback_payload {
    uint32_t struct_size;
    uint32_t kind;
    synui_bytes_view_t bytes;
} synui_callback_payload_t;

typedef synui_status_t (*synui_action_callback_t)(void *user_data);
typedef synui_status_t (*synui_post_callback_t)(
    void *user_data, const synui_callback_payload_t *payload);

typedef struct synui_post {
    uint32_t struct_size;
    synui_post_callback_t callback;
    void *user_data;
    synui_bytes_view_t payload;
} synui_post_t;

#define SYNUI_POST_ACCEPTED UINT32_C(0)
#define SYNUI_POST_RUNNING UINT32_C(1)
#define SYNUI_POST_EXECUTED UINT32_C(2)
#define SYNUI_POST_CANCELLED_STATE UINT32_C(3)
#define SYNUI_POST_FAILED UINT32_C(4)

typedef struct synui_post_token_info {
    uint32_t struct_size;
    uint32_t state;
    synui_status_t callback_status;
} synui_post_token_info_t;

/* Virtual DataTable values use fixed-width coordinates and half-open ranges. */
typedef struct synui_table_dimensions {
    uint32_t struct_size;
    uint64_t rows;
    uint64_t columns;
} synui_table_dimensions_t;

typedef struct synui_cell_range {
    uint32_t struct_size;
    uint64_t row_start;
    uint64_t row_end;
    uint64_t column_start;
    uint64_t column_end;
} synui_cell_range_t;

typedef struct synui_cell_request {
    uint32_t struct_size;
    uint64_t request_id;
    synui_cell_range_t range;
} synui_cell_request_t;

typedef struct synui_cell_coordinate {
    uint32_t struct_size;
    uint64_t row;
    uint64_t column;
} synui_cell_coordinate_t;

#define SYNUI_CELL_EMPTY UINT32_C(0)
#define SYNUI_CELL_TEXT UINT32_C(1)
#define SYNUI_CELL_BOOL UINT32_C(2)
#define SYNUI_CELL_NUMBER UINT32_C(3)
typedef struct synui_cell_value {
    uint32_t struct_size;
    uint32_t kind;
    synui_text_view_t text;
    uint32_t boolean;
    double number;
} synui_cell_value_t;

#define SYNUI_SORT_ASCENDING UINT32_C(0)
#define SYNUI_SORT_DESCENDING UINT32_C(1)
typedef struct synui_sort_descriptor {
    uint32_t struct_size;
    uint64_t column;
    uint32_t direction;
} synui_sort_descriptor_t;

typedef struct synui_prefetch_band {
    uint32_t struct_size;
    uint64_t rows_before;
    uint64_t rows_after;
    uint64_t columns_before;
    uint64_t columns_after;
} synui_prefetch_band_t;

typedef synui_status_t (*synui_model_dimensions_callback_t)(
    void *user_data, synui_table_dimensions_t *out_dimensions);
typedef synui_status_t (*synui_model_request_cells_callback_t)(
    void *user_data, const synui_cell_request_t *request);
typedef synui_status_t (*synui_model_apply_cell_update_callback_t)(
    void *user_data, const synui_cell_coordinate_t *coordinate,
    const synui_cell_value_t *value);
typedef synui_status_t (*synui_model_sort_changed_callback_t)(
    void *user_data, const synui_sort_descriptor_t *sort, size_t sort_len);
typedef synui_status_t (*synui_model_selection_changed_callback_t)(
    void *user_data, const synui_cell_range_t *ranges, size_t ranges_len);

typedef struct synui_virtual_data_table_model_vtable {
    uint32_t struct_size;
    synui_model_dimensions_callback_t dimensions;
    synui_model_request_cells_callback_t request_cells;
    synui_model_apply_cell_update_callback_t apply_cell_update;
    synui_model_sort_changed_callback_t sort_changed;
    synui_model_selection_changed_callback_t selection_changed;
} synui_virtual_data_table_model_vtable_t;

/*
 * Error contract for every status-returning function below:
 * - out_error may be NULL. Otherwise it must be aligned writable storage and
 *   must not overlap another output object used by the same call.
 * - success stores an ABI-owned SYNUI_OK record with empty message and detail.
 * - failure stores an ABI-owned record whose status and code equal the return.
 * - release each returned record exactly once with synui_error_record_free.
 * No record can be returned when out_error itself is invalid or overlapping.
 */
SYNUI_ABI_EXPORT void synui_error_record_free(synui_error_record_t *record);

/*
 * Releases bytes returned in synui_buffer_t. The buffer is passed by value so
 * NULL/zero buffers and unknown payload pointers can be ignored without
 * dereferencing caller memory. A returned non-empty buffer must be freed once.
 */
SYNUI_ABI_EXPORT void synui_buffer_free(synui_buffer_t buffer);


/*
 * Negotiates ABI v1 without creating runtime resources. out_info is required,
 * aligned, writable, and initialized with struct_size >= sizeof(*out_info).
 * It remains unchanged on failure. Error Record allocation is diagnostic only.
 */
SYNUI_ABI_EXPORT synui_status_t synui_abi_query(
    synui_abi_version_t requested,
    synui_capabilities_t requested_capabilities,
    synui_abi_info_t *out_info,
    synui_error_record_t **out_error);

/*
 * Creates one owned runtime lease. options and out_runtime are required and
 * aligned; options->struct_size must cover synui_runtime_options_t. Unsupported
 * negotiation creates no runtime and leaves out_runtime unchanged.
 */
SYNUI_ABI_EXPORT synui_status_t synui_runtime_create(
    const synui_runtime_options_t *options,
    synui_runtime_t **out_runtime,
    synui_error_record_t **out_error);

/* out_info is required, aligned, writable, and size-initialized. */
SYNUI_ABI_EXPORT synui_status_t synui_runtime_query(
    const synui_runtime_t *runtime,
    synui_runtime_info_t *out_info,
    synui_error_record_t **out_error);

/*
 * Creates an App whose UI thread is the calling thread. The runtime must be
 * live; the App owns an independent lease and is released with
 * synui_app_release.
 */
SYNUI_ABI_EXPORT synui_status_t synui_app_create(
    const synui_runtime_t *runtime,
    synui_app_t **out_app,
    synui_error_record_t **out_error);

/* Context-owned signed 64-bit State; direct access is UI-thread-only. */
SYNUI_ABI_EXPORT synui_status_t synui_state_i64_create(
    const synui_app_t *app, int64_t initial,
    synui_state_t **out_state, synui_error_record_t **out_error);
SYNUI_ABI_EXPORT synui_status_t synui_state_i64_get(
    const synui_state_t *state, int64_t *out_value,
    synui_error_record_t **out_error);
SYNUI_ABI_EXPORT synui_status_t synui_state_i64_set(
    const synui_state_t *state, int64_t value,
    synui_error_record_t **out_error);

/* Copies a UI-thread Action trampoline into the creating App Context. */
SYNUI_ABI_EXPORT synui_status_t synui_callback_create(
    const synui_app_t *app, synui_action_callback_t callback, void *user_data,
    synui_callback_t **out_callback, synui_error_record_t **out_error);

/*
 * Creates retained facade Text/Buttons without exposing Engine layouts.
 * Text input is copied synchronously and must be valid UTF-8. set_text changes
 * the pending value; the rendered snapshot changes only after a successful
 * synui_app_render containing that Text Element.
 */
SYNUI_ABI_EXPORT synui_status_t synui_text_element_create(
    const synui_app_t *app, synui_text_view_t text,
    synui_element_t **out_element, synui_error_record_t **out_error);
SYNUI_ABI_EXPORT synui_status_t synui_text_element_set_text(
    const synui_element_t *element, synui_text_view_t text,
    synui_error_record_t **out_error);
SYNUI_ABI_EXPORT synui_status_t synui_button_element_create(
    const synui_app_t *app, const synui_callback_t *callback,
    synui_element_t **out_element, synui_error_record_t **out_error);

/*
 * Renders one retained facade root whose children are the supplied Elements.
 * Button dispatch may change application State, while pending Element values
 * become rendered snapshots only in this explicit render batch.
 */
SYNUI_ABI_EXPORT synui_status_t synui_app_render(
    const synui_app_t *app, const synui_element_t *const *elements,
    size_t elements_len, synui_error_record_t **out_error);
SYNUI_ABI_EXPORT synui_status_t synui_element_press(
    const synui_element_t *element, synui_error_record_t **out_error);

/*
 * Copies the last successfully rendered UTF-8 snapshot into an ABI-owned
 * buffer. out_text must be zero-initialized except for struct_size, remains
 * unchanged on failure, and must be released with synui_buffer_free on success.
 */
SYNUI_ABI_EXPORT synui_status_t synui_text_element_get_rendered_text(
    const synui_element_t *element, synui_buffer_t *out_text,
    synui_error_record_t **out_error);

/*
 * May be called from any thread. work->struct_size must cover synui_post_t,
 * callback must be non-NULL, and payload follows the NULL/len bytes-view rule.
 * SynUI copies payload bytes before accepting the post. out_token receives a
 * caller-owned lease; the queue owns a separate internal reference, so
 * releasing the caller token does not cancel work.
 */
SYNUI_ABI_EXPORT synui_status_t synui_app_post(
    const synui_app_t *app,
    const synui_post_t *work,
    synui_post_token_t **out_token,
    synui_error_record_t **out_error);

/*
 * Executes the queue snapshot on the App UI thread and writes the number of
 * callbacks attempted. A non-UI call returns SYNUI_THREAD_AFFINITY without
 * changing the queue or out_drained. Synchronous reentrant drain from a post
 * callback returns SYNUI_CALLBACK_ERROR. Work posted by a callback remains for
 * a later drain. Callback payload bytes are valid only until callback return.
 * A callback non-OK result produces FAILED/SYNUI_CALLBACK_ERROR; a contained
 * Rust unwind produces FAILED/SYNUI_PANIC_CONTAINED.
 */
SYNUI_ABI_EXPORT synui_status_t synui_app_drain_posts(
    const synui_app_t *app,
    size_t *out_drained,
    synui_error_record_t **out_error);

/* out_info must be size-initialized and remains unchanged on failure. */
SYNUI_ABI_EXPORT synui_status_t synui_post_token_query(
    const synui_post_token_t *token,
    synui_post_token_info_t *out_info,
    synui_error_record_t **out_error);

/* Cancels ACCEPTED work. Other terminal/running states are not changed. */
SYNUI_ABI_EXPORT synui_status_t synui_post_token_cancel(
    const synui_post_token_t *token,
    synui_error_record_t **out_error);

/*
 * Creates a Context-owned virtual model. The vtable is copied synchronously;
 * user_data remains caller-owned and must outlive every retained model lease.
 * All callbacks run on the App UI thread without the handle-registry lock.
 */
SYNUI_ABI_EXPORT synui_status_t synui_virtual_data_table_model_create(
    const synui_app_t *app,
    const synui_virtual_data_table_model_vtable_t *vtable,
    void *user_data,
    synui_model_t **out_model,
    synui_error_record_t **out_error);

/* Computes a clipped viewport+prefetch request through the public adapter. */
SYNUI_ABI_EXPORT synui_status_t synui_virtual_data_table_model_request_visible(
    const synui_model_t *model,
    synui_cell_range_t viewport,
    synui_prefetch_band_t prefetch,
    uint32_t *out_requested,
    synui_cell_request_t *out_request,
    synui_error_record_t **out_error);

SYNUI_ABI_EXPORT synui_status_t synui_virtual_data_table_model_apply_cell_update(
    const synui_model_t *model,
    const synui_cell_coordinate_t *coordinate,
    const synui_cell_value_t *value,
    synui_error_record_t **out_error);
SYNUI_ABI_EXPORT synui_status_t synui_virtual_data_table_model_notify_sort(
    const synui_model_t *model,
    const synui_sort_descriptor_t *sort,
    size_t sort_len,
    synui_error_record_t **out_error);
SYNUI_ABI_EXPORT synui_status_t synui_virtual_data_table_model_notify_selection(
    const synui_model_t *model,
    const synui_cell_range_t *ranges,
    size_t ranges_len,
    synui_error_record_t **out_error);

/*
 * retain validates type and generation, then creates a distinct owned token.
 * release consumes exactly that token. NULL release succeeds; repeated release
 * returns SYNUI_STALE_HANDLE; a live/well-known wrong type returns
 * SYNUI_INVALID_ARGUMENT. Implementations never dereference handle tokens.
 * Safety trade-off: every issued token allocation and compact type tombstone is
 * retained until library unload, so registry memory grows with issued leases.
 */
#define SYNUI_DECLARE_HANDLE_OWNERSHIP(prefix, type) \
    SYNUI_ABI_EXPORT synui_status_t prefix##_retain( \
        const type *handle, type **out_retained, synui_error_record_t **out_error); \
    SYNUI_ABI_EXPORT synui_status_t prefix##_release( \
        type *handle, synui_error_record_t **out_error)

SYNUI_DECLARE_HANDLE_OWNERSHIP(synui_runtime, synui_runtime_t);
SYNUI_DECLARE_HANDLE_OWNERSHIP(synui_app, synui_app_t);
SYNUI_DECLARE_HANDLE_OWNERSHIP(synui_state, synui_state_t);
SYNUI_DECLARE_HANDLE_OWNERSHIP(synui_window, synui_window_t);
SYNUI_DECLARE_HANDLE_OWNERSHIP(synui_element, synui_element_t);
SYNUI_DECLARE_HANDLE_OWNERSHIP(synui_model, synui_model_t);
SYNUI_DECLARE_HANDLE_OWNERSHIP(synui_callback, synui_callback_t);
SYNUI_DECLARE_HANDLE_OWNERSHIP(synui_post_token, synui_post_token_t);
#undef SYNUI_DECLARE_HANDLE_OWNERSHIP

/*
 * Lifecycle summary (full machine-readable table: abi/synui_v1.lifecycle.toml)
 * runtime: create -> caller-owned lease -> retain/query -> release.
 * app: create on UI thread -> post from any thread -> drain on UI thread ->
 * release. Releasing the final App lease closes it and cancels ACCEPTED posts;
 * final release during callback is rejected with SYNUI_CALLBACK_ERROR.
 * post token: caller lease plus queue internal reference; query/cancel/release.
 * callback is invoked without registry/error locks. Its payload is borrowed
 * only for the callback dynamic extent. post from callback is allowed;
 * synchronous drain and final App release are rejected.
 * model: virtual_model capability creates a Context-owned vtable adapter;
 * callbacks run on the App UI thread without registry locks, and viewport
 * requests are clipped by the public VirtualDataTableAdapter.
 * state/callback: Context-owned Counter resources remain typed and UI-thread
 * gated. Text Elements own copied UTF-8 pending/rendered values; ABI buffers
 * returned for rendered text are caller-owned until synui_buffer_free.
 * window ownership APIs remain a safe placeholder.
 * Error Record: each requested command result -> caller -> exactly one free;
 * the shared static INTERNAL_ERROR fallback accepts free as a no-op.
 * Handle validity requires matching type, slot, and generation. Every exported
 * entrypoint contains Rust panic; process abort/foreign longjmp are unsupported.
 */

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* SYNUI_V1_H */