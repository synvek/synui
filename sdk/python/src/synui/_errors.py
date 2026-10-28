"""Stable Python result and diagnostic types for Production ABI commands."""

from dataclasses import dataclass
from enum import IntEnum
from typing import Generic, TypeVar


class Status(IntEnum):
    OK = 0
    UNSUPPORTED_ABI = 1
    UNSUPPORTED_CAPABILITY = 2
    INVALID_ARGUMENT = 3
    CONTEXT_MISMATCH = 4
    CONTEXT_DESTROYED = 5
    STALE_HANDLE = 6
    THREAD_AFFINITY = 7
    POST_CANCELLED = 8
    CALLBACK_ERROR = 9
    PANIC_CONTAINED = 10
    INTERNAL_ERROR = 11


@dataclass(frozen=True)
class NoError:
    """The ABI-owned success Error Record after copying and release."""

    status: int = int(Status.OK)
    code: int = int(Status.OK)
    message: str = ""
    detail: str = ""


@dataclass(frozen=True)
class ErrorRecord:
    """A copied failure record whose storage is owned by Python."""

    status: int
    code: int
    message: str
    detail: str


class SynuiError(RuntimeError):
    """Defined Production ABI failure with its copied Error Record."""

    def __init__(self, record: ErrorRecord):
        self.record = record
        self.status = record.status
        label = Status(record.status).name if record.status in Status._value2member_map_ else str(record.status)
        detail = " ({})".format(record.detail) if record.detail else ""
        super().__init__("{}: {}{}".format(label, record.message, detail))


T = TypeVar("T")


@dataclass(frozen=True)
class SynuiResult(Generic[T]):
    """Successful value plus the required no-failure diagnostic record."""

    value: T
    diagnostic: NoError

    def unwrap(self) -> T:
        return self.value
