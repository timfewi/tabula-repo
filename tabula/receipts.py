"""Validate, load and persist versioned receipts and test-evidence records."""

import json
import re
from pathlib import Path

from .limits import (
    CARD_COUNT,
    CARD_DEPTH,
    SECTION_COUNT,
    SOURCE_COUNT,
    STATE_LIMIT,
    TEST_COUNT,
    VERSION,
)
from .storage import TabulaError, parse_json, read_file, safe_path, write_atomic


def hash_value(value):
    return isinstance(value, str) and re.fullmatch(r"[0-9a-f]{64}", value) is not None


def source_map(root, value):
    if not isinstance(value, dict) or len(value) > SOURCE_COUNT:
        return False
    for key, item in value.items():
        if not isinstance(key, str) or not hash_value(item):
            return False
        path = safe_path(root, key)
        if path.relative_to(root).as_posix() != key or key.startswith(".tabula/"):
            return False
    return True


def validate_test(root, record):
    if not isinstance(record, dict) or set(record) != {
        "version",
        "check_id",
        "outcome",
        "provenance",
        "sources",
    }:
        raise TabulaError(
            "test evidence needs exactly version, check_id, outcome, provenance and sources; logs are not accepted"
        )
    if (
        type(record["version"]) is not int
        or record["version"] != 1
        or not isinstance(record["check_id"], str)
        or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._:/-]{0,79}", record["check_id"])
    ):
        raise TabulaError("test evidence needs version 1 and a short check identity")
    if (
        record["outcome"] not in ("pass", "fail", "unknown")
        or record["provenance"] not in ("caller", "untrusted")
        or not record["sources"]
        or not source_map(root, record["sources"])
    ):
        raise TabulaError(
            "test evidence needs explicit outcome, provenance and exact source hashes"
        )


def load_receipt(root, optional=False):
    path = safe_path(root, ".tabula/state.json")
    if optional and not path.exists():
        return None
    state = parse_json(read_file(path, STATE_LIMIT))
    if not isinstance(state, dict) or type(state.get("version")) is not int:
        raise TabulaError("malformed Tabula receipt")
    if state["version"] not in (1, 2, VERSION):
        raise TabulaError(
            "unsupported receipt version; use a compatible Tabula release"
        )
    fields = {"version", "entry_sha256", "sources"}
    if state["version"] >= 2:
        fields |= {"cards", "reviews"}
    if state["version"] == VERSION:
        fields |= {"registration_sha256", "tests"}
    if set(state) != fields:
        raise TabulaError("malformed receipt fields")
    if not hash_value(state.get("entry_sha256")) or not source_map(
        root, state.get("sources")
    ):
        raise TabulaError("malformed Tabula receipt")
    if state["version"] == 1:
        return state
    cards = state.get("cards")
    reviews = state.get("reviews")
    if (
        not isinstance(cards, dict)
        or "entry" not in cards
        or len(cards) > CARD_COUNT
        or not isinstance(reviews, dict)
    ):
        raise TabulaError("malformed receipt cards or reviews")
    sections = {}
    for identity, card in cards.items():
        if (
            not isinstance(card, dict)
            or set(card) != {"path", "sha256", "sections"}
            or not isinstance(card.get("path"), str)
            or not card["path"].startswith(".tabula/")
            or not card["path"].endswith(".md")
            or not hash_value(card.get("sha256"))
            or not isinstance(card.get("sections"), dict)
            or len(card["sections"]) > SECTION_COUNT
        ):
            raise TabulaError("malformed receipt card")
        if (
            safe_path(root, card["path"]).relative_to(root).as_posix() != card["path"]
            or len(Path(card["path"]).parts) - 1 > CARD_DEPTH
        ):
            raise TabulaError("malformed receipt card path")
        if not re.fullmatch(r"[a-z][a-z0-9-]{0,31}", identity):
            raise TabulaError("malformed receipt card identity")
        for key, section in card["sections"].items():
            if (
                not re.fullmatch(
                    re.escape(identity) + r"/[1-6]-[\w-]+-[1-9][0-9]*", key
                )
                or not isinstance(section, dict)
                or set(section) != {"fingerprint", "sources"}
                or not hash_value(section.get("fingerprint"))
                or not source_map(root, section.get("sources"))
            ):
                raise TabulaError("malformed receipt section")
            sections[key] = section
    for key, review in reviews.items():
        if (
            key not in sections
            or not isinstance(review, dict)
            or set(review) != {"fingerprint", "card_sha256"}
            or not hash_value(review.get("fingerprint"))
            or not hash_value(review.get("card_sha256"))
        ):
            raise TabulaError("malformed review acknowledgement")
    if (
        cards["entry"]["path"] != ".tabula/ENTRY.md"
        or cards["entry"]["sha256"] != state["entry_sha256"]
    ):
        raise TabulaError("inconsistent entry receipt")
    union = {}
    for section in sections.values():
        for name, value in section["sources"].items():
            if state["sources"].get(name) != value:
                raise TabulaError("inconsistent source receipt")
            union[name] = value
    if union != state["sources"]:
        raise TabulaError("inconsistent source inventory")
    if state.get("registration_sha256") is not None and not hash_value(
        state["registration_sha256"]
    ):
        raise TabulaError("malformed registration receipt")
    if state["version"] == VERSION:
        tests = state.get("tests")
        if not isinstance(tests, list) or len(tests) > TEST_COUNT:
            raise TabulaError("malformed test evidence inventory")
        identities = set()
        for record in tests:
            validate_test(root, record)
            if record["check_id"] in identities:
                raise TabulaError("duplicate test identity")
            identities.add(record["check_id"])
    return state


def write_receipt(root, state):
    data = (json.dumps(state, indent=2) + "\n").encode()
    if len(data) > STATE_LIMIT:
        raise TabulaError("receipt exceeds its byte budget", "oversized")
    write_atomic(root, ".tabula/state.json", data)


def state_sections(state):
    return {
        key: section
        for card in state.get("cards", {}).values()
        for key, section in card["sections"].items()
    }
