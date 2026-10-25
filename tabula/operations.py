"""Initialize cards, update review receipts, and report freshness or diagnostics."""

from .cards import inspect_repository, snapshot
from .evidence import test_states
from .limits import (
    CARD_COUNT,
    CARD_DEPTH,
    CARD_TOTAL,
    ENTRY_LIMIT,
    REGISTRY_LIMIT,
    SECTION_COUNT,
    SOURCE_COUNT,
    SOURCE_LIMIT,
    STATE_LIMIT,
    TEST_COUNT,
    TEST_LIMIT,
    VERSION,
)
from .receipts import load_receipt, state_sections, write_receipt
from .storage import TabulaError, diagnostic, safe_path

DECLARATIONS = (
    "README.md",
    "AGENTS.md",
    "flake.nix",
    ".project-checks.json",
    "pyproject.toml",
    "package.json",
    "Cargo.toml",
)


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
    state = snapshot(root, text.encode())
    directory.mkdir(exist_ok=True)
    with entry.open("xb") as stream:
        stream.write(text.encode())
    try:
        write_receipt(root, state)
    except (OSError, ValueError):
        entry.unlink()
        raise
    return {
        "status": "created",
        "sources": len(state["sources"]),
        "review": "unreviewed; expand ownership and flow from source",
        "tests": test_states([], {}),
    }


def update(root, upgrade=False, acknowledge=()):
    previous = load_receipt(root, optional=True)
    if previous and previous["version"] != VERSION and not upgrade:
        raise TabulaError("older receipt: use update --upgrade for explicit migration")
    state = snapshot(root)
    state["tests"] = (previous or {}).get("tests", [])
    sections = state_sections(state)
    for key, review in (previous or {}).get("reviews", {}).items():
        if key in sections and review["fingerprint"] == sections[key]["fingerprint"]:
            state["reviews"][key] = review
    for key in acknowledge:
        if key not in sections:
            raise TabulaError("unknown section; inspect check --json for section IDs")
        state["reviews"][key] = {
            "fingerprint": sections[key]["fingerprint"],
            "card_sha256": state["cards"][key.split("/", 1)[0]]["sha256"],
        }
    write_receipt(root, state)
    return {
        "status": "updated",
        "sources": len(state["sources"]),
        "review": {
            key: "acknowledged" if key in state["reviews"] else "unreviewed"
            for key in sections
        },
        "review_basis": "Caller assertion for exact section and source bytes; not proof of correctness.",
        "tests": test_states(state["tests"], state["sources"]),
    }


def compare(report, stored):
    changed = sorted(
        name
        for name in report["sources"].keys() | stored["sources"].keys()
        if report["sources"].get(name) != stored["sources"].get(name)
    )
    if report["registration_sha256"] != stored.get("registration_sha256"):
        changed.append(".tabula/cards.json")
    previous = state_sections(stored)
    sections = {}
    for identity, card in report["cards"].items():
        old_card = stored.get("cards", {}).get(identity)
        old_hash = (
            old_card["sha256"]
            if old_card
            else stored["entry_sha256"]
            if identity == "entry" and stored["version"] == 1
            else None
        )
        if card.get("sha256") != old_hash:
            changed.append(card["path"])
        for key, section in card["sections"].items():
            old = previous.get(key)
            stale = old is None or old["fingerprint"] != section["fingerprint"]
            if stored["version"] == 1:
                stale = old_hash != card.get("sha256") or any(
                    source in changed for source in section["sources"]
                )
            review = stored.get("reviews", {}).get(key)
            sections[key] = {
                "card": identity,
                "line": section["line"],
                "heading": section["heading"],
                "freshness": "stale" if stale else "current",
                "review": "unreviewed"
                if review is None
                else "acknowledged"
                if review["fingerprint"] == section["fingerprint"]
                and all(section["sources"].values())
                else "invalidated",
                "sources": sorted(section["sources"]),
            }
    for key in sorted(previous.keys() - sections.keys()):
        sections[key] = {
            "card": key.split("/", 1)[0],
            "freshness": "removed",
            "review": "invalidated",
            "sources": sorted(previous[key]["sources"]),
        }
    for identity, card in stored.get("cards", {}).items():
        if identity not in report["cards"]:
            changed.append(card["path"])
    return {
        "status": "stale" if changed else "current",
        "changed": sorted(set(changed)),
        "sources": len(report["sources"]),
        "sections": sections,
        "affected_sections": [
            key for key, value in sections.items() if value["freshness"] != "current"
        ],
        "receipt_version": stored["version"],
        "review_basis": "Caller assertion, separate from byte freshness and tests.",
        "tests": test_states(stored.get("tests", []), report["sources"]),
        "cards": {
            key: {
                "path": card["path"],
                "bytes": card["bytes"],
                "limit": card["limit"],
                "evidence_files": len(card.get("sources", {})),
            }
            for key, card in report["cards"].items()
        },
    }


def check(root):
    report = inspect_repository(root)
    result = compare(report, load_receipt(root))
    if report["diagnostics"]:
        result.update(status="error", diagnostics=report["diagnostics"])
    return result


def doctor(root):
    report = inspect_repository(root)
    receipt = None
    try:
        receipt = load_receipt(root)
    except (OSError, ValueError) as error:
        report["diagnostics"].append(diagnostic(error, ".tabula/state.json"))
    comparison = compare(report, receipt) if receipt else None
    cards = {
        identity: {
            key: value
            for key, value in card.items()
            if key not in ("sections", "sources", "diagnostics")
        }
        | {
            "evidence_files": len(card.get("sources", {})),
            "sections": len(card["sections"]),
        }
        for identity, card in report["cards"].items()
    }
    return {
        "status": "error" if report["diagnostics"] else "ok",
        "limits": {
            "card_bytes": ENTRY_LIMIT,
            "source_bytes": SOURCE_LIMIT,
            "source_files": SOURCE_COUNT,
            "sections": SECTION_COUNT,
            "receipt_bytes": STATE_LIMIT,
            "cards": CARD_COUNT,
            "aggregate_card_bytes": CARD_TOTAL,
            "card_depth": CARD_DEPTH,
            "registration_bytes": REGISTRY_LIMIT,
            "test_import_bytes": TEST_LIMIT,
            "test_results": TEST_COUNT,
        },
        "cards": cards,
        "evidence_files": len(report["sources"]),
        "diagnostics": report["diagnostics"],
        "affected_sections": comparison["affected_sections"] if comparison else None,
        "sections": comparison["sections"] if comparison else {},
        "aggregate_card_bytes": sum(
            card["bytes"] or 0 for card in report["cards"].values()
        ),
        "receipt_version": receipt["version"] if receipt else None,
        "tests": test_states((receipt or {}).get("tests", []), report["sources"]),
    }
