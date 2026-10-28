"""Production-ABI fixture for a bounded 100,000-row virtual table."""

import os

from synui import (
    Abi,
    CellCoordinate,
    CellRange,
    PrefetchBand,
    Runtime,
    SortDescriptor,
    SortDirection,
    TableDimensions,
    VirtualDataTableModel,
)


class RecordingTable:
    def __init__(self):
        self.requests = []
        self.updates = []
        self.sorts = []
        self.selections = []

    def dimensions(self):
        return TableDimensions(100_000, 8)

    def request_cells(self, request):
        self.requests.append(request)

    def apply_cell_update(self, coordinate, value):
        self.updates.append((coordinate, value))

    def sort_changed(self, sort):
        self.sorts.append(sort)

    def selection_changed(self, ranges):
        self.selections.append(ranges)


def main():
    abi = Abi.load(os.environ.get("SYNUI_ABI_LIBRARY"))
    runtime = Runtime.create(abi).unwrap()
    app = runtime.create_app().unwrap()
    provider = RecordingTable()
    model = VirtualDataTableModel.create(app, provider).unwrap()

    request = model.request_visible(
        CellRange(50_000, 50_020, 1, 4),
        PrefetchBand(10, 10, 2, 3),
    ).unwrap()
    assert request.range == CellRange(49_990, 50_030, 0, 7)
    assert request.request_id == 1
    assert provider.requests == [request]
    assert all(
        item.range.row_end - item.range.row_start < 100_000
        for item in provider.requests
    )

    model.apply_cell_update(CellCoordinate(50_001, 2), "updated").unwrap()
    model.notify_sort([SortDescriptor(2, SortDirection.DESCENDING)]).unwrap()
    model.notify_selection([CellRange(50_000, 50_003, 1, 3)]).unwrap()
    assert provider.updates == [(CellCoordinate(50_001, 2), "updated")]
    assert provider.sorts == [[SortDescriptor(2, SortDirection.DESCENDING)]]
    assert provider.selections == [[CellRange(50_000, 50_003, 1, 3)]]

    model.close()
    app.close()
    runtime.close()


if __name__ == "__main__":
    main()
