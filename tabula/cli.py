"""Create and verify bounded, source-bound repository context offline."""

import argparse
import sys
from pathlib import Path

from .context import context, render
from .evidence import import_test, test_snapshot
from .limits import TEST_LIMIT
from .operations import check, doctor, initialize, update
from .storage import TabulaError, diagnostic


def main(argv=None):
    parser = argparse.ArgumentParser(prog="tabula", description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    for name in (
        "init",
        "update",
        "check",
        "doctor",
        "context",
        "test-snapshot",
        "import-test",
    ):
        command = sub.add_parser(name)
        command.add_argument("root", nargs="?", default=".", type=Path)
        command.add_argument("--json", action="store_true")
        if name == "init":
            command.add_argument("--purpose", required=True)
        if name == "update":
            command.add_argument("--upgrade", action="store_true")
            command.add_argument("--review", nargs="+", default=[], metavar="SECTION")
        if name in ("context", "test-snapshot"):
            command.add_argument("--files", nargs="+", required=True, metavar="PATH")
        if name == "context":
            command.add_argument("--max-bytes", type=int, required=True)
    args = parser.parse_args(argv)
    try:
        root = args.root.resolve(strict=True)
        if not root.is_dir():
            raise TabulaError("repository root must be a directory")
        if args.command == "init":
            result = initialize(root, args.purpose)
        elif args.command == "update":
            result = update(root, args.upgrade, args.review)
        elif args.command == "doctor":
            result = doctor(root)
        elif args.command == "context":
            result = context(root, args.files, args.max_bytes, args.json)
        elif args.command == "test-snapshot":
            result = test_snapshot(root, args.files)
        elif args.command == "import-test":
            result = import_test(root, sys.stdin.buffer.read(TEST_LIMIT + 1))
        else:
            result = check(root)
        code = (
            2
            if result.get("status") == "error"
            else 1
            if result.get("status") == "stale"
            else 0
        )
    except (OSError, ValueError) as error:
        result = {"status": "error", **diagnostic(error)}
        code = 2
    output = render(result, args.json)
    if args.command == "context" and (
        args.max_bytes <= 0 or len(output.encode()) > args.max_bytes
    ):
        print(
            "tabula: byte budget cannot fit response; increase --max-bytes",
            file=sys.stderr,
        )
        return 2
    if hasattr(sys.stdout, "buffer"):
        sys.stdout.buffer.write(output.encode("utf-8"))
    else:
        sys.stdout.write(output)
    return code
