"""Inspect registered cards and their cited sources without executing project code."""

import re
from pathlib import Path
from urllib.parse import unquote, urlsplit

from .limits import (
    CARD_COUNT,
    CARD_DEPTH,
    CARD_TOTAL,
    ENTRY_LIMIT,
    REGISTRY_LIMIT,
    SOURCE_COUNT,
    SOURCE_LIMIT,
    VERSION,
)
from .markdown import markdown
from .storage import (
    TabulaError,
    canonical,
    diagnostic,
    digest,
    parse_json,
    read_file,
    safe_path,
)


def inspect_card(
    root,
    name,
    data=None,
    *,
    identity="entry",
    registrations=None,
    sources=None,
    source_errors=None,
    remaining=CARD_TOTAL,
):
    registrations = registrations or {".tabula/ENTRY.md": "entry"}
    sources = sources if sources is not None else {}
    source_errors = source_errors if source_errors is not None else {}
    report = {
        "path": name,
        "bytes": None,
        "limit": ENTRY_LIMIT,
        "remaining_bytes": None,
        "exceeded_bytes": None,
        "sections": {},
        "navigation": [],
        "topics": [],
        "diagnostics": [],
    }
    try:
        path = safe_path(root, name)
        size = len(data) if data is not None else path.stat().st_size
        report.update(
            bytes=size,
            remaining_bytes=max(0, ENTRY_LIMIT - size),
            exceeded_bytes=max(0, size - ENTRY_LIMIT),
        )
        if data is None:
            data = read_file(path, min(ENTRY_LIMIT, remaining))
        if len(data) > ENTRY_LIMIT:
            raise TabulaError("card exceeds its byte budget", "oversized")
        report["sha256"] = digest(data)
        text = data.decode("utf-8")
        if any(
            ord(char) < 32 and char not in "\r\n\t" or ord(char) == 127 for char in text
        ):
            raise TabulaError("control characters are not allowed in cards", "unsafe")
        parsed = markdown(text)
    except (OSError, ValueError) as error:
        report["diagnostics"].append(diagnostic(error, name))
        return report
    card_sources = {}
    for section_id, section in parsed.items():
        dependencies = {}
        for line, target in section["links"]:
            source = None
            try:
                url = urlsplit(target)
                if url.scheme or url.netloc or not url.path:
                    report["navigation"].append(
                        {
                            "line": line,
                            "kind": "external"
                            if url.scheme or url.netloc
                            else "anchor",
                        }
                    )
                    continue
                if url.query or url.path.startswith("/"):
                    raise TabulaError(
                        "source links must be relative paths without a query", "unsafe"
                    )
                path = safe_path(
                    root, Path(name).parent / unquote(url.path, errors="strict")
                )
                if path.is_dir():
                    report["navigation"].append({"line": line, "kind": "directory"})
                    continue
                source = path.relative_to(root).as_posix()
                if source in registrations:
                    topic = {
                        "line": line,
                        "kind": "topic",
                        "id": registrations[source],
                        "path": source,
                        "section": identity + "/" + section_id,
                    }
                    report["navigation"].append(topic)
                    report["topics"].append(topic)
                    continue
                if source.startswith(".tabula/"):
                    raise TabulaError(
                        "unregistered card or generated state; register Markdown cards in cards.json",
                        "unsafe",
                    )
                if source not in sources:
                    if len(sources) >= SOURCE_COUNT:
                        raise TabulaError("too many cited source files", "oversized")
                    sources[source] = None
                    try:
                        sources[source] = digest(read_file(path, SOURCE_LIMIT))
                    except (OSError, ValueError) as error:
                        source_errors[source] = error
                if source in source_errors:
                    raise source_errors[source]
                dependencies[source] = sources[source]
            except (OSError, ValueError) as error:
                report["diagnostics"].append(diagnostic(error, name, line))
                if source in sources:
                    dependencies[source] = sources[source]
        card_sources.update(dependencies)
        section["sources"] = dependencies
        section["fingerprint"] = digest(
            canonical(
                {
                    "text": section["text"],
                    "ancestors": section["ancestors"],
                    "links": [target for _, target in section["links"]],
                    "sources": dependencies,
                }
            )
        )
        report["sections"][identity + "/" + section_id] = section
    report["sources"] = card_sources
    if not card_sources:
        report["diagnostics"].append(
            diagnostic(
                TabulaError("card needs at least one relative source-file link"), name
            )
        )
    return report


def registrations(root):
    path = safe_path(root, ".tabula/cards.json")
    cards = {"entry": ".tabula/ENTRY.md"}
    if not path.exists():
        return cards, None
    data = read_file(path, REGISTRY_LIMIT)
    registry = parse_json(data)
    if (
        not isinstance(registry, dict)
        or set(registry) != {"version", "cards"}
        or type(registry["version"]) is not int
        or registry["version"] != 1
        or not isinstance(registry["cards"], list)
    ):
        raise TabulaError(
            "malformed card registration; expected version 1 and a cards list"
        )
    if len(registry["cards"]) >= CARD_COUNT:
        raise TabulaError("too many registered cards", "oversized")
    for card in registry["cards"]:
        if (
            not isinstance(card, dict)
            or set(card) != {"id", "path"}
            or not isinstance(card["id"], str)
            or not re.fullmatch(r"[a-z][a-z0-9-]{0,31}", card["id"])
            or not isinstance(card["path"], str)
        ):
            raise TabulaError(
                "card registration needs a short lowercase id and relative Markdown path"
            )
        relative = Path(card["path"])
        path = safe_path(root, Path(".tabula") / relative)
        if (
            relative.is_absolute()
            or ".." in relative.parts
            or relative.as_posix() != card["path"]
            or path.suffix != ".md"
            or len(relative.parts) > CARD_DEPTH
        ):
            raise TabulaError(
                "card path must be a normalized .md path within .tabula and its depth limit",
                "unsafe",
            )
        name = path.relative_to(root).as_posix()
        if card["id"] in cards or name in cards.values():
            raise TabulaError("duplicate card id or path")
        cards[card["id"]] = name
    return dict(
        sorted(cards.items(), key=lambda pair: (pair[0] != "entry", pair[0]))
    ), digest(data)


def inspect_repository(root, entry=None):
    report = {
        "cards": {},
        "sources": {},
        "diagnostics": [],
        "registration_sha256": None,
    }
    try:
        cards, report["registration_sha256"] = registrations(root)
    except (OSError, ValueError) as error:
        report["diagnostics"].append(diagnostic(error, ".tabula/cards.json"))
        cards = {"entry": ".tabula/ENTRY.md"}
    source_errors = {}
    remaining = CARD_TOTAL
    paths = {name: identity for identity, name in cards.items()}
    for identity, name in cards.items():
        card = inspect_card(
            root,
            name,
            entry if identity == "entry" else None,
            identity=identity,
            registrations=paths,
            sources=report["sources"],
            source_errors=source_errors,
            remaining=remaining,
        )
        remaining = max(0, remaining - (card["bytes"] or 0))
        report["cards"][identity] = card
        report["diagnostics"].extend(card["diagnostics"])

    # Deduplicate edges and memoize depth: repeated links cannot amplify traversal.
    depths = {}

    def visit(identity, trail):
        if identity in trail:
            raise TabulaError("card-link cycle; remove a topic back-link")
        if identity in depths:
            return depths[identity]
        children = {topic["id"] for topic in report["cards"][identity]["topics"]}
        depth = 1 + max(
            (visit(child, trail + (identity,)) for child in sorted(children)), default=0
        )
        if depth > CARD_DEPTH:
            raise TabulaError(
                "card-link depth exceeds the configured limit", "oversized"
            )
        depths[identity] = depth
        return depth

    try:
        for identity in cards:
            visit(identity, ())
    except TabulaError as error:
        report["diagnostics"].append(diagnostic(error, ".tabula/cards.json"))
    return report


def snapshot(root, entry=None):
    report = inspect_repository(root, entry)
    if report["diagnostics"]:
        item = report["diagnostics"][0]
        if item["kind"] == "missing":
            raise FileNotFoundError("missing source or card")
        raise TabulaError(item["message"], item["kind"], item["file"], item["line"])
    cards = {}
    for identity, card in report["cards"].items():
        cards[identity] = {
            "path": card["path"],
            "sha256": card["sha256"],
            "sections": {
                key: {
                    "fingerprint": section["fingerprint"],
                    "sources": section["sources"],
                }
                for key, section in card["sections"].items()
            },
        }
    return {
        "version": VERSION,
        "entry_sha256": cards["entry"]["sha256"],
        "sources": dict(sorted(report["sources"].items())),
        "cards": cards,
        "reviews": {},
        "registration_sha256": report["registration_sha256"],
        "tests": [],
    }
