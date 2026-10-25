"""Safe local paths, bounded reads, atomic writes and strict JSON decoding."""

import hashlib
import json
import os
import stat
import tempfile
from pathlib import Path

from .limits import REGISTRY_LIMIT, SOURCE_COUNT

FORBIDDEN_PARTS = {".git", ".ssh", ".gnupg", "node_modules", ".direnv", "__pycache__"}
FORBIDDEN_SUFFIXES = {".key", ".pem", ".db", ".sqlite", ".sqlite3", ".log"}


class TabulaError(ValueError):
    """An input contract violation containing only safe diagnostic text."""

    def __init__(self, message, kind="malformed", file=None, line=None):
        super().__init__(message)
        self.kind = kind
        self.file = file
        self.line = line


def safe_path(root, relative):
    try:
        str(relative).encode("utf-8")
    except UnicodeEncodeError as error:
        raise TabulaError("paths must be valid UTF-8", "unsafe") from error
    if any(ord(char) < 32 or ord(char) == 127 for char in str(relative)):
        raise TabulaError("control characters are not allowed in paths", "unsafe")
    path = Path(os.path.abspath(root / relative))
    if not path.is_relative_to(root):
        raise TabulaError("path escapes the repository", "unsafe")
    parts = path.relative_to(root).parts
    if not parts or any(
        part in FORBIDDEN_PARTS or part == ".env" or part.startswith(".env.")
        for part in parts
    ):
        raise TabulaError("credential or runtime path is not source evidence", "unsafe")
    if path.suffix.lower() in FORBIDDEN_SUFFIXES:
        raise TabulaError("credential or runtime file is not source evidence", "unsafe")
    current = root
    for part in parts:
        current /= part
        if current.is_symlink():
            raise TabulaError("symlink is not source evidence", "unsafe")
    return path


def read_file(path, limit):
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(fd, "rb") as stream:
        info = os.fstat(stream.fileno())
        if not stat.S_ISREG(info.st_mode):
            raise TabulaError("expected a regular file", "unsafe")
        if info.st_size > limit:
            raise TabulaError(f"file exceeds {limit} bytes", "oversized")
        data = stream.read(limit + 1)
    if len(data) > limit:
        raise TabulaError("file exceeded its read budget", "oversized")
    return data


def digest(data):
    return hashlib.sha256(data).hexdigest()


def canonical(value):
    return json.dumps(
        value, sort_keys=True, ensure_ascii=True, separators=(",", ":")
    ).encode()


def diagnostic(error, file=None, line=None):
    return {
        "kind": error.kind
        if isinstance(error, TabulaError)
        else "missing"
        if isinstance(error, FileNotFoundError)
        else "malformed"
        if isinstance(error, (ValueError, UnicodeError))
        else "unreadable",
        "file": getattr(error, "file", None) or file,
        "line": getattr(error, "line", None) or line,
        "message": str(error)
        if isinstance(error, TabulaError)
        else "Unreadable or malformed input; inspect the selected file.",
    }


def parse_json(data):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise TabulaError("duplicate JSON field")
            result[key] = value
        return result

    try:
        return json.loads(data, object_pairs_hook=unique)
    except RecursionError as error:
        raise TabulaError("JSON nesting is too deep", "oversized") from error


def write_atomic(root, relative, data):
    path = safe_path(root, relative)
    with tempfile.NamedTemporaryFile(
        dir=path.parent, prefix=".tabula-write-", delete=False
    ) as stream:
        temporary = Path(stream.name)
        try:
            stream.write(data)
            stream.flush()
            os.replace(temporary, path)
        finally:
            temporary.unlink(missing_ok=True)


def requested_files(root, names):
    if (
        not names
        or len(names) > SOURCE_COUNT
        or sum(len(name.encode()) for name in names) > REGISTRY_LIMIT
    ):
        raise TabulaError(
            "request between 1 and 64 repository-relative source paths within 8000 bytes"
        )
    result = set()
    for name in names:
        if Path(name).is_absolute():
            raise TabulaError("requested files must be repository-relative", "unsafe")
        path = safe_path(root, name)
        result.add(path.relative_to(root).as_posix())
    return result
