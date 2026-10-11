"""Bounded pinned inputs and NEW-only publication for Linux packaging."""
import ctypes
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import tarfile
import tempfile
import urllib.request

ROOT = Path(__file__).resolve().parents[1]


def digest(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def pins():
    return json.loads((ROOT / "packaging/appimage/inputs.json").read_text())


def cached_input(entry, cache, offline=False):
    cache = Path(cache)
    cache.mkdir(parents=True, exist_ok=True)
    path = cache / entry["name"]
    if os.path.lexists(path):
        if path.is_symlink() or not path.is_file() or path.stat().st_size > entry["max_bytes"] or digest(path) != entry["sha256"]:
            raise ValueError(f"cached input checksum/type mismatch: {path}")
        return path
    if offline:
        raise ValueError(f"offline input missing: {path}")
    if not entry["url"].startswith("https://"):
        raise ValueError("packaging input requires HTTPS")
    with tempfile.NamedTemporaryFile(dir=cache, prefix=".download-", delete=False) as output:
        stage = Path(output.name)
        try:
            with urllib.request.urlopen(entry["url"], timeout=60) as response:
                if not response.url.startswith("https://"):
                    raise ValueError("download redirected away from HTTPS")
                count = 0
                while block := response.read(1024 * 1024):
                    count += len(block)
                    if count > entry["max_bytes"]:
                        raise ValueError("packaging input exceeds byte cap")
                    output.write(block)
            output.flush()
            os.fsync(output.fileno())
            if digest(stage) != entry["sha256"]:
                raise ValueError(f"download checksum mismatch: {entry['name']}")
            os.link(stage, path)  # Refuse a racing cache destination.
        finally:
            stage.unlink(missing_ok=True)
    return path


def extract_regular(archive, destination, max_bytes=512 * 1024 * 1024, max_members=20000):
    """Extract only regular files/directories into a new task-owned directory."""
    destination = Path(destination)
    destination.mkdir()
    total = 0
    seen = set()
    with tarfile.open(archive, "r:*") as source:
        for count, member in enumerate(source, 1):
            name = PurePosixPath(member.name)
            if (count > max_members or name.is_absolute() or ".." in name.parts
                    or not name.parts or member.name in seen or not (member.isfile() or member.isdir())):
                raise ValueError("unsafe or oversized packaging archive")
            seen.add(member.name)
            total += member.size
            if total > max_bytes or member.size < 0:
                raise ValueError("packaging archive exceeds expanded byte cap")
            source.extract(member, destination, filter="data")


def publish_new(source, destination):
    libc = ctypes.CDLL(None, use_errno=True)
    rename = libc.renameat2
    rename.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_int, ctypes.c_char_p, ctypes.c_uint]
    rename.restype = ctypes.c_int
    if rename(-100, os.fsencode(source), -100, os.fsencode(destination), 1) != 0:
        error = ctypes.get_errno()
        raise OSError(error, os.strerror(error), str(destination))
