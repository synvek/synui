"""Property coverage for exactly-once Python opaque-handle release."""

import random
import unittest

from synui._errors import NoError
from synui._handles import Runtime, _finalize_handle


class FakeFFI:
    NULL = None


class FakeLib:
    def __init__(self):
        self.release_calls = []
        self.query_calls = []

    def synui_runtime_release(self, handle):
        self.release_calls.append(handle)
        return 0

    def synui_runtime_query(self, handle):
        self.query_calls.append(handle)
        return 0


class FakeAbi:
    def __init__(self):
        self.ffi = FakeFFI()
        self.lib = FakeLib()

    def invoke(self, function, handle, *arguments):
        del arguments
        return NoError() if function(handle) == 0 else NoError()


class HandleOwnershipPropertyTest(unittest.TestCase):
    # Feature: synui-api-abi-reset, Property 12: Python handle 释放恰好一次
    def test_close_context_exit_and_finalizer_interleave_release_once(self):
        randomizer = random.Random(0x5A17)
        for _ in range(100):
            abi = FakeAbi()
            handle = Runtime._from_owned(abi, object())
            operations = [randomizer.randrange(3) for _ in range(randomizer.randrange(1, 17))]

            for operation in operations:
                if operation == 0:
                    handle.close()
                elif operation == 1:
                    if handle.closed:
                        handle.__exit__(None, None, None)
                    else:
                        with handle:
                            pass
                else:
                    _finalize_handle(
                        handle._gate, abi, "synui_runtime_release"
                    )
                self.assertLessEqual(len(abi.lib.release_calls), 1)

            # The object remains owned until at least one generated operation closes it.
            handle.close()
            self.assertTrue(handle.closed)
            self.assertEqual(len(abi.lib.release_calls), 1)
            _finalize_handle(handle._gate, abi, "synui_runtime_release")
            self.assertEqual(len(abi.lib.release_calls), 1)
            with self.assertRaises(RuntimeError):
                handle._require_open()
            with self.assertRaises(RuntimeError):
                handle._invoke(abi.lib.synui_runtime_query)
            self.assertEqual(len(abi.lib.release_calls), 1)
