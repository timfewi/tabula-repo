"""Select and render complete context sections within an exact UTF-8 byte budget."""

import json

from .cards import inspect_repository
from .evidence import test_states
from .operations import compare
from .receipts import load_receipt
from .storage import TabulaError, requested_files


def render(result, as_json):
    if as_json:
        return json.dumps(result, ensure_ascii=False, separators=(",", ":")) + "\n"
    if "selection" not in result:
        return "\n".join(f"{key}: {value}" for key, value in result.items()) + "\n"
    lines = [
        "Tabula context (UTF-8 bytes)",
        "Caller assertions are not proof of correctness or test execution.",
    ]
    for key in (
        "status",
        "selection",
        "unknown_paths",
        "unrelated_sections",
        "omitted",
        "related_topics",
        "omitted_topics",
        "tests",
    ):
        lines.append(f"{key}: {json.dumps(result[key], ensure_ascii=False)}")
    for section in result["sections"]:
        metadata = {key: value for key, value in section.items() if key != "text"}
        lines.extend(["", json.dumps(metadata, ensure_ascii=False), section["text"]])
    return "\n".join(lines) + "\n"


def context(root, names, max_bytes, as_json):
    if max_bytes <= 0:
        raise TabulaError("--max-bytes must be a positive UTF-8 byte budget")
    selected = requested_files(root, names)
    report = inspect_repository(root)
    if report["diagnostics"]:
        return {"status": "error", "diagnostics": report["diagnostics"]}
    stored = load_receipt(root)
    comparison = compare(report, stored)
    all_sections = {
        key: section
        for card in report["cards"].values()
        for key, section in card["sections"].items()
    }
    direct = [
        key
        for key, section in all_sections.items()
        if section["sources"].keys() & selected
    ]
    candidates = [(key, "exact source dependency") for key in direct]
    if direct:
        selected_cards = {"entry"} | {key.split("/", 1)[0] for key in direct}
        for identity, card in report["cards"].items():
            if identity in selected_cards:
                key = next(iter(card["sections"]))
                if key not in direct:
                    candidates.append((key, "card orientation"))
    related = {}
    direct_cards = {key.split("/", 1)[0] for key in direct}
    for identity, card in report["cards"].items():
        if identity == "entry" or identity in direct_cards:
            for topic in card["topics"]:
                if topic["id"] in direct_cards:
                    related.setdefault(topic["id"], topic)
    result = {
        "status": "ok" if direct else "no_matches",
        "budget_unit": "UTF-8 bytes",
        "selection": "exact dependencies first, then card orientation and related topic links",
        "unknown_paths": sorted(selected - report["sources"].keys()),
        "unrelated_sections": len(all_sections) - len(candidates),
        "sections": [],
        "omitted": [
            {"id": key, "reason": reason + ": byte budget"}
            for key, reason in candidates
        ],
        "related_topics": [],
        "omitted_topics": sorted(related),
        "tests": test_states(stored.get("tests", []), report["sources"], selected),
    }
    if len(render(result, as_json).encode()) > max_bytes:
        raise TabulaError(
            "byte budget cannot fit minimal context metadata; increase --max-bytes"
        )
    for key, reason in candidates:
        identity = key.split("/", 1)[0]
        card = report["cards"][identity]
        section = all_sections[key]
        item = {
            "id": key,
            "card": card["path"],
            "line": section["line"],
            "reason": reason,
            "freshness": comparison["sections"][key]["freshness"],
            "review": comparison["sections"][key]["review"],
            "card_sha256": card["sha256"],
            "section_fingerprint": section["fingerprint"],
            "sources": section["sources"],
            "resolved_links": [target for _, target in section["links"]],
            "text": section["text"],
        }
        omitted = result["omitted"]
        result["omitted"] = [item for item in omitted if item["id"] != key]
        result["sections"].append(item)
        if len(render(result, as_json).encode()) > max_bytes:
            result["sections"].pop()
            result["omitted"] = omitted
    for identity, topic in sorted(related.items()):
        omitted = result["omitted_topics"]
        result["omitted_topics"] = [item for item in omitted if item != identity]
        result["related_topics"].append(topic)
        if len(render(result, as_json).encode()) > max_bytes:
            result["related_topics"].pop()
            result["omitted_topics"] = omitted
    return result
