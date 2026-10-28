"""cffi declarations derived from the published synui_v1.h ABI header."""

import ctypes

_SIZE_T = (
    "unsigned long"
    if ctypes.sizeof(ctypes.c_size_t) == ctypes.sizeof(ctypes.c_ulong)
    else "unsigned long long"
)

_CDEF_TEMPLATE = r"""
typedef unsigned int uint32_t;
typedef long long int64_t;
typedef unsigned long long uint64_t;
typedef @SIZE_T@ size_t;

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
typedef uint64_t synui_capabilities_t;
typedef uint32_t synui_status_t;

typedef struct synui_text_view {
    const unsigned char *ptr;
    size_t len;
} synui_text_view_t;
typedef struct synui_bytes_view {
    const unsigned char *ptr;
    size_t len;
} synui_bytes_view_t;

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
    uint32_t code;
    synui_text_view_t message;
    synui_text_view_t detail;
} synui_error_record_t;

typedef struct synui_buffer {
    uint32_t struct_size;
    unsigned char *ptr;
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

typedef struct synui_post_token_info {
    uint32_t struct_size;
    uint32_t state;
    synui_status_t callback_status;
} synui_post_token_info_t;

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
typedef struct synui_cell_value {
    uint32_t struct_size;
    uint32_t kind;
    synui_text_view_t text;
    uint32_t boolean;
    double number;
} synui_cell_value_t;
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

void synui_error_record_free(synui_error_record_t *record);
void synui_buffer_free(synui_buffer_t buffer);
synui_status_t synui_abi_query(
    synui_abi_version_t requested,
    synui_capabilities_t requested_capabilities,
    synui_abi_info_t *out_info,
    synui_error_record_t **out_error);
synui_status_t synui_runtime_create(
    const synui_runtime_options_t *options,
    synui_runtime_t **out_runtime,
    synui_error_record_t **out_error);
synui_status_t synui_runtime_query(
    const synui_runtime_t *runtime,
    synui_runtime_info_t *out_info,
    synui_error_record_t **out_error);
synui_status_t synui_app_create(
    const synui_runtime_t *runtime,
    synui_app_t **out_app,
    synui_error_record_t **out_error);
synui_status_t synui_state_i64_create(
    const synui_app_t *app, int64_t initial,
    synui_state_t **out_state, synui_error_record_t **out_error);
synui_status_t synui_state_i64_get(
    const synui_state_t *state, int64_t *out_value,
    synui_error_record_t **out_error);
synui_status_t synui_state_i64_set(
    const synui_state_t *state, int64_t value,
    synui_error_record_t **out_error);
synui_status_t synui_callback_create(
    const synui_app_t *app, synui_action_callback_t callback, void *user_data,
    synui_callback_t **out_callback, synui_error_record_t **out_error);
synui_status_t synui_text_element_create(
    const synui_app_t *app, synui_text_view_t text,
    synui_element_t **out_element, synui_error_record_t **out_error);
synui_status_t synui_text_element_set_text(
    const synui_element_t *element, synui_text_view_t text,
    synui_error_record_t **out_error);
synui_status_t synui_button_element_create(
    const synui_app_t *app, const synui_callback_t *callback,
    synui_element_t **out_element, synui_error_record_t **out_error);
synui_status_t synui_app_render(
    const synui_app_t *app, const synui_element_t *const *elements,
    size_t elements_len, synui_error_record_t **out_error);
synui_status_t synui_element_press(
    const synui_element_t *element, synui_error_record_t **out_error);
synui_status_t synui_text_element_get_rendered_text(
    const synui_element_t *element, synui_buffer_t *out_text,
    synui_error_record_t **out_error);
synui_status_t synui_app_post(
    const synui_app_t *app,
    const synui_post_t *work,
    synui_post_token_t **out_token,
    synui_error_record_t **out_error);
synui_status_t synui_app_drain_posts(
    const synui_app_t *app,
    size_t *out_drained,
    synui_error_record_t **out_error);
synui_status_t synui_post_token_query(
    const synui_post_token_t *token,
    synui_post_token_info_t *out_info,
    synui_error_record_t **out_error);
synui_status_t synui_post_token_cancel(
    const synui_post_token_t *token,
    synui_error_record_t **out_error);
synui_status_t synui_virtual_data_table_model_create(
    const synui_app_t *app,
    const synui_virtual_data_table_model_vtable_t *vtable,
    void *user_data,
    synui_model_t **out_model,
    synui_error_record_t **out_error);
synui_status_t synui_virtual_data_table_model_request_visible(
    const synui_model_t *model,
    synui_cell_range_t viewport,
    synui_prefetch_band_t prefetch,
    uint32_t *out_requested,
    synui_cell_request_t *out_request,
    synui_error_record_t **out_error);
synui_status_t synui_virtual_data_table_model_apply_cell_update(
    const synui_model_t *model,
    const synui_cell_coordinate_t *coordinate,
    const synui_cell_value_t *value,
    synui_error_record_t **out_error);
synui_status_t synui_virtual_data_table_model_notify_sort(
    const synui_model_t *model,
    const synui_sort_descriptor_t *sort,
    size_t sort_len,
    synui_error_record_t **out_error);
synui_status_t synui_virtual_data_table_model_notify_selection(
    const synui_model_t *model,
    const synui_cell_range_t *ranges,
    size_t ranges_len,
    synui_error_record_t **out_error);

synui_status_t synui_runtime_retain(
    const synui_runtime_t *handle, synui_runtime_t **out_retained,
    synui_error_record_t **out_error);
synui_status_t synui_runtime_release(
    synui_runtime_t *handle, synui_error_record_t **out_error);
synui_status_t synui_app_retain(
    const synui_app_t *handle, synui_app_t **out_retained,
    synui_error_record_t **out_error);
synui_status_t synui_app_release(
    synui_app_t *handle, synui_error_record_t **out_error);
synui_status_t synui_state_retain(
    const synui_state_t *handle, synui_state_t **out_retained,
    synui_error_record_t **out_error);
synui_status_t synui_state_release(
    synui_state_t *handle, synui_error_record_t **out_error);
synui_status_t synui_window_retain(
    const synui_window_t *handle, synui_window_t **out_retained,
    synui_error_record_t **out_error);
synui_status_t synui_window_release(
    synui_window_t *handle, synui_error_record_t **out_error);
synui_status_t synui_element_retain(
    const synui_element_t *handle, synui_element_t **out_retained,
    synui_error_record_t **out_error);
synui_status_t synui_element_release(
    synui_element_t *handle, synui_error_record_t **out_error);
synui_status_t synui_model_retain(
    const synui_model_t *handle, synui_model_t **out_retained,
    synui_error_record_t **out_error);
synui_status_t synui_model_release(
    synui_model_t *handle, synui_error_record_t **out_error);
synui_status_t synui_callback_retain(
    const synui_callback_t *handle, synui_callback_t **out_retained,
    synui_error_record_t **out_error);
synui_status_t synui_callback_release(
    synui_callback_t *handle, synui_error_record_t **out_error);
synui_status_t synui_post_token_retain(
    const synui_post_token_t *handle, synui_post_token_t **out_retained,
    synui_error_record_t **out_error);
synui_status_t synui_post_token_release(
    synui_post_token_t *handle, synui_error_record_t **out_error);
"""

SYNUI_V1_CDEF = _CDEF_TEMPLATE.replace("@SIZE_T@", _SIZE_T)
