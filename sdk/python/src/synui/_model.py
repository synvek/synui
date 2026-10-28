"""Virtual DataTable model wrappers backed only by the Production ABI."""

from dataclasses import dataclass
from enum import IntEnum
from typing import Any, List, Optional, Sequence, Tuple, Union

from ._errors import Status, SynuiResult
from ._handles import App, Model


@dataclass(frozen=True)
class TableDimensions:
    rows: int
    columns: int


@dataclass(frozen=True)
class CellRange:
    row_start: int
    row_end: int
    column_start: int
    column_end: int


@dataclass(frozen=True)
class CellRequest:
    request_id: int
    range: CellRange


@dataclass(frozen=True)
class CellCoordinate:
    row: int
    column: int


@dataclass(frozen=True)
class PrefetchBand:
    rows_before: int = 0
    rows_after: int = 0
    columns_before: int = 0
    columns_after: int = 0


class SortDirection(IntEnum):
    ASCENDING = 0
    DESCENDING = 1


@dataclass(frozen=True)
class SortDescriptor:
    column: int
    direction: SortDirection


CellValue = Union[None, str, bool, float]


class _ModelContext:
    def __init__(self, provider: Any):
        self.provider = provider
        self.callbacks: Tuple[Any, ...] = ()
        self.user_data: Any = None
        self.exception: Optional[BaseException] = None

    def failed(self, error: BaseException) -> int:
        self.exception = error
        return int(Status.CALLBACK_ERROR)


class VirtualDataTableModel(Model):
    """Context-owned Python model called through the versioned ABI vtable."""

    def __init__(
        self, abi: Any, handle: Any, owned: bool = False, context: Any = None
    ) -> None:
        super().__init__(abi, handle, owned)
        self._context = context

    @classmethod
    def _from_owned(
        cls, abi: Any, handle: Any, context: _ModelContext
    ) -> "VirtualDataTableModel":
        return cls(abi, handle, owned=True, context=context)

    @classmethod
    def create(cls, app: App, provider: Any) -> SynuiResult["VirtualDataTableModel"]:
        abi = app._abi
        ffi = abi.ffi
        context = _ModelContext(provider)

        def current(user_data: Any) -> _ModelContext:
            return ffi.from_handle(user_data)

        def dimensions(user_data: Any, output: Any) -> int:
            model = current(user_data)
            try:
                value = model.provider.dimensions()
                if not isinstance(value, TableDimensions):
                    value = TableDimensions(*value)
                if value.rows < 0 or value.columns < 0:
                    raise ValueError("table dimensions must be non-negative")
                output.struct_size = ffi.sizeof("synui_table_dimensions_t")
                output.rows = value.rows
                output.columns = value.columns
                return int(Status.OK)
            except BaseException as error:
                return model.failed(error)

        def request_cells(user_data: Any, request: Any) -> int:
            model = current(user_data)
            try:
                model.provider.request_cells(
                    CellRequest(
                        int(request.request_id),
                        cls._read_range(request.range),
                    )
                )
                return int(Status.OK)
            except BaseException as error:
                return model.failed(error)

        def apply_cell_update(user_data: Any, coordinate: Any, value: Any) -> int:
            model = current(user_data)
            try:
                model.provider.apply_cell_update(
                    CellCoordinate(int(coordinate.row), int(coordinate.column)),
                    cls._read_value(ffi, value),
                )
                return int(Status.OK)
            except BaseException as error:
                return model.failed(error)

        def sort_changed(user_data: Any, sort: Any, length: int) -> int:
            model = current(user_data)
            try:
                values = [
                    SortDescriptor(int(sort[index].column), SortDirection(sort[index].direction))
                    for index in range(int(length))
                ]
                model.provider.sort_changed(values)
                return int(Status.OK)
            except BaseException as error:
                return model.failed(error)

        def selection_changed(user_data: Any, ranges: Any, length: int) -> int:
            model = current(user_data)
            try:
                values = [cls._read_range(ranges[index]) for index in range(int(length))]
                model.provider.selection_changed(values)
                return int(Status.OK)
            except BaseException as error:
                return model.failed(error)

        context.callbacks = (
            ffi.callback("synui_model_dimensions_callback_t", dimensions),
            ffi.callback("synui_model_request_cells_callback_t", request_cells),
            ffi.callback("synui_model_apply_cell_update_callback_t", apply_cell_update),
            ffi.callback("synui_model_sort_changed_callback_t", sort_changed),
            ffi.callback("synui_model_selection_changed_callback_t", selection_changed),
        )
        context.user_data = ffi.new_handle(context)
        vtable = ffi.new("synui_virtual_data_table_model_vtable_t *")
        vtable.struct_size = ffi.sizeof("synui_virtual_data_table_model_vtable_t")
        vtable.dimensions = context.callbacks[0]
        vtable.request_cells = context.callbacks[1]
        vtable.apply_cell_update = context.callbacks[2]
        vtable.sort_changed = context.callbacks[3]
        vtable.selection_changed = context.callbacks[4]
        output = ffi.new("synui_model_t **")
        diagnostic = app._invoke(
            abi.lib.synui_virtual_data_table_model_create,
            vtable,
            context.user_data,
            output,
        )
        return SynuiResult(cls._from_owned(abi, output[0], context), diagnostic)

    @staticmethod
    def _read_range(value: Any) -> CellRange:
        return CellRange(
            int(value.row_start),
            int(value.row_end),
            int(value.column_start),
            int(value.column_end),
        )

    @staticmethod
    def _read_value(ffi: Any, value: Any) -> CellValue:
        kind = int(value.kind)
        if kind == 0:
            return None
        if kind == 1:
            if int(value.text.len) == 0:
                return ""
            if value.text.ptr == ffi.NULL:
                raise ValueError("text cell has NULL pointer with non-zero length")
            return bytes(ffi.buffer(value.text.ptr, int(value.text.len))).decode("utf-8")
        if kind == 2:
            return bool(value.boolean)
        if kind == 3:
            return float(value.number)
        raise ValueError("unknown cell value kind {}".format(kind))

    @property
    def callback_exception(self) -> Optional[BaseException]:
        return self._context.exception

    def retain(self) -> SynuiResult["VirtualDataTableModel"]:
        output = self._abi.ffi.new("synui_model_t **")
        diagnostic = self._invoke(self._abi.lib.synui_model_retain, output)
        return SynuiResult(
            VirtualDataTableModel._from_owned(self._abi, output[0], self._context),
            diagnostic,
        )

    def request_visible(
        self, viewport: CellRange, prefetch: PrefetchBand = PrefetchBand()
    ) -> SynuiResult[Optional[CellRequest]]:
        ffi = self._abi.ffi
        requested = ffi.new("uint32_t *")
        output = ffi.new("synui_cell_request_t *")
        output.struct_size = ffi.sizeof("synui_cell_request_t")
        diagnostic = self._invoke(
            self._abi.lib.synui_virtual_data_table_model_request_visible,
            self._range(ffi, viewport)[0],
            self._prefetch(ffi, prefetch)[0],
            requested,
            output,
        )
        value = None
        if int(requested[0]) != 0:
            value = CellRequest(int(output.request_id), self._read_range(output.range))
        return SynuiResult(value, diagnostic)

    def apply_cell_update(
        self, coordinate: CellCoordinate, value: CellValue
    ) -> SynuiResult[None]:
        ffi = self._abi.ffi
        abi_coordinate = ffi.new("synui_cell_coordinate_t *")
        abi_coordinate.struct_size = ffi.sizeof("synui_cell_coordinate_t")
        abi_coordinate.row = coordinate.row
        abi_coordinate.column = coordinate.column
        abi_value, keepalive = self._value(ffi, value)
        diagnostic = self._invoke(
            self._abi.lib.synui_virtual_data_table_model_apply_cell_update,
            abi_coordinate,
            abi_value,
        )
        _ = keepalive
        return SynuiResult(None, diagnostic)

    def notify_sort(self, sort: Sequence[SortDescriptor]) -> SynuiResult[None]:
        ffi = self._abi.ffi
        values = ffi.new("synui_sort_descriptor_t[]", len(sort)) if sort else ffi.NULL
        for index, item in enumerate(sort):
            values[index].struct_size = ffi.sizeof("synui_sort_descriptor_t")
            values[index].column = item.column
            values[index].direction = int(item.direction)
        diagnostic = self._invoke(
            self._abi.lib.synui_virtual_data_table_model_notify_sort,
            values,
            len(sort),
        )
        return SynuiResult(None, diagnostic)

    def notify_selection(self, ranges: Sequence[CellRange]) -> SynuiResult[None]:
        ffi = self._abi.ffi
        values = ffi.new("synui_cell_range_t[]", len(ranges)) if ranges else ffi.NULL
        for index, item in enumerate(ranges):
            values[index] = self._range(ffi, item)[0]
        diagnostic = self._invoke(
            self._abi.lib.synui_virtual_data_table_model_notify_selection,
            values,
            len(ranges),
        )
        return SynuiResult(None, diagnostic)

    @staticmethod
    def _range(ffi: Any, value: CellRange) -> Any:
        output = ffi.new("synui_cell_range_t *")
        output.struct_size = ffi.sizeof("synui_cell_range_t")
        output.row_start = value.row_start
        output.row_end = value.row_end
        output.column_start = value.column_start
        output.column_end = value.column_end
        return output

    @staticmethod
    def _prefetch(ffi: Any, value: PrefetchBand) -> Any:
        output = ffi.new("synui_prefetch_band_t *")
        output.struct_size = ffi.sizeof("synui_prefetch_band_t")
        output.rows_before = value.rows_before
        output.rows_after = value.rows_after
        output.columns_before = value.columns_before
        output.columns_after = value.columns_after
        return output

    @staticmethod
    def _value(ffi: Any, value: CellValue) -> Tuple[Any, Any]:
        output = ffi.new("synui_cell_value_t *")
        output.struct_size = ffi.sizeof("synui_cell_value_t")
        keepalive = None
        if value is None:
            output.kind = 0
        elif isinstance(value, str):
            output.kind = 1
            encoded = value.encode("utf-8")
            if encoded:
                keepalive = ffi.new("unsigned char[]", encoded)
                output.text.ptr = keepalive
                output.text.len = len(encoded)
            else:
                output.text.ptr = ffi.NULL
                output.text.len = 0
        elif isinstance(value, bool):
            output.kind = 2
            output.boolean = int(value)
        elif isinstance(value, (int, float)):
            output.kind = 3
            output.number = float(value)
        else:
            raise TypeError("unsupported cell value {!r}".format(type(value)))
        return output, keepalive
