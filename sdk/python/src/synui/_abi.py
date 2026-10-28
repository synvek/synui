"""cffi ABI-mode loader and Error Record ownership boundary."""

import ctypes.util
import os
import sys
from pathlib import Path
from typing import Any, Callable, List, Optional, Union

from cffi import FFI

from ._cdef import SYNUI_V1_CDEF
from ._errors import ErrorRecord, NoError, Status, SynuiError

ABI_MAJOR = 1
ABI_MINOR = 1
CAPABILITY_BASE = 1 << 0
CAPABILITY_VIRTUAL_MODEL = 1 << 1


class AbiInfo:
    """Negotiated immutable ABI information."""

    def __init__(self, major: int, minor: int, capabilities: int):
        self.major = major
        self.minor = minor
        self.capabilities = capabilities

    def __repr__(self) -> str:
        return "AbiInfo(major={!r}, minor={!r}, capabilities={!r})".format(
            self.major, self.minor, self.capabilities
        )


LibraryPath = Optional[Union[str, os.PathLike]]


class Abi:
    """Loaded Production ABI with negotiated v1 base capability."""

    def __init__(self, ffi: FFI, library: Any, library_name: str):
        self.ffi = ffi
        self.lib = library
        self.library_name = library_name
        self.info = self._negotiate()

    @classmethod
    def load(cls, library_path: LibraryPath = None) -> "Abi":
        ffi = FFI()
        ffi.cdef(SYNUI_V1_CDEF)
        errors: List[str] = []
        for candidate in cls._library_candidates(library_path):
            try:
                return cls(ffi, ffi.dlopen(candidate), str(candidate))
            except OSError as error:
                errors.append("{}: {}".format(candidate, error))
        raise OSError("unable to load SynUI Production ABI; " + "; ".join(errors))
    @staticmethod
    def _library_candidates(library_path: LibraryPath) -> List[str]:
        if library_path is not None:
            return [str(Path(library_path))]
        configured = os.environ.get("SYNUI_ABI_LIBRARY")
        if configured:
            return [configured]
        candidates: List[str] = []
        discovered = ctypes.util.find_library("synui_abi")
        if discovered:
            candidates.append(discovered)
        if sys.platform == "darwin":
            candidates.append("libsynui_abi.dylib")
        elif os.name == "nt":
            candidates.append("synui_abi.dll")
        else:
            candidates.append("libsynui_abi.so")
        return list(dict.fromkeys(candidates))

    def _negotiate(self) -> AbiInfo:
        requested = self.ffi.new(
            "synui_abi_version_t *", {"major": ABI_MAJOR, "minor": ABI_MINOR}
        )
        info = self.ffi.new("synui_abi_info_t *")
        info.struct_size = self.ffi.sizeof("synui_abi_info_t")
        self.invoke(
            self.lib.synui_abi_query,
            requested[0],
            CAPABILITY_BASE,
            info,
        )
        if int(info.version.major) != ABI_MAJOR:
            raise SynuiError(
                ErrorRecord(
                    int(Status.UNSUPPORTED_ABI),
                    int(Status.UNSUPPORTED_ABI),
                    "loaded library negotiated an unexpected ABI major",
                    "expected {}, received {}".format(ABI_MAJOR, info.version.major),
                )
            )
        return AbiInfo(
            int(info.version.major),
            int(info.version.minor),
            int(info.capabilities),
        )

    def _text(self, view: Any) -> str:
        length = int(view.len)
        if length == 0:
            return ""
        if view.ptr == self.ffi.NULL:
            raise ValueError("ABI text has a NULL pointer with non-zero length")
        return bytes(self.ffi.buffer(view.ptr, length)).decode("utf-8")
    @staticmethod
    def _contract_failure(detail: str) -> SynuiError:
        return SynuiError(
            ErrorRecord(
                int(Status.INTERNAL_ERROR),
                int(Status.INTERNAL_ERROR),
                "invalid Error Record returned by Production ABI",
                detail,
            )
        )

    def _copy_record(self, pointer: Any) -> ErrorRecord:
        if pointer == self.ffi.NULL:
            raise self._contract_failure("out_error was not populated")
        try:
            return ErrorRecord(
                int(pointer.status),
                int(pointer.code),
                self._text(pointer.message),
                self._text(pointer.detail),
            )
        except (UnicodeDecodeError, ValueError) as error:
            raise self._contract_failure(str(error))
        finally:
            self.lib.synui_error_record_free(pointer)

    def invoke(self, function: Callable[..., int], *arguments: Any) -> NoError:
        """Invoke one status command and consume its ABI-owned Error Record."""

        out_error = self.ffi.new("synui_error_record_t **")
        status = int(function(*arguments, out_error))
        record = self._copy_record(out_error[0])
        if record.status != status or record.code != status:
            raise self._contract_failure(
                "status {}, record.status {}, record.code {}".format(
                    status, record.status, record.code
                )
            )
        if status != int(Status.OK):
            raise SynuiError(record)
        if record.message or record.detail:
            raise self._contract_failure("successful record carried failure text")
        return NoError()
