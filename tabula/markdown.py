"""Parse the bounded Markdown evidence dialect into sections and links."""

import html
import re

from .limits import SECTION_COUNT
from .storage import TabulaError


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
