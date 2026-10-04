"""Create and verify small, source-bound repository entrypoints offline."""

import argparse
import hashlib
import json
import os
import re
import stat
import tempfile
from pathlib import Path
from urllib.parse import unquote, urlsplit

ENTRY_LIMIT = 10_000
SOURCE_LIMIT = 2 * 1024 * 1024
SOURCE_COUNT = 64
LINK = re.compile(r"\[[^\]\n]*\]\(([^\s)]+)\)")
FORBIDDEN_PARTS = {".git", ".ssh", ".gnupg", "node_modules", ".direnv", "__pycache__"}
FORBIDDEN_SUFFIXES = {".key", ".pem", ".db", ".sqlite", ".sqlite3", ".log"}
DECLARATIONS = (
    "README.md",
    "AGENTS.md",
    "flake.nix",
    ".project-checks.json",
    "pyproject.toml",
    "package.json",
    "Cargo.toml",
)


class TabulaError(ValueError):
    """An input contract violation with a safe diagnostic."""


def safe_path(root, relative):
    path = Path(os.path.abspath(root / relative))
    if not path.is_relative_to(root):
        raise TabulaError("path escapes the repository")
    parts = path.relative_to(root).parts
    if not parts or any(
        part in FORBIDDEN_PARTS or part == ".env" or part.startswith(".env.")
        for part in parts
    ):
        raise TabulaError("credential or runtime path is not source evidence")
    if path.suffix.lower() in FORBIDDEN_SUFFIXES:
        raise TabulaError("credential or runtime file is not source evidence")
    current = root
    for part in parts:
        current /= part
        if current.is_symlink():
            raise TabulaError("symlink is not source evidence")
    return path


def read_file(path, limit):
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(fd, "rb") as stream:
        info = os.fstat(stream.fileno())
        if not stat.S_ISREG(info.st_mode) or info.st_size > limit:
            raise TabulaError(f"expected a regular file of at most {limit} bytes")
        data = stream.read(limit + 1)
    if len(data) > limit:
        raise TabulaError("file exceeded its read budget")
    return data


def digest(data):
    return hashlib.sha256(data).hexdigest()


def snapshot(root, entry=None):
    if entry is None:
        entry = read_file(safe_path(root, ".tabula/ENTRY.md"), ENTRY_LIMIT)
    text = entry.decode("utf-8")
    if not text.startswith("# "):
        raise TabulaError("ENTRY.md must begin with a repository heading")
    sources = {}
    for target in LINK.findall(text):
        url = urlsplit(target)
        if url.scheme or url.netloc or not url.path:
            continue
        if url.query or url.path.startswith("/"):
            raise TabulaError("source links must be relative paths without a query")
        path = safe_path(root, Path(".tabula") / unquote(url.path))
        if path.is_dir():
            continue
        name = path.relative_to(root).as_posix()
        if name.startswith(".tabula/"):
            raise TabulaError("cite repository source, not generated Tabula state")
        if name in sources:
            continue
        if len(sources) >= SOURCE_COUNT:
            raise TabulaError("too many cited source files")
        sources[name] = digest(read_file(path, SOURCE_LIMIT))
    if not sources:
        raise TabulaError("ENTRY.md needs at least one relative source-file link")
    return {
        "version": 1,
        "entry_sha256": digest(entry),
        "sources": dict(sorted(sources.items())),
    }


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


def update(root):
    state = snapshot(root)
    write_atomic(
        root, ".tabula/state.json", (json.dumps(state, indent=2) + "\n").encode()
    )
    return {"status": "updated", "sources": len(state["sources"])}


def initialize(root, purpose):
    if not purpose.strip() or "\n" in purpose or "\r" in purpose:
        raise TabulaError("purpose must be a nonempty single line")
    directory = safe_path(root, ".tabula")
    entry = safe_path(root, ".tabula/ENTRY.md")
    if entry.exists() or safe_path(root, ".tabula/state.json").exists():
        raise TabulaError("Tabula already exists; edit the card and use update")
    paths = [name for name in DECLARATIONS if safe_path(root, name).is_file()]
    if not paths:
        raise TabulaError("no repository declarations found; add a README first")
    text = f"# {root.name}\n\n{purpose.strip()}\n\n## Source entrypoints\n\n"
    text += "\n".join(f"- [{name}](../{name})" for name in paths) + "\n"
    text += "\nRead the applicable instructions and declared checks before editing.\n"
    if len(text.encode()) > ENTRY_LIMIT:
        raise TabulaError("entrypoint exceeds the size budget")
    state = snapshot(root, text.encode())
    directory.mkdir(exist_ok=True)
    with entry.open("xb") as stream:
        stream.write(text.encode())
    try:
        write_atomic(
            root, ".tabula/state.json", (json.dumps(state, indent=2) + "\n").encode()
        )
    except OSError:
        entry.unlink()
        raise
    result = {"sources": len(state["sources"])}
    return {
        **result,
        "status": "created",
        "review": "Expand ownership and flow from source; generated links alone are not architecture.",
    }


def check(root):
    current = snapshot(root)
    stored = json.loads(read_file(safe_path(root, ".tabula/state.json"), 32_000))
    if (
        not isinstance(stored, dict)
        or stored.get("version") != 1
        or not isinstance(stored.get("sources"), dict)
        or not isinstance(stored.get("entry_sha256"), str)
        or not re.fullmatch(r"[0-9a-f]{64}", stored["entry_sha256"])
        or any(
            not isinstance(value, str) or not re.fullmatch(r"[0-9a-f]{64}", value)
            for value in stored["sources"].values()
        )
    ):
        raise TabulaError("unsupported or malformed Tabula receipt")
    changed = sorted(
        name
        for name in current["sources"].keys() | stored["sources"].keys()
        if current["sources"].get(name) != stored["sources"].get(name)
    )
    if current["entry_sha256"] != stored.get("entry_sha256"):
        changed.insert(0, ".tabula/ENTRY.md")
    return {
        "status": "stale" if changed else "current",
        "changed": changed,
        "sources": len(current["sources"]),
    }


def main(argv=None):
    parser = argparse.ArgumentParser(prog="tabula", description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    for name in ("init", "update", "check"):
        command = sub.add_parser(name)
        command.add_argument("root", nargs="?", default=".", type=Path)
        command.add_argument("--json", action="store_true")
        if name == "init":
            command.add_argument("--purpose", required=True)
    args = parser.parse_args(argv)
    try:
        root = args.root.resolve(strict=True)
        if not root.is_dir():
            raise TabulaError("repository root must be a directory")
        result = (
            initialize(root, args.purpose)
            if args.command == "init"
            else update(root)
            if args.command == "update"
            else check(root)
        )
        code = 1 if result["status"] == "stale" else 0
    except (OSError, ValueError) as error:
        # Do not echo file contents or untrusted parser diagnostics.
        result = {
            "status": "error",
            "error": type(error).__name__,
            "message": str(error)
            if isinstance(error, TabulaError)
            else "Unreadable, malformed or unsafe input; inspect the selected repository.",
        }
        code = 2
    print(
        json.dumps(result)
        if args.json
        else "\n".join(f"{key}: {value}" for key, value in result.items())
    )
    return code


if __name__ == "__main__":
    raise SystemExit(main())
