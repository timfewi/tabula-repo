"""Capture test-input hashes and import caller-supplied test results."""

from .cards import snapshot
from .limits import TEST_COUNT, TEST_LIMIT, VERSION
from .receipts import load_receipt, validate_test, write_receipt
from .storage import TabulaError, parse_json, requested_files


def test_states(records, sources, selected=None):
    results = []
    for record in records:
        if selected is not None and not (record["sources"].keys() & selected):
            continue
        results.append(
            {
                "check_id": record["check_id"],
                "outcome": record["outcome"],
                "provenance": record["provenance"],
                "applicability": "current"
                if all(
                    sources.get(name) == value
                    for name, value in record["sources"].items()
                )
                else "stale",
                "sources": record["sources"],
            }
        )
    return {
        "status": "recorded" if results else "missing",
        "basis": "caller assertions, not proof of execution",
        "results": results,
    }


def test_snapshot(root, names):
    current = snapshot(root)
    selected = requested_files(root, names)
    if not selected <= current["sources"].keys():
        raise TabulaError(
            "test snapshot paths must be cited source files; inspect doctor"
        )
    return {
        "version": 1,
        "check_id": "REPLACE_WITH_CHECK_ID",
        "outcome": "unknown",
        "provenance": "untrusted",
        "sources": {name: current["sources"][name] for name in sorted(selected)},
    }


def import_test(root, data):
    if len(data) > TEST_LIMIT:
        raise TabulaError("test evidence exceeds its byte budget", "oversized")
    record = parse_json(data)
    validate_test(root, record)
    state = load_receipt(root)
    if state["version"] != VERSION:
        raise TabulaError("use update --upgrade before importing test evidence")
    current = snapshot(root)
    if not record["sources"].keys() <= current["sources"].keys():
        raise TabulaError("imported test inputs must be cited source files")
    records = [
        item for item in state["tests"] if item["check_id"] != record["check_id"]
    ]
    if len(records) >= TEST_COUNT:
        raise TabulaError(
            "too many test results; replace an existing check identity", "oversized"
        )
    records.append(record)
    state["tests"] = sorted(records, key=lambda item: item["check_id"])
    write_receipt(root, state)
    return {
        "status": "imported",
        "tests": test_states(state["tests"], current["sources"]),
    }
