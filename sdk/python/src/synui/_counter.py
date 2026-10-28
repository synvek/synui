"""Typed State and retained facade Elements for the ABI Counter path."""

from typing import Any, Callable, Optional, Sequence

from ._errors import Status, SynuiResult
from ._handles import App, Element, _OwnedHandle


def _utf8_view(ffi: Any, text: str) -> tuple[Any, Any]:
    if not isinstance(text, str):
        raise TypeError("text must be str")
    encoded = text.encode("utf-8")
    storage = ffi.new("unsigned char[]", encoded) if encoded else ffi.NULL
    view = ffi.new("synui_text_view_t *")
    view.ptr = storage
    view.len = len(encoded)
    return storage, view[0]


class IntState(_OwnedHandle):
    _c_type = "synui_state_t"
    _retain_name = "synui_state_retain"
    _release_name = "synui_state_release"

    @classmethod
    def create(cls, app: App, initial: int = 0) -> SynuiResult["IntState"]:
        output = app._abi.ffi.new("synui_state_t **")
        diagnostic = app._invoke(
            app._abi.lib.synui_state_i64_create,
            initial,
            output,
        )
        return SynuiResult(cls._from_owned(app._abi, output[0]), diagnostic)

    def get(self) -> SynuiResult[int]:
        output = self._abi.ffi.new("int64_t *")
        diagnostic = self._invoke(self._abi.lib.synui_state_i64_get, output)
        return SynuiResult(int(output[0]), diagnostic)

    def set(self, value: int) -> SynuiResult[None]:
        diagnostic = self._invoke(self._abi.lib.synui_state_i64_set, value)
        return SynuiResult(None, diagnostic)


class _CallbackContext:
    def __init__(self, callback: Callable[[], Optional[int]]):
        self.callback = callback
        self.trampoline: Any = None
        self.user_data: Any = None
        self.exception: Optional[BaseException] = None


class ActionCallback(_OwnedHandle):
    _c_type = "synui_callback_t"
    _retain_name = "synui_callback_retain"
    _release_name = "synui_callback_release"

    def __init__(
        self, abi: Any, handle: Any, owned: bool = False, context: Any = None
    ) -> None:
        super().__init__(abi, handle, owned)
        self._context = context

    @classmethod
    def _from_owned_with_context(
        cls, abi: Any, handle: Any, context: _CallbackContext
    ) -> "ActionCallback":
        return cls(abi, handle, owned=True, context=context)

    @classmethod
    def create(
        cls, app: App, callback: Callable[[], Optional[int]]
    ) -> SynuiResult["ActionCallback"]:
        ffi = app._abi.ffi
        context = _CallbackContext(callback)

        def invoke(user_data: Any) -> int:
            current = ffi.from_handle(user_data)
            try:
                result = current.callback()
                status = int(Status.OK) if result is None else int(result)
                if status < 0 or status > 0xFFFF_FFFF:
                    raise ValueError("callback status is outside uint32_t")
                return status
            except BaseException as error:
                current.exception = error
                return int(Status.CALLBACK_ERROR)

        context.trampoline = ffi.callback("synui_action_callback_t", invoke)
        context.user_data = ffi.new_handle(context)
        output = ffi.new("synui_callback_t **")
        diagnostic = app._invoke(
            app._abi.lib.synui_callback_create,
            context.trampoline,
            context.user_data,
            output,
        )
        return SynuiResult(
            cls._from_owned_with_context(app._abi, output[0], context),
            diagnostic,
        )

    @property
    def callback_exception(self) -> Optional[BaseException]:
        return self._context.exception

    def retain(self) -> SynuiResult["ActionCallback"]:
        output = self._abi.ffi.new("synui_callback_t **")
        diagnostic = self._invoke(self._abi.lib.synui_callback_retain, output)
        return SynuiResult(
            ActionCallback._from_owned_with_context(
                self._abi, output[0], self._context
            ),
            diagnostic,
        )


class TextElement(Element):
    @classmethod
    def create(cls, app: App, text: str) -> SynuiResult["TextElement"]:
        ffi = app._abi.ffi
        storage, view = _utf8_view(ffi, text)
        output = ffi.new("synui_element_t **")
        diagnostic = app._invoke(
            app._abi.lib.synui_text_element_create,
            view,
            output,
        )
        del storage
        return SynuiResult(cls._from_owned(app._abi, output[0]), diagnostic)

    def set_text(self, text: str) -> SynuiResult[None]:
        ffi = self._abi.ffi
        storage, view = _utf8_view(ffi, text)
        diagnostic = self._invoke(
            self._abi.lib.synui_text_element_set_text,
            view,
        )
        del storage
        return SynuiResult(None, diagnostic)

    def rendered_text(self) -> SynuiResult[str]:
        ffi = self._abi.ffi
        output = ffi.new("synui_buffer_t *")
        output.struct_size = ffi.sizeof("synui_buffer_t")
        try:
            diagnostic = self._invoke(
                self._abi.lib.synui_text_element_get_rendered_text,
                output,
            )
            try:
                text = self._abi._text(output[0])
            except (UnicodeDecodeError, ValueError) as error:
                raise self._abi._contract_failure(str(error))
            return SynuiResult(text, diagnostic)
        finally:
            self._abi.lib.synui_buffer_free(output[0])


class ButtonElement(Element):
    def __init__(
        self, abi: Any, handle: Any, owned: bool = False, context: Any = None
    ) -> None:
        super().__init__(abi, handle, owned)
        self._context = context

    @classmethod
    def _from_owned_with_context(
        cls, abi: Any, handle: Any, context: _CallbackContext
    ) -> "ButtonElement":
        return cls(abi, handle, owned=True, context=context)

    @classmethod
    def create(
        cls, app: App, callback: ActionCallback
    ) -> SynuiResult["ButtonElement"]:
        output = app._abi.ffi.new("synui_element_t **")
        diagnostic = app._invoke(
            app._abi.lib.synui_button_element_create,
            callback._require_open(),
            output,
        )
        return SynuiResult(
            cls._from_owned_with_context(app._abi, output[0], callback._context),
            diagnostic,
        )

    def press(self) -> SynuiResult[None]:
        diagnostic = self._invoke(self._abi.lib.synui_element_press)
        return SynuiResult(None, diagnostic)

    def retain(self) -> SynuiResult["ButtonElement"]:
        output = self._abi.ffi.new("synui_element_t **")
        diagnostic = self._invoke(self._abi.lib.synui_element_retain, output)
        return SynuiResult(
            ButtonElement._from_owned_with_context(
                self._abi, output[0], self._context
            ),
            diagnostic,
        )


def render(app: App, elements: Sequence[Element]) -> SynuiResult[None]:
    ffi = app._abi.ffi
    handles = ffi.new("synui_element_t *[]", len(elements)) if elements else ffi.NULL
    for index, element in enumerate(elements):
        handles[index] = element._require_open()
    diagnostic = app._invoke(
        app._abi.lib.synui_app_render,
        handles,
        len(elements),
    )
    return SynuiResult(None, diagnostic)
