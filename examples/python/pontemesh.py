from __future__ import annotations

import ctypes
import os
import platform
from pathlib import Path
from typing import Callable


class TransferSummary(ctypes.Structure):
    _fields_ = [
        ("bytes_from_peer", ctypes.c_uint64),
        ("bytes_from_replica", ctypes.c_uint64),
        ("bytes_from_origin", ctypes.c_uint64),
        ("fragments_from_peer", ctypes.c_uint64),
        ("fragments_from_replica", ctypes.c_uint64),
        ("fragments_from_origin", ctypes.c_uint64),
        ("peer_failures", ctypes.c_uint64),
        ("peer_hash_failures", ctypes.c_uint64),
        ("peer_rejected_fragments", ctypes.c_uint64),
        ("fallback_activations", ctypes.c_uint64),
    ]


ProgressCallback = ctypes.CFUNCTYPE(
    None,
    ctypes.c_uint32,
    ctypes.c_uint64,
    ctypes.c_uint64,
    ctypes.c_char_p,
    ctypes.c_void_p,
)


def library_filename() -> str:
    system = platform.system()
    if system == "Windows":
        return "pontemesh_sdk.dll"
    if system == "Darwin":
        return "libpontemesh_sdk.dylib"
    return "libpontemesh_sdk.so"


def find_library(repo_root: Path) -> Path:
    configured = os.environ.get("PONTEMESH_SDK_LIBRARY")
    candidates = [
        Path(configured) if configured else None,
        repo_root / "native" / library_filename(),
        repo_root.parent / "pontemesh-sdk" / "target" / "release" / library_filename(),
    ]
    for candidate in candidates:
        if candidate and candidate.is_file():
            return candidate.resolve()
    raise FileNotFoundError(
        f"Could not find {library_filename()}. Set PONTEMESH_SDK_LIBRARY or extract an SDK release into native/."
    )


class NativeSdk:
    def __init__(self, library_path: Path):
        self.library = ctypes.CDLL(str(library_path))
        self.library.pontemesh_client_create.argtypes = [
            ctypes.c_char_p,
            ctypes.c_char_p,
            ctypes.POINTER(ctypes.c_void_p),
        ]
        self.library.pontemesh_client_create.restype = ctypes.c_int
        self.library.pontemesh_client_sync_object_with_summary_and_progress.argtypes = [
            ctypes.c_void_p,
            ctypes.c_char_p,
            ctypes.c_char_p,
            ctypes.c_char_p,
            ctypes.POINTER(TransferSummary),
            ProgressCallback,
            ctypes.c_void_p,
        ]
        self.library.pontemesh_client_sync_object_with_summary_and_progress.restype = ctypes.c_int
        self.library.pontemesh_client_get_last_error.argtypes = [
            ctypes.c_void_p,
            ctypes.c_char_p,
            ctypes.c_size_t,
        ]
        self.library.pontemesh_client_get_last_error.restype = ctypes.c_int
        self.library.pontemesh_client_free.argtypes = [ctypes.c_void_p]
        self.library.pontemesh_client_free.restype = None


class PontemeshClient:
    def __init__(self, sdk: NativeSdk, origin_url: str, application_token: str):
        self._sdk = sdk
        self._client = ctypes.c_void_p()
        status = sdk.library.pontemesh_client_create(
            origin_url.encode(), application_token.encode(), ctypes.byref(self._client)
        )
        if status != 0:
            raise RuntimeError(f"pontemesh_client_create failed with status {status}")

    def close(self) -> None:
        if self._client:
            self._sdk.library.pontemesh_client_free(self._client)
            self._client = ctypes.c_void_p()

    def __enter__(self) -> "PontemeshClient":
        return self

    def __exit__(self, *_: object) -> None:
        self.close()

    def sync_object(
        self,
        bucket: str,
        key: str,
        destination: Path,
        progress: Callable[[int, int, int, str], None] | None = None,
    ) -> TransferSummary:
        destination.parent.mkdir(parents=True, exist_ok=True)
        summary = TransferSummary()

        def report(fragment: int, downloaded: int, total: int, source: bytes, _: int) -> None:
            if progress:
                progress(fragment, downloaded, total, source.decode())

        callback = ProgressCallback(report)
        status = self._sdk.library.pontemesh_client_sync_object_with_summary_and_progress(
            self._client,
            bucket.encode(),
            key.encode(),
            str(destination).encode(),
            ctypes.byref(summary),
            callback,
            None,
        )
        if status != 0:
            raise RuntimeError(self._last_error(status))
        return summary

    def _last_error(self, status: int) -> str:
        buffer = ctypes.create_string_buffer(2048)
        self._sdk.library.pontemesh_client_get_last_error(
            self._client, buffer, len(buffer)
        )
        message = buffer.value.decode(errors="replace")
        return message or f"Ponte Mesh SDK failed with status {status}"
