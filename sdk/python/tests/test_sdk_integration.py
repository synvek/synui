"""ABI-only Python SDK integration coverage."""

import os
import threading
import unittest
from pathlib import Path

from synui._abi import Abi
from synui._counter import ActionCallback, ButtonElement, IntState, TextElement, render
from synui._errors import Status, SynuiError
from synui._handles import PostState, Runtime
from synui._model import (
    CellCoordinate,
    CellRange,
    PrefetchBand,
    TableDimensions,
    VirtualDataTableModel,
)


class Provider:
    def __init__(self):
        self.requests = []
        self.updates = []

    def dimensions(self):
        return TableDimensions(100_000, 8)

    def request_cells(self, request):
        self.requests.append(request)

    def apply_cell_update(self, coordinate, value):
        self.updates.append((coordinate, value))

    def sort_changed(self, sort):
        pass

    def selection_changed(self, ranges):
        pass


class SdkIntegrationTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        configured = os.environ.get("SYNUI_ABI_LIBRARY")
        library = configured or str(Path.cwd() / "target/debug/libsynui_abi.dylib")
        if not Path(library).exists():
            raise unittest.SkipTest("Production ABI library is not built")
        cls.abi = Abi.load(library)

    def test_counter_state_action_text_render_and_thread_gate(self):
        runtime = Runtime.create(self.abi).unwrap()
        app = runtime.create_app().unwrap()
        state = IntState.create(app, 0).unwrap()

        def increment():
            state.set(state.get().unwrap() + 1)

        callback = ActionCallback.create(app, increment).unwrap()
        button = ButtonElement.create(app, callback).unwrap()
        text = TextElement.create(app, "0").unwrap()
        render(app, [text, button]).unwrap()
        self.assertEqual(text.rendered_text().unwrap(), "0")
        button.press().unwrap()
        self.assertEqual(state.get().unwrap(), 1)
        text.set_text("你好").unwrap()
        self.assertEqual(text.rendered_text().unwrap(), "0")
        render(app, [text, button]).unwrap()
        self.assertEqual(text.rendered_text().unwrap(), "你好")

        result = []
        worker = threading.Thread(
            target=lambda: self._capture(result, app.drain_posts)
        )
        worker.start()
        worker.join()
        self.assertIsInstance(result[0], SynuiError)
        self.assertEqual(result[0].status, int(Status.THREAD_AFFINITY))

        button.close()
        callback.close()
        text.close()
        state.close()
        app.close()
        runtime.close()

    @staticmethod
    def _capture(output, operation):
        try:
            operation()
        except BaseException as error:
            output.append(error)

    def test_worker_post_and_virtual_table_callbacks(self):
        runtime = Runtime.create(self.abi).unwrap()
        app = runtime.create_app().unwrap()
        payloads = []
        token = app.post(lambda payload: payloads.append(payload), b"payload").unwrap()
        self.assertEqual(app.drain_posts().unwrap(), 1)
        self.assertEqual(payloads, [b"payload"])
        self.assertEqual(token.query().unwrap().state, PostState.EXECUTED)

        provider = Provider()
        model = VirtualDataTableModel.create(app, provider).unwrap()
        request = model.request_visible(
            CellRange(99_990, 100_010, 0, 2),
            PrefetchBand(rows_before=8, rows_after=8, columns_before=1, columns_after=1),
        ).unwrap()
        self.assertIsNotNone(request)
        self.assertLessEqual(request.range.row_end, 100_000)
        self.assertLessEqual(request.range.column_end, 8)
        self.assertEqual(provider.requests, [request])
        model.apply_cell_update(CellCoordinate(99_999, 1), "ok").unwrap()
        self.assertEqual(len(provider.updates), 1)

        model.close()
        token.close()
        app.close()
        runtime.close()
