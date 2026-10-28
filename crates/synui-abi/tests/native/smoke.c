#include "synui_v1.h"

#include <stddef.h>
#include <stdio.h>
#include <string.h>

static int failures = 0;

#define CHECK(condition)                                                        \
    do {                                                                        \
        if (!(condition)) {                                                     \
            fprintf(stderr, "C smoke check failed at line %d: %s\n",          \
                    __LINE__, #condition);                                      \
            failures += 1;                                                      \
        }                                                                       \
    } while (0)

static void check_record(synui_error_record_t *record,
                         synui_status_t expected) {
    CHECK(record != NULL);
    if (record == NULL) {
        return;
    }
    CHECK(record->struct_size == sizeof(*record));
    CHECK(record->status == expected);
    CHECK(record->code == expected);
    if (expected == SYNUI_OK) {
        CHECK(record->message.ptr == NULL);
        CHECK(record->message.len == 0);
        CHECK(record->detail.ptr == NULL);
        CHECK(record->detail.len == 0);
    } else {
        CHECK(record->message.ptr != NULL);
        CHECK(record->message.len != 0);
        CHECK(record->detail.ptr != NULL);
        CHECK(record->detail.len != 0);
    }
    synui_error_record_free(record);
}
int main(void) {
    synui_error_record_t *error = NULL;
    synui_abi_info_t info = {
        (uint32_t)sizeof(info),
        {0, 0},
        0,
    };
    synui_status_t status = synui_abi_query(
        (synui_abi_version_t){SYNUI_ABI_VERSION_MAJOR,
                              SYNUI_ABI_VERSION_MINOR},
        SYNUI_CAPABILITY_BASE, &info, &error);
    CHECK(status == SYNUI_OK);
    check_record(error, status);
    CHECK(info.version.major == SYNUI_ABI_VERSION_MAJOR);
    CHECK(info.version.minor == SYNUI_ABI_VERSION_MINOR);
    CHECK(info.capabilities == SYNUI_CAPABILITIES_SUPPORTED);

    synui_abi_info_t unchanged = {
        (uint32_t)sizeof(unchanged),
        {77, 88},
        UINT64_MAX,
    };
    synui_abi_info_t before = unchanged;
    status = synui_abi_query((synui_abi_version_t){99, 0}, 0,
                             &unchanged, &error);
    CHECK(status == SYNUI_UNSUPPORTED_ABI);
    CHECK(memcmp(&unchanged, &before, sizeof(before)) == 0);
    check_record(error, status);

    synui_runtime_options_t options = {
        (uint32_t)sizeof(options),
        {SYNUI_ABI_VERSION_MAJOR, SYNUI_ABI_VERSION_MINOR},
        SYNUI_CAPABILITY_BASE,
    };
    synui_runtime_t *runtime = NULL;
    status = synui_runtime_create(&options, &runtime, &error);
    CHECK(status == SYNUI_OK);
    CHECK(runtime != NULL);
    check_record(error, status);

    synui_runtime_t *retained = NULL;
    status = synui_runtime_retain(runtime, &retained, &error);
    CHECK(status == SYNUI_OK);
    CHECK(retained != NULL);
    CHECK(retained != runtime);
    check_record(error, status);

    synui_runtime_info_t runtime_info = {
        (uint32_t)sizeof(runtime_info),
        {0, 0},
        0,
        0,
        0,
        0,
    };
    status = synui_runtime_query(retained, &runtime_info, &error);
    CHECK(status == SYNUI_OK);
    CHECK(runtime_info.reference_count == 2);
    CHECK(runtime_info.generation != 0);
    check_record(error, status);

    synui_app_t *app = NULL;
    status = synui_app_create(retained, &app, &error);
    CHECK(status == SYNUI_OK);
    CHECK(app != NULL);
    check_record(error, status);

    const uint8_t initial_text[] = {'h', 'e', 'l', 'l', 'o'};
    synui_element_t *text = NULL;
    status = synui_text_element_create(
        app, (synui_text_view_t){initial_text, sizeof(initial_text)}, &text, &error);
    CHECK(status == SYNUI_OK);
    CHECK(text != NULL);
    check_record(error, status);

    synui_buffer_t rendered = {
        (uint32_t)sizeof(rendered),
        NULL,
        0,
        0,
    };
    status = synui_text_element_get_rendered_text(text, &rendered, &error);
    CHECK(status == SYNUI_OK);
    CHECK(rendered.ptr == NULL);
    CHECK(rendered.len == 0);
    check_record(error, status);
    synui_buffer_free(rendered);

    const synui_element_t *elements[] = {text};
    status = synui_app_render(app, elements, 1, &error);
    CHECK(status == SYNUI_OK);
    check_record(error, status);
    rendered = (synui_buffer_t){(uint32_t)sizeof(rendered), NULL, 0, 0};
    status = synui_text_element_get_rendered_text(text, &rendered, &error);
    CHECK(status == SYNUI_OK);
    CHECK(rendered.len == sizeof(initial_text));
    CHECK(memcmp(rendered.ptr, initial_text, rendered.len) == 0);
    check_record(error, status);
    synui_buffer_free(rendered);

    const uint8_t next_text[] = {
        0xe4, 0xbd, 0xa0, 0xe5, 0xa5, 0xbd,
    };
    status = synui_text_element_set_text(
        text, (synui_text_view_t){next_text, sizeof(next_text)}, &error);
    CHECK(status == SYNUI_OK);
    check_record(error, status);
    rendered = (synui_buffer_t){(uint32_t)sizeof(rendered), NULL, 0, 0};
    status = synui_text_element_get_rendered_text(text, &rendered, &error);
    CHECK(status == SYNUI_OK);
    CHECK(rendered.len == sizeof(initial_text));
    CHECK(memcmp(rendered.ptr, initial_text, rendered.len) == 0);
    check_record(error, status);
    synui_buffer_free(rendered);

    status = synui_app_render(app, elements, 1, &error);
    CHECK(status == SYNUI_OK);
    check_record(error, status);
    rendered = (synui_buffer_t){(uint32_t)sizeof(rendered), NULL, 0, 0};
    status = synui_text_element_get_rendered_text(text, &rendered, &error);
    CHECK(status == SYNUI_OK);
    CHECK(rendered.len == sizeof(next_text));
    CHECK(memcmp(rendered.ptr, next_text, rendered.len) == 0);
    check_record(error, status);
    synui_buffer_free(rendered);

    const uint8_t invalid_text[] = {0xff};
    status = synui_text_element_set_text(
        text, (synui_text_view_t){invalid_text, sizeof(invalid_text)}, &error);
    CHECK(status == SYNUI_INVALID_ARGUMENT);
    check_record(error, status);
    status = synui_element_release(text, &error);
    CHECK(status == SYNUI_OK);
    check_record(error, status);
    status = synui_app_release(app, &error);
    CHECK(status == SYNUI_OK);
    check_record(error, status);

    status = synui_app_release((synui_app_t *)runtime, &error);
    CHECK(status == SYNUI_INVALID_ARGUMENT);
    check_record(error, status);

    status = synui_runtime_release(runtime, &error);
    CHECK(status == SYNUI_OK);
    check_record(error, status);
    status = synui_runtime_release(runtime, &error);
    CHECK(status == SYNUI_STALE_HANDLE);
    check_record(error, status);
    status = synui_runtime_release(NULL, &error);
    CHECK(status == SYNUI_OK);
    check_record(error, status);
    status = synui_runtime_release(retained, &error);
    CHECK(status == SYNUI_OK);
    check_record(error, status);
    synui_error_record_free(NULL);

    printf("LAYOUT %zu %zu %zu %zu %zu %zu %zu %zu %zu %zu "
           "%zu %zu %zu %zu %zu %zu %zu %zu\n",
           sizeof(synui_status_t),
           sizeof(synui_abi_version_t),
           sizeof(synui_abi_info_t),
           sizeof(synui_runtime_options_t),
           sizeof(synui_runtime_info_t),
           sizeof(synui_error_record_t),
           sizeof(synui_buffer_t),
           sizeof(synui_callback_payload_t),
           sizeof(synui_post_t),
           sizeof(synui_post_token_info_t),
           sizeof(synui_table_dimensions_t),
           sizeof(synui_cell_range_t),
           sizeof(synui_cell_request_t),
           sizeof(synui_cell_coordinate_t),
           sizeof(synui_cell_value_t),
           sizeof(synui_sort_descriptor_t),
           sizeof(synui_prefetch_band_t),
           sizeof(synui_virtual_data_table_model_vtable_t));
    return failures == 0 ? 0 : 1;
}
