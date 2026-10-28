#include "synui_v1.h"

#include <algorithm>
#include <cstdint>
#include <cstdio>
#include <limits>
#include <thread>
#include <vector>

static int failures = 0;

#define CHECK(condition)                                                        \
    do {                                                                        \
        if (!(condition)) {                                                     \
            std::fprintf(stderr, "C++ smoke check failed at line %d: %s\n",  \
                         __LINE__, #condition);                                 \
            failures += 1;                                                      \
        }                                                                       \
    } while (0)

static void check_record(synui_error_record_t *record,
                         synui_status_t expected) {
    CHECK(record != nullptr);
    if (record == nullptr) {
        return;
    }
    CHECK(record->struct_size == sizeof(*record));
    CHECK(record->status == expected);
    CHECK(record->code == expected);
    if (expected == SYNUI_OK) {
        CHECK(record->message.ptr == nullptr);
        CHECK(record->message.len == 0);
        CHECK(record->detail.ptr == nullptr);
        CHECK(record->detail.len == 0);
    } else {
        CHECK(record->message.ptr != nullptr);
        CHECK(record->message.len != 0);
        CHECK(record->detail.ptr != nullptr);
        CHECK(record->detail.len != 0);
    }
    synui_error_record_free(record);
}

struct Observation {
    std::thread::id callback_thread;
    std::vector<std::uint8_t> bytes;
    std::size_t calls = 0;
};
static synui_status_t record_callback(
    void *user_data, const synui_callback_payload_t *payload) {
    if (user_data == nullptr || payload == nullptr ||
        payload->struct_size < sizeof(*payload) ||
        (payload->bytes.ptr == nullptr && payload->bytes.len != 0)) {
        return SYNUI_INVALID_ARGUMENT;
    }
    auto *observation = static_cast<Observation *>(user_data);
    observation->callback_thread = std::this_thread::get_id();
    observation->calls += 1;
    if (payload->bytes.len == 0) {
        observation->bytes.clear();
    } else {
        observation->bytes.assign(payload->bytes.ptr,
                                  payload->bytes.ptr + payload->bytes.len);
    }
    return SYNUI_OK;
}

static synui_status_t no_op_callback(
    void *, const synui_callback_payload_t *) {
    return SYNUI_OK;
}

static synui_status_t failing_callback(
    void *, const synui_callback_payload_t *) {
    return SYNUI_INVALID_ARGUMENT;
}

static synui_post_t post(synui_post_callback_t callback, void *user_data,
                         const std::uint8_t *bytes, std::size_t len) {
    return synui_post_t{
        static_cast<std::uint32_t>(sizeof(synui_post_t)),
        callback,
        user_data,
        {bytes, len},
    };
}

static synui_post_token_info_t query(synui_post_token_t *token) {
    synui_post_token_info_t info{
        static_cast<std::uint32_t>(sizeof(info)),
        std::numeric_limits<std::uint32_t>::max(),
        std::numeric_limits<std::uint32_t>::max(),
    };
    synui_error_record_t *error = nullptr;
    const auto status = synui_post_token_query(token, &info, &error);
    CHECK(status == SYNUI_OK);
    check_record(error, status);
    return info;
}
int main() {
    synui_runtime_options_t options{
        static_cast<std::uint32_t>(sizeof(options)),
        {SYNUI_ABI_VERSION_MAJOR, SYNUI_ABI_VERSION_MINOR},
        SYNUI_CAPABILITY_BASE,
    };
    synui_error_record_t *error = nullptr;
    synui_runtime_t *runtime = nullptr;
    auto status = synui_runtime_create(&options, &runtime, &error);
    CHECK(status == SYNUI_OK);
    check_record(error, status);

    synui_app_t *app = nullptr;
    status = synui_app_create(runtime, &app, &error);
    CHECK(status == SYNUI_OK);
    check_record(error, status);
    const auto ui_thread = std::this_thread::get_id();

    Observation observation;
    synui_post_token_t *executed = nullptr;
    synui_status_t worker_post_status = SYNUI_INTERNAL_ERROR;
    synui_status_t worker_drain_status = SYNUI_INTERNAL_ERROR;
    std::size_t worker_drained = std::numeric_limits<std::size_t>::max();
    std::thread worker([&] {
        std::vector<std::uint8_t> source{1, 2, 3, 4};
        auto work = post(record_callback, &observation,
                         source.data(), source.size());
        synui_error_record_t *worker_error = nullptr;
        worker_post_status = synui_app_post(
            app, &work, &executed, &worker_error);
        check_record(worker_error, worker_post_status);
        std::fill(source.begin(), source.end(), 9);
        worker_drain_status = synui_app_drain_posts(
            app, &worker_drained, &worker_error);
        check_record(worker_error, worker_drain_status);
    });
    worker.join();

    CHECK(worker_post_status == SYNUI_OK);
    CHECK(worker_drain_status == SYNUI_THREAD_AFFINITY);
    CHECK(worker_drained == std::numeric_limits<std::size_t>::max());
    CHECK(query(executed).state == SYNUI_POST_ACCEPTED);

    std::size_t drained = 0;
    status = synui_app_drain_posts(app, &drained, &error);
    CHECK(status == SYNUI_OK);
    CHECK(drained == 1);
    check_record(error, status);
    const auto executed_info = query(executed);
    CHECK(executed_info.state == SYNUI_POST_EXECUTED);
    CHECK(executed_info.callback_status == SYNUI_OK);
    CHECK(observation.callback_thread == ui_thread);
    CHECK(observation.calls == 1);
    CHECK(observation.bytes == std::vector<std::uint8_t>({1, 2, 3, 4}));
    auto no_op = post(no_op_callback, nullptr, nullptr, 0);
    synui_post_token_t *cancelled = nullptr;
    status = synui_app_post(app, &no_op, &cancelled, &error);
    CHECK(status == SYNUI_OK);
    check_record(error, status);
    status = synui_post_token_cancel(cancelled, &error);
    CHECK(status == SYNUI_OK);
    check_record(error, status);
    const auto cancelled_info = query(cancelled);
    CHECK(cancelled_info.state == SYNUI_POST_CANCELLED_STATE);
    CHECK(cancelled_info.callback_status == SYNUI_POST_CANCELLED);
    status = synui_post_token_cancel(cancelled, &error);
    CHECK(status == SYNUI_POST_CANCELLED);
    check_record(error, status);

    auto failing = post(failing_callback, nullptr, nullptr, 0);
    synui_post_token_t *failed = nullptr;
    status = synui_app_post(app, &failing, &failed, &error);
    CHECK(status == SYNUI_OK);
    check_record(error, status);
    drained = 0;
    status = synui_app_drain_posts(app, &drained, &error);
    CHECK(status == SYNUI_CALLBACK_ERROR);
    CHECK(drained == 1);
    check_record(error, status);
    const auto failed_info = query(failed);
    CHECK(failed_info.state == SYNUI_POST_FAILED);
    CHECK(failed_info.callback_status == SYNUI_INVALID_ARGUMENT);

    synui_post_token_t *closed = nullptr;
    status = synui_app_post(app, &no_op, &closed, &error);
    CHECK(status == SYNUI_OK);
    check_record(error, status);
    status = synui_app_release(app, &error);
    CHECK(status == SYNUI_OK);
    check_record(error, status);
    const auto closed_info = query(closed);
    CHECK(closed_info.state == SYNUI_POST_CANCELLED_STATE);
    CHECK(closed_info.callback_status == SYNUI_POST_CANCELLED);
    for (auto *token : {executed, cancelled, failed, closed}) {
        status = synui_post_token_release(token, &error);
        CHECK(status == SYNUI_OK);
        check_record(error, status);
    }
    status = synui_runtime_release(runtime, &error);
    CHECK(status == SYNUI_OK);
    check_record(error, status);
    return failures == 0 ? 0 : 1;
}
