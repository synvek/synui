"""Opaque Production ABI handle wrappers available in ABI v1."""

import threading
import weakref
from dataclasses import dataclass
from enum import IntEnum
from functools import partial
from typing import Any, Callable, ClassVar, Dict, Optional, Tuple, Type, TypeVar

from ._abi import ABI_MAJOR, ABI_MINOR, Abi
from ._errors import NoError, Status, SynuiResult


@dataclass(frozen=True)
class RuntimeInfo:
    major: int
    minor: int
    capabilities: int
    resource_slot: int
    generation: int
    reference_count: int


class PostState(IntEnum):
    ACCEPTED = 0
    RUNNING = 1
    EXECUTED = 2
    CANCELLED = 3
    FAILED = 4


@dataclass(frozen=True)
class PostInfo:
    state: PostState
    callback_status: int


class _PostContext:
    def __init__(self, callback: Callable[[bytes], Optional[int]]):
        self.callback = callback
        self.invoked = False
        self.exception: Optional[BaseException] = None
        self.trampoline: Any = None
        self.user_data: Any = None


class _AppState:
    def __init__(self) -> None:
        self.lock = threading.RLock()
        self.callbacks: Dict[int, _PostContext] = {}
        self.leases = 0


def _release_app_state_lease(state: _AppState) -> None:
    with state.lock:
        state.leases -= 1
        if state.leases == 0:
            state.callbacks.clear()


class _ClosedHandleError(RuntimeError):
    pass


class _CloseGate:
    _OPEN = 0
    _CLOSING = 1
    _CLOSED = 2

    def __init__(
        self, handle: Any, on_closed: Optional[Callable[[], None]] = None
    ) -> None:
        self._condition = threading.Condition(threading.RLock())
        self._handle = handle
        self._state = self._OPEN
        self._active = 0
        self._active_by_thread: Dict[int, int] = {}
        self._on_closed = on_closed

    @property
    def closed(self) -> bool:
        with self._condition:
            return self._state != self._OPEN

    def require_open(self) -> Any:
        with self._condition:
            if self._state != self._OPEN:
                raise _ClosedHandleError("opaque handle is closed")
            return self._handle

    def invoke(self, abi: Abi, function: Callable[..., int], *arguments: Any) -> NoError:
        thread_id = threading.get_ident()
        with self._condition:
            if self._state != self._OPEN:
                raise _ClosedHandleError("opaque handle is closed")
            handle = self._handle
            self._active += 1
            self._active_by_thread[thread_id] = (
                self._active_by_thread.get(thread_id, 0) + 1
            )
        try:
            return abi.invoke(function, handle, *arguments)
        finally:
            with self._condition:
                self._active -= 1
                remaining = self._active_by_thread[thread_id] - 1
                if remaining == 0:
                    del self._active_by_thread[thread_id]
                else:
                    self._active_by_thread[thread_id] = remaining
                self._condition.notify_all()

    def release(self, abi: Abi, function: Callable[..., int]) -> Tuple[NoError, bool]:
        thread_id = threading.get_ident()
        with self._condition:
            while self._state == self._CLOSING:
                self._condition.wait()
            if self._state == self._CLOSED:
                return NoError(), False
            self._state = self._CLOSING
            own_active = self._active_by_thread.get(thread_id, 0)
            try:
                while self._active > own_active:
                    self._condition.wait()
            except BaseException:
                self._state = self._OPEN
                self._condition.notify_all()
                raise
            handle = self._handle

        try:
            diagnostic = abi.invoke(function, handle)
        finally:
            try:
                if self._on_closed is not None:
                    self._on_closed()
            finally:
                with self._condition:
                    self._handle = abi.ffi.NULL
                    self._state = self._CLOSED
                    self._condition.notify_all()
        return diagnostic, True


def _finalize_handle(gate: _CloseGate, abi: Abi, release_name: str) -> None:
    try:
        gate.release(abi, getattr(abi.lib, release_name))
    except BaseException:
        # Finalization is a best-effort fallback; explicit close surfaces errors.
        pass


HandleT = TypeVar("HandleT", bound="_OwnedHandle")


class _OwnedHandle:
    _c_type: ClassVar[str]
    _retain_name: ClassVar[str]
    _release_name: ClassVar[str]

    def __init__(
        self,
        abi: Abi,
        handle: Any,
        owned: bool = False,
        on_closed: Optional[Callable[[], None]] = None,
    ):
        if not owned:
            raise TypeError("opaque handles can only be created by Production ABI commands")
        if handle == abi.ffi.NULL:
            raise ValueError("Production ABI returned a NULL owned handle")
        self._abi = abi
        self._gate = _CloseGate(handle, on_closed)
        self._finalizer = weakref.finalize(
            self, _finalize_handle, self._gate, abi, self._release_name
        )

    @classmethod
    def _from_owned(cls: Type[HandleT], abi: Abi, handle: Any) -> HandleT:
        return cls(abi, handle, owned=True)

    @property
    def closed(self) -> bool:
        return self._gate.closed

    def _require_open(self) -> Any:
        try:
            return self._gate.require_open()
        except _ClosedHandleError:
            raise RuntimeError("{} handle is closed".format(type(self).__name__))

    def _invoke(self, function: Callable[..., int], *arguments: Any) -> NoError:
        try:
            return self._gate.invoke(self._abi, function, *arguments)
        except _ClosedHandleError:
            raise RuntimeError("{} handle is closed".format(type(self).__name__))

    def retain(self: HandleT) -> SynuiResult[HandleT]:
        output = self._abi.ffi.new("{} **".format(self._c_type))
        diagnostic = self._invoke(getattr(self._abi.lib, self._retain_name), output)
        return SynuiResult(type(self)._from_owned(self._abi, output[0]), diagnostic)

    def close(self) -> SynuiResult[None]:
        try:
            diagnostic, _ = self._gate.release(
                self._abi, getattr(self._abi.lib, self._release_name)
            )
            return SynuiResult(None, diagnostic)
        finally:
            if self._gate.closed:
                self._finalizer.detach()

    def __enter__(self: HandleT) -> HandleT:
        self._require_open()
        return self

    def __exit__(self, exc_type: Any, exc_value: Any, traceback: Any) -> bool:
        self.close()
        return False


class Runtime(_OwnedHandle):
    _c_type = "synui_runtime_t"
    _retain_name = "synui_runtime_retain"
    _release_name = "synui_runtime_release"

    @classmethod
    def create(cls, abi: Abi) -> SynuiResult["Runtime"]:
        options = abi.ffi.new("synui_runtime_options_t *")
        options.struct_size = abi.ffi.sizeof("synui_runtime_options_t")
        options.requested_version.major = ABI_MAJOR
        options.requested_version.minor = ABI_MINOR
        options.requested_capabilities = abi.info.capabilities
        output = abi.ffi.new("synui_runtime_t **")
        diagnostic = abi.invoke(abi.lib.synui_runtime_create, options, output)
        return SynuiResult(cls._from_owned(abi, output[0]), diagnostic)

    def query(self) -> SynuiResult[RuntimeInfo]:
        output = self._abi.ffi.new("synui_runtime_info_t *")
        output.struct_size = self._abi.ffi.sizeof("synui_runtime_info_t")
        diagnostic = self._invoke(self._abi.lib.synui_runtime_query, output)
        value = RuntimeInfo(
            int(output.version.major),
            int(output.version.minor),
            int(output.capabilities),
            int(output.resource_slot),
            int(output.generation),
            int(output.reference_count),
        )
        return SynuiResult(value, diagnostic)

    def create_app(self) -> SynuiResult["App"]:
        output = self._abi.ffi.new("synui_app_t **")
        diagnostic = self._invoke(self._abi.lib.synui_app_create, output)
        return SynuiResult(App._from_owned(self._abi, output[0]), diagnostic)


class App(_OwnedHandle):
    _c_type = "synui_app_t"
    _retain_name = "synui_app_retain"
    _release_name = "synui_app_release"

    def __init__(
        self, abi: Abi, handle: Any, owned: bool = False, state: Optional[_AppState] = None
    ):
        self._state = state if state is not None else _AppState()
        with self._state.lock:
            self._state.leases += 1
        try:
            super().__init__(
                abi,
                handle,
                owned,
                on_closed=partial(_release_app_state_lease, self._state),
            )
        except BaseException:
            _release_app_state_lease(self._state)
            raise

    @classmethod
    def _from_owned(
        cls, abi: Abi, handle: Any, state: Optional[_AppState] = None
    ) -> "App":
        return cls(abi, handle, owned=True, state=state)

    def _token_key(self, handle: Any) -> int:
        return int(self._abi.ffi.cast("size_t", handle))

    def retain(self) -> SynuiResult["App"]:
        output = self._abi.ffi.new("synui_app_t **")
        with self._state.lock:
            diagnostic = self._invoke(self._abi.lib.synui_app_retain, output)
            retained = App._from_owned(self._abi, output[0], self._state)
        return SynuiResult(retained, diagnostic)

    def post(
        self, callback: Callable[[bytes], Optional[int]], payload: bytes = b""
    ) -> SynuiResult["PostToken"]:
        copied_payload = bytes(payload)
        context = _PostContext(callback)
        ffi = self._abi.ffi

        def invoke_callback(user_data: Any, abi_payload: Any) -> int:
            current = ffi.from_handle(user_data)
            current.invoked = True
            try:
                if abi_payload == ffi.NULL:
                    raise ValueError("Production ABI passed a NULL callback payload")
                view = abi_payload.bytes
                if int(view.len) == 0:
                    callback_payload = b""
                elif view.ptr == ffi.NULL:
                    raise ValueError("callback bytes have NULL pointer with non-zero length")
                else:
                    callback_payload = bytes(ffi.buffer(view.ptr, int(view.len)))
                result = current.callback(callback_payload)
                status = int(Status.OK) if result is None else int(result)
                if status < 0 or status > 0xFFFF_FFFF:
                    raise ValueError("callback status is outside uint32_t")
                return status
            except BaseException as error:
                current.exception = error
                return int(Status.CALLBACK_ERROR)

        context.trampoline = ffi.callback("synui_post_callback_t", invoke_callback)
        context.user_data = ffi.new_handle(context)
        work = ffi.new("synui_post_t *")
        work.struct_size = ffi.sizeof("synui_post_t")
        work.callback = context.trampoline
        work.user_data = context.user_data
        if copied_payload:
            buffer = ffi.from_buffer("const unsigned char[]", copied_payload)
            work.payload.ptr = buffer
            work.payload.len = len(copied_payload)
        else:
            work.payload.ptr = ffi.NULL
            work.payload.len = 0
        output = ffi.new("synui_post_token_t **")
        with self._state.lock:
            diagnostic = self._invoke(self._abi.lib.synui_app_post, work, output)
            key = self._token_key(output[0])
            if not context.invoked:
                self._state.callbacks[key] = context
            token = PostToken._from_owned(
                self._abi, output[0], self._state, key, context
            )
        return SynuiResult(token, diagnostic)

    def drain_posts(self) -> SynuiResult[int]:
        drained = self._abi.ffi.new("size_t *")
        try:
            diagnostic = self._invoke(self._abi.lib.synui_app_drain_posts, drained)
        finally:
            with self._state.lock:
                invoked = [
                    key
                    for key, context in self._state.callbacks.items()
                    if context.invoked
                ]
                for key in invoked:
                    self._state.callbacks.pop(key, None)
        return SynuiResult(int(drained[0]), diagnostic)


class PostToken(_OwnedHandle):
    _c_type = "synui_post_token_t"
    _retain_name = "synui_post_token_retain"
    _release_name = "synui_post_token_release"

    def __init__(
        self,
        abi: Abi,
        handle: Any,
        owned: bool,
        state: _AppState,
        key: int,
        context: _PostContext,
    ):
        super().__init__(abi, handle, owned)
        self._state = state
        self._key = key
        self._context = context

    @classmethod
    def _from_owned(
        cls,
        abi: Abi,
        handle: Any,
        state: _AppState,
        key: int,
        context: _PostContext,
    ) -> "PostToken":
        return cls(abi, handle, True, state, key, context)

    @property
    def callback_exception(self) -> Optional[BaseException]:
        return self._context.exception

    def retain(self) -> SynuiResult["PostToken"]:
        output = self._abi.ffi.new("synui_post_token_t **")
        diagnostic = self._invoke(self._abi.lib.synui_post_token_retain, output)
        return SynuiResult(
            PostToken._from_owned(
                self._abi, output[0], self._state, self._key, self._context
            ),
            diagnostic,
        )

    def query(self) -> SynuiResult[PostInfo]:
        output = self._abi.ffi.new("synui_post_token_info_t *")
        output.struct_size = self._abi.ffi.sizeof("synui_post_token_info_t")
        diagnostic = self._invoke(self._abi.lib.synui_post_token_query, output)
        return SynuiResult(
            PostInfo(PostState(int(output.state)), int(output.callback_status)),
            diagnostic,
        )

    def cancel(self) -> SynuiResult[None]:
        diagnostic = self._invoke(self._abi.lib.synui_post_token_cancel)
        with self._state.lock:
            self._state.callbacks.pop(self._key, None)
        return SynuiResult(None, diagnostic)


class Window(_OwnedHandle):
    """Opaque Window wrapper; ABI v1 base does not yet expose a creator."""

    _c_type = "synui_window_t"
    _retain_name = "synui_window_retain"
    _release_name = "synui_window_release"


class Element(_OwnedHandle):
    """Opaque Element wrapper; ABI v1 base does not yet expose a creator."""

    _c_type = "synui_element_t"
    _retain_name = "synui_element_retain"
    _release_name = "synui_element_release"


class Model(_OwnedHandle):
    """Opaque Model wrapper; ABI v1 base does not yet expose a creator."""

    _c_type = "synui_model_t"
    _retain_name = "synui_model_retain"
    _release_name = "synui_model_release"


class Callback(_OwnedHandle):
    """Opaque Callback wrapper; ABI v1 base does not yet expose a creator."""

    _c_type = "synui_callback_t"
    _retain_name = "synui_callback_retain"
    _release_name = "synui_callback_release"
