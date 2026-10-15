"""Create and verify bounded, source-bound repository context offline."""

import argparse
import hashlib
import html
import json
import os
import re
import stat
import sys
import tempfile
from pathlib import Path
from urllib.parse import unquote, urlsplit

ENTRY_LIMIT = 10_000
SOURCE_LIMIT = 2 * 1024 * 1024
SOURCE_COUNT = 64
STATE_LIMIT = 512_000
SECTION_COUNT = 256
VERSION = 3
CARD_COUNT = 16
CARD_TOTAL = 80_000
CARD_DEPTH = 4
REGISTRY_LIMIT = 8_000
TEST_LIMIT = 16_000
TEST_COUNT = 16
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


def unescape(value):
    return re.sub(r"\\([!\"#$%&'()*+,\-./:;<=>?@\[\]\\^_`{|}~])", r"\1", value)


def label_key(value):
    return " ".join(unescape(value).split()).casefold()


def code_span(text, start):
    end = start
    while end < len(text) and text[end] == "`":
        end += 1
    boundary = re.search(
        r"\r?\n[ \t]*\r?\n|\n(?= {0,3}(?:#{1,6}\s|>|`{3}|~{3}))", text[end:]
    )
    limit = end + boundary.start() if boundary else len(text)
    match = re.search(
        r"(?<!`)" + re.escape(text[start:end]) + r"(?!`)", text[end:limit]
    )
    return (end + match.end(), True) if match else (end, False)


def bracket_end(text, start):
    depth = 1
    index = start + 1
    while index < len(text):
        if text[index] == "\\":
            index += 2
            continue
        if text[index] == "`":
            index, _ = code_span(text, index)
            continue
        if text[index] == "[":
            depth += 1
        elif text[index] == "]":
            depth -= 1
            if depth == 0:
                return index
            if text[index + 1 : index + 2] == "(":
                raise TabulaError(
                    "nested links are unsupported; use separate source links"
                )
        index += 1
    raise TabulaError("unclosed link label; escape literal brackets or use inline code")


def destination(text, start, inline):
    """Read the documented destination/title grammar, never a regex approximation."""
    index = start
    while index < len(text) and text[index].isspace():
        index += 1
    begin = index
    if index < len(text) and text[index] == "<":
        index += 1
        begin = index
        while index < len(text) and text[index] != ">":
            if text[index] in "\n<":
                raise TabulaError(
                    "invalid angle destination; close it on the same line"
                )
            index += 2 if text[index] == "\\" else 1
        if index >= len(text):
            raise TabulaError("unclosed angle destination")
        target = text[begin:index]
        index += 1
    else:
        depth = 0
        while index < len(text):
            char = text[index]
            if char == "\\":
                index += 2
                continue
            if char.isspace() or (char == ")" and depth == 0):
                break
            if char == "(":
                depth += 1
            elif char == ")":
                depth -= 1
            if char in "<>":
                raise TabulaError(
                    "use angle brackets around destinations containing spaces"
                )
            index += 1
        if depth:
            raise TabulaError(
                "unbalanced destination parentheses; escape literal delimiters"
            )
        target = text[begin:index]
    after = index
    while index < len(text) and text[index].isspace():
        index += 1
    if index > after and index < len(text) and text[index] in "\"'(":
        close = ")" if text[index] == "(" else text[index]
        index += 1
        while index < len(text) and text[index] != close:
            index += 2 if text[index] == "\\" else 1
        if index >= len(text):
            raise TabulaError("unclosed link title")
        index += 1
        while index < len(text) and text[index].isspace():
            index += 1
    if inline:
        if index >= len(text) or text[index] != ")":
            raise TabulaError(
                'malformed inline link; use [label](<relative/path> "title")'
            )
        index += 1
    elif index != len(text):
        raise TabulaError(
            "malformed reference definition; use one destination and optional title"
        )
    return html.unescape(unescape(target)), index


def blank(value):
    return "".join("\n" if char == "\n" else " " for char in value)


def markdown(text):
    """A bounded Markdown evidence dialect; not a full CommonMark parser."""
    lines = text.splitlines(keepends=True)
    masked = []
    fence = None
    headings = []
    offset = 0
    list_active = False
    indented = False
    for number, line in enumerate(lines, 1):
        marker = re.match(r"^ {0,3}(`{3,}|~{3,})(.*)$", line.rstrip("\r\n"))
        if fence:
            masked.append(blank(line))
            if (
                marker
                and marker[1][0] == fence[0]
                and len(marker[1]) >= fence[1]
                and not marker[2].strip()
            ):
                fence = None
        elif marker:
            if marker[1][0] == "`" and "`" in marker[2]:
                raise TabulaError(
                    "backtick fence info cannot contain backticks", line=number
                )
            fence = (marker[1][0], len(marker[1]))
            masked.append(blank(line))
        elif line.startswith(("    ", "\t")) and line.strip():
            if list_active:
                raise TabulaError(
                    "indented list content is unsupported; use flat single-line items",
                    line=number,
                )
            if not indented and number > 1 and lines[number - 2].strip():
                raise TabulaError(
                    "indented paragraph continuation is unsupported; separate code with a blank line",
                    line=number,
                )
            indented = True
            masked.append(blank(line))
        else:
            if line.strip():
                indented = False
                if list_active and line.startswith(("  ", "\t")):
                    raise TabulaError(
                        "nested list content is unsupported; use flat single-line items",
                        line=number,
                    )
                list_active = bool(re.match(r"^ {0,3}(?:[-+*]|[0-9]+[.)]) +", line))
            if re.match(r"^ {0,3}(>|[-+*] +(?:>|`{3}|~{3}))", line):
                raise TabulaError(
                    "container blocks are unsupported; move evidence to plain paragraphs or lists",
                    line=number,
                )
            heading = re.match(
                r"^ {0,3}(#{1,6})(?:[ \t]+|$)(.*?)(?:[ \t]+#+[ \t]*)?$",
                line.rstrip("\r\n"),
            )
            if heading:
                headings.append((offset, number, len(heading[1]), heading[2]))
            elif (
                re.fullmatch(r" {0,3}(?:=+|-+)[ \t]*\r?\n?", line)
                and masked
                and masked[-1].strip()
            ):
                previous = lines[number - 2]
                headings.append(
                    (
                        offset - len(previous),
                        number - 1,
                        1 if "=" in line else 2,
                        previous.strip(),
                    )
                )
            masked.append(line)
        offset += len(line)
    visible = "".join(masked)
    # Protect destinations from code-span parsing; backticks are literal there.
    chars = list(visible)
    destinations = {}
    index = 0
    while index < len(visible):
        if index in destinations:
            index = destinations[index]
            continue
        if visible[index] == "\\":
            index += 2
            continue
        if visible[index] == "[":
            try:
                end = bracket_end(visible, index)
                if visible[end + 1 : end + 2] == "(":
                    _, stop = destination(visible, end + 2, True)
                    destinations[end + 1] = stop
                elif (
                    visible[end + 1 : end + 2] == ":"
                    and not visible[visible.rfind("\n", 0, index) + 1 : index].strip()
                ):
                    stop = visible.find("\n", end)
                    destinations[end + 1] = len(visible) if stop < 0 else stop
            except TabulaError:
                pass  # The link reader below supplies the line diagnostic.
        if visible[index] == "`":
            stop, paired = code_span(visible, index)
            if paired:
                chars[index:stop] = blank(visible[index:stop])
            index = stop
        else:
            index += 1
    visible = "".join(chars)
    html_visible = list(visible)
    for start, stop in destinations.items():
        html_visible[start:stop] = blank(visible[start:stop])
    for number, line in enumerate("".join(html_visible).splitlines(), 1):
        if re.search(r"<\s*(?:/?[A-Za-z][A-Za-z0-9-]*(?:\s|/?>)|!--)", line):
            raise TabulaError(
                "HTML evidence is unsupported; use Markdown links or fenced examples",
                line=number,
            )
    headings = [
        item for item in headings if visible[item[0] :].split("\n", 1)[0].strip()
    ]
    definitions = {}
    chars = list(visible)
    offset = 0
    for number, line in enumerate(visible.splitlines(keepends=True), 1):
        match = re.match(r"^ {0,3}\[", line)
        if match:
            start = match.end() - 1
            try:
                end = bracket_end(visible, offset + start) - offset
            except TabulaError as error:
                error.line = number
                raise
            if end < len(line) and line[end + 1 : end + 2] == ":":
                try:
                    target, _ = destination(line[end + 2 :].strip(), 0, False)
                    if not target:
                        raise TabulaError(
                            "reference definition needs a destination on the same line"
                        )
                    key = label_key(lines[number - 1][start + 1 : end])
                    if key in definitions:
                        raise TabulaError(
                            "duplicate reference definition; use a unique label"
                        )
                    definitions[key] = target
                except TabulaError as error:
                    error.line = number
                    raise
                chars[offset : offset + len(line)] = blank(line)
        offset += len(line)
    visible = "".join(chars)
    links = []
    index = 0
    while index < len(visible):
        char = visible[index]
        if char == "\\":
            index += 2
            continue
        line = visible.count("\n", 0, index) + 1
        try:
            if char == "!" and visible[index + 1 : index + 2] == "[":
                raise TabulaError(
                    "image evidence is unsupported; use an ordinary source link"
                )
            if char == "<":
                auto = re.match(
                    r"<([A-Za-z][A-Za-z0-9+.-]*:[^<>\s]*|[^<>\s]+@[^<>\s]+)>",
                    visible[index:],
                )
                if auto:
                    target = auto[1]
                    links.append(
                        (index, line, target if ":" in target else "mailto:" + target)
                    )
                    index += auto.end()
                    continue
            if char != "[":
                index += 1
                continue
            end = bracket_end(visible, index)
            label = text[index + 1 : end]
            after = end + 1
            if visible[after : after + 1] == "(":
                target, after = destination(visible, after + 1, True)
            else:
                if visible[after : after + 1] == "[":
                    ref_end = bracket_end(visible, after)
                    key = label_key(text[after + 1 : ref_end] or label)
                    after = ref_end + 1
                else:
                    key = label_key(label)
                if key not in definitions:
                    raise TabulaError(
                        "unresolved reference; define [label]: path or escape literal brackets"
                    )
                target = definitions[key]
            links.append((index, line, target))
            index = after
        except TabulaError as error:
            error.line = line
            raise
    if not headings or headings[0][0] != 0 or headings[0][2] != 1:
        raise TabulaError("card must begin with a repository heading", line=1)
    if len(headings) > SECTION_COUNT:
        raise TabulaError("too many sections", "oversized")
    sections = {}
    occurrences = {}
    ancestors = []
    for pos, (start, line, level, title) in enumerate(headings):
        end = headings[pos + 1][0] if pos + 1 < len(headings) else len(text)
        key = f"{level}-" + (
            re.sub(r"[^\w-]+", "-", title.casefold()).strip("-") or "section"
        )
        occurrences[key] = occurrences.get(key, 0) + 1
        identity = f"{key}-{occurrences[key]}"
        ancestors = [(depth, heading) for depth, heading in ancestors if depth < level]
        sections[identity] = {
            "line": line,
            "heading": title,
            "text": text[start:end],
            "ancestors": [heading for _, heading in ancestors],
            "links": [
                (number, target)
                for offset, number, target in links
                if start <= offset < end
            ],
        }
        ancestors.append((level, title))
    return sections


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


if __name__ == "__main__":
    raise SystemExit(main())
