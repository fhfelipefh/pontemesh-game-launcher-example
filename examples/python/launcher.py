from __future__ import annotations

import hashlib
import json
import os
import shutil
import sys
import tempfile
import tomllib
from pathlib import Path, PurePosixPath
from urllib.parse import urlparse

from pontemesh import NativeSdk, PontemeshClient, TransferSummary, find_library

MAX_RELEASE_BYTES = 20 * 1024 * 1024 * 1024
MAX_RELEASE_FILES = 10_000
REPO_ROOT = Path(__file__).resolve().parents[2]


def load_config(path: Path) -> dict[str, object]:
    with path.open("rb") as stream:
        config = tomllib.load(stream)
    config["origin_url"] = os.environ.get(
        "PONTEMESH_ORIGIN_URL", str(config.get("origin_url", "http://127.0.0.1:8080"))
    )
    config["application_token"] = os.environ.get(
        "PONTEMESH_APPLICATION_TOKEN", str(config.get("application_token", ""))
    )
    origin = urlparse(str(config["origin_url"]))
    if origin.scheme not in {"http", "https"} or not origin.hostname:
        raise ValueError("origin_url must be an HTTP or HTTPS URL with a host")
    for name in ("application_token", "release_bucket", "release_manifest_key"):
        if not str(config.get(name, "")).strip():
            raise ValueError(f"{name} cannot be empty")
    return config


def validate_manifest(value: object) -> dict[str, object]:
    if not isinstance(value, dict) or value.get("schemaVersion") != 1:
        raise ValueError("release descriptor must use schemaVersion 1")
    version = value.get("version")
    files = value.get("files")
    if not isinstance(version, str) or not version.strip():
        raise ValueError("release version cannot be empty")
    if not isinstance(files, list) or not files or len(files) > MAX_RELEASE_FILES:
        raise ValueError("release must contain between 1 and 10,000 files")
    seen: set[str] = set()
    total = 0
    for item in files:
        if not isinstance(item, dict):
            raise ValueError("every release file must be an object")
        for field in ("bucket", "key", "path", "sha256"):
            if not isinstance(item.get(field), str) or not item[field].strip():
                raise ValueError(f"release file {field} cannot be empty")
        relative = PurePosixPath(item["path"])
        if relative.is_absolute() or ".." in relative.parts or "\\" in item["path"]:
            raise ValueError(f"unsafe release path: {item['path']}")
        if item["path"] in seen:
            raise ValueError(f"duplicate release path: {item['path']}")
        seen.add(item["path"])
        size = item.get("sizeBytes")
        if not isinstance(size, int) or size < 0:
            raise ValueError("sizeBytes must be a non-negative integer")
        digest = item["sha256"]
        if len(digest) != 64 or any(character not in "0123456789abcdefABCDEF" for character in digest):
            raise ValueError("sha256 must contain 64 hexadecimal characters")
        total += size
    if total > MAX_RELEASE_BYTES:
        raise ValueError("release exceeds the 20 GiB example limit")
    value["files"] = sorted(files, key=lambda item: int(item.get("order", 0)))
    return value


def replace_installation(install_root: Path, staging: Path) -> None:
    rollback = install_root.with_name(f"{install_root.name}.rollback")
    if rollback.exists():
        shutil.rmtree(rollback)
    if install_root.exists():
        install_root.rename(rollback)
    try:
        staging.rename(install_root)
    except Exception:
        if rollback.exists():
            rollback.rename(install_root)
        raise
    if rollback.exists():
        shutil.rmtree(rollback)


def merge_summary(total: TransferSummary, current: TransferSummary) -> None:
    for name, _ in TransferSummary._fields_:
        setattr(total, name, getattr(total, name) + getattr(current, name))


def install(config_path: Path = REPO_ROOT / "launcher.toml") -> Path:
    config = load_config(config_path)
    install_root = REPO_ROOT / "runtime" / "installations" / "python"
    install_root.parent.mkdir(parents=True, exist_ok=True)
    sdk = NativeSdk(find_library(REPO_ROOT))
    total = TransferSummary()
    with PontemeshClient(sdk, str(config["origin_url"]), str(config["application_token"])) as client:
        with tempfile.TemporaryDirectory(prefix="pontemesh-python-", dir=install_root.parent) as work:
            work_path = Path(work)
            descriptor = work_path / "release.json"
            client.sync_object(
                str(config["release_bucket"]),
                str(config["release_manifest_key"]),
                descriptor,
            )
            manifest = validate_manifest(json.loads(descriptor.read_text(encoding="utf-8")))
            staging = work_path / "installation"
            staging.mkdir()
            for item in manifest["files"]:
                destination = staging.joinpath(*PurePosixPath(item["path"]).parts)

                def progress(fragment: int, downloaded: int, size: int, source: str) -> None:
                    percent = downloaded * 100 // size if size else 100
                    print(f"{item['path']}: {percent:3}% (fragment {fragment + 1}, {source})")

                summary = client.sync_object(item["bucket"], item["key"], destination, progress)
                contents = destination.read_bytes()
                if len(contents) != item["sizeBytes"] or hashlib.sha256(contents).hexdigest().lower() != item["sha256"].lower():
                    raise RuntimeError(f"release verification failed for {item['path']}")
                merge_summary(total, summary)
            (staging / ".pontemesh-version").write_text(f"{manifest['version']}\n", encoding="utf-8")
            replace_installation(install_root, staging)
    print(f"\nUpdate {manifest['version']} installed in {install_root}")
    print(f"Origin: {total.bytes_from_origin} bytes; Replica/Edge: {total.bytes_from_replica}; Peers: {total.bytes_from_peer}")
    print("Game status: READY TO PLAY")
    return install_root


if __name__ == "__main__":
    try:
        install()
    except Exception as error:
        print(f"Update failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
