# tabula-repo-nix

A small, offline Python tool for portable repository context. `tabula` keeps human
explanations in `.tabula/ENTRY.md` and derived evidence in `.tabula/state.json`.
Cards work in any editor or agent without NixOS, a service or a database.

```sh
python3 tabula.py init /path/to/repo --purpose 'Explain what this repository owns.'
# Edit ENTRY.md to explain components, flow, interfaces and limits.
python3 tabula.py doctor /path/to/repo --json
python3 tabula.py update /path/to/repo
python3 tabula.py check /path/to/repo --json
# After reviewing an explanation against source, copy its ID from check:
python3 tabula.py update /path/to/repo --review entry/2-source-entrypoints-1
```

`init` never replaces an existing card. `update` atomically replaces only the
receipt and preserves explanations. A plain update records byte freshness, not
review or test success. `--review SECTION...` explicitly acknowledges those
sections against their exact content, ancestor headings, resolved links and
source hashes. The receipt also records the card hash at acknowledgement time.
An acknowledgement is the caller's assertion, not proof of architectural truth.
Unchanged sections retain acknowledgements across updates, including edits to
unrelated prose. Changed evidence invalidates an acknowledgement; updating alone
leaves that section unreviewed. Byte equality does not prove correctness.

`check` keeps the top-level `changed` file list and reports `sections` and
`affected_sections`. Section identity is `CARD-ID/LEVEL-heading-slug-OCCURRENCE`:
heading text is case-folded, runs of punctuation become hyphens, and the count
is per level/slug within the card. Duplicate headings have distinct counts.
Renames remove the old identity and introduce an unreviewed one. Inserting or
reordering duplicate headings can change their identities. Ancestor heading
changes invalidate descendant acknowledgements. Line numbers are diagnostic
locations, not identity or freshness inputs. Removed links affect their previous
sections, even when another section still cites the same source.

Receipts are version 3. Version-1 single-card receipts and version-2 section
receipts remain readable by `check`, `doctor` and `context`; v1 has no review
state. `update --upgrade` explicitly migrates an older receipt without changing
prose or inventing acknowledgements. Compatible v2 acknowledgements survive.
Unknown receipt versions and malformed receipts are rejected; failed updates retain the previous
receipt. A legacy receipt has no historical section mapping, so a changed legacy
card conservatively marks all its current sections affected.

`doctor` is read-only and works even with a broken receipt. It reports actual
card bytes, limits, remaining/exceeded bytes, evidence count, navigation-only
links and diagnostics with card paths and line numbers. It uses file metadata
for oversized cards without reading their contents. Diagnostics distinguish
missing, oversized, unsafe, malformed and unreadable inputs without echoing
source contents or parser exceptions. `check` and `doctor` use exit 2 for invalid
input; `check` uses 1 for stale bytes and 0 for current bytes. Unreviewed sections
do not change the byte-freshness exit code.

## Markdown evidence contract

Use Markdown links relative to the containing card, for example
`[parser](../src/parser.py#L10)`. Supported evidence syntax includes inline links,
full/collapsed/shortcut references, angle destinations, percent-encoded spaces,
fragments, escaped punctuation, balanced destination parentheses and optional
single-quoted, double-quoted or parenthesized titles. Reference labels are
case-insensitive with whitespace normalized; definitions use one physical line.
HTML character entities in destinations are decoded. ATX and single-line setext
headings identify sections. Ordinary flat, single-line list items work.

Fenced code, indented code and paired inline code spans are examples, not
evidence. External URLs, autolinks, same-document anchors and directory links
are navigation only; they are never fetched. Cite files for freshness evidence.
Every card needs source-file evidence. Undefined references, duplicate reference
definitions, nested links/lists, indented paragraph continuations, images, HTML
and blockquote/container blocks produce actionable diagnostics; move such
examples into code fences or use ordinary Markdown links.
Escape literal brackets in prose. This deliberately bounded dialect is not a
full CommonMark implementation. It has no third-party parser dependency.

Cards should explain purpose, ownership, data flow, interfaces, checks and
important limits in their own words, linking details on demand. They are durable
architecture explanations, not a task journal or a full source inventory. No
special instruction precedence is attached to a card.

## Registered topic cards

Keep `.tabula/ENTRY.md` as the default orientation and register optional detail
cards in `.tabula/cards.json`. This is the only registration inventory:

```json
{"version":1,"cards":[
  {"id":"research","path":"research.md"},
  {"id":"desktop","path":"topics/desktop.md"}
]}
```

IDs use lowercase letters, digits and hyphens (start with a letter, at most 32
characters); `entry` is reserved. Paths are normalized `.md` paths relative to
`.tabula`. Link to registered cards using ordinary relative Markdown links.
Links to cards are navigation, not source evidence. Each card needs at least one
source-file link. Unregistered files are never discovered or read. Unregistered
card links, missing cards, duplicate IDs/paths, cycles and excessive depth are
errors. All registered cards are checked, including those not linked from ENTRY.
Keep topic links directed; a topic back-link to ENTRY creates a cycle if ENTRY
links to that topic. Directory links remain navigation, not registration.

Each card has its own sections, source dependencies and acknowledgements. The
receipt records registry freshness, but never becomes a second registration
inventory: traversal always uses cards.json. Removing a card reports its previous
sections as removed. Old single-card repositories need no registry. `doctor` and
`check` report all cards' budgets, evidence coverage and affected sections.

## Task-focused context

```sh
python3 tabula.py context /path/to/repo --files src/research.py --max-bytes 6000 --json
```

Paths are repository-relative exact source dependencies, not globs or substring
queries. Selection is deterministic: matching sections come first, then the
first section of ENTRY and of matching topic cards supplies orientation, then
related topic links. Card order is ENTRY first, then topic ID; section order is
source order. Multiple matching paths do not duplicate a section. Unrelated topic
explanations are excluded. Unknown paths, no matches, omitted sections and
topic links omitted by the byte budget are explicit. Oversized candidates are skipped whole so
later, smaller candidates can still fit. No prose is generated or rewritten.

The complete stdout, including its final newline, headings, provenance and JSON
structure, fits `--max-bytes` **UTF-8 bytes**. It never cuts a section midway or
emits broken JSON. If minimal metadata cannot fit, it exits 2 with empty stdout
and a short stderr diagnostic. This also holds for invalid input and tiny or
nonpositive budgets. JSON includes original section text, resolved link targets,
card location/hash, section fingerprint, exact source hashes, freshness and review
state. Resolved targets remain relative to the original card; they preserve the
evidence when a reference definition lives in an omitted section. Human output
contains the same provenance. A stale section is labelled stale; an invalidated
review is never presented as acknowledged. Context exits 0 for usable results,
including explicitly labelled stale content or no matches, and 2 for invalid
inputs. Use `check` for the independent byte-freshness exit contract.

## Explicit test evidence

Test evidence is a third state, independent of byte freshness and review. Tabula
never runs a check or infers success from logs, installed packages or timestamps.
A caller can request a bounded template for an exact cited-source snapshot:

```sh
python3 tabula.py test-snapshot /path/to/repo --files src/research.py --json
# The caller runs its check separately against those exact bytes, then supplies:
python3 tabula.py import-test /path/to/repo --json < /path/to/runtime/check-result.json
```

The stdin import contract has exactly these fields:

```json
{"version":1,"check_id":"research-unit","outcome":"pass","provenance":"caller",
 "sources":{"src/research.py":"64 lowercase hexadecimal SHA-256 characters"}}
```

Replace the illustrative hash with the template's actual hash. The template
starts with outcome `unknown` and provenance `untrusted`. A caller must supply the
check identity (1–80 letters/digits or `._:/-`, beginning with a letter/digit),
outcome (`pass`, `fail`, `unknown`), provenance (`caller`, `untrusted`) and a
nonempty map of exact tested source hashes. The caller is responsible for listing
**all** check inputs and ensuring the check actually ran against those bytes.
Only cited source paths are accepted at import. Metadata remains a caller
assertion, never cryptographic proof of execution; `caller` is not authenticated.
Raw logs, timestamps, arbitrary extra fields and malformed provenance are rejected.
Keep input metadata and raw run data outside source and the Nix store.

Results are stored in state.json, keyed by check identity; importing that identity
again replaces its previous result. Importing never refreshes the architecture
snapshot or acknowledges review. `check`, `doctor` and relevant context expose
outcome, provenance and applicability separately. Changed, missing or no-longer-
cited inputs make applicability stale. Plain update preserves the record and
cannot make it applicable again to different bytes. Failed and untrusted results
remain visible; absent relevant evidence reports `missing`. Byte-freshness exit
codes do not turn into a test runner's exit codes.

## Boundaries and development

All work is bounded; these are byte/file limits, not model-token estimates:

| Resource | Limit |
| --- | ---: |
| ENTRY or one topic card | 10,000 UTF-8 bytes |
| All cards together | 80,000 bytes |
| Cards, including ENTRY | 16 |
| Card path depth below .tabula / linked-card chain | 4 components / 4 cards |
| Sections per card | 256 |
| Unique source files across all cards | 64 |
| One source read | 2 MiB |
| cards.json / one stdin test import | 8,000 / 16,000 bytes |
| Retained test identities / state.json | 16 / 512,000 bytes |
| Context request | 64 paths, 8,000 UTF-8 bytes of path names |

Shared source dependencies are read once per inspection. Registry graph edges
are deduplicated and depths memoized. Limits are independent: a receipt can reach
its own budget before every card reaches its individual maximum. Failed writes
leave the previous receipt intact. Symlinks, special files, repository escapes,
credentials, runtime paths and control characters in paths/cards are rejected.
Never put secrets in cards or cited source. These checks protect ordinary local
use; this operator-owned process is not an isolation boundary against concurrent
hostile filesystem mutation. There is no filesystem-wide atomic snapshot: the
caller must avoid concurrent edits during acknowledgement or test capture.

No command executes project code, evaluates Nix, starts models, follows remote
links or discovers files recursively. Python 3.11+ and its standard library are
sufficient. Nix exposes `packages.<system>.default` with `tabula` on x86_64-linux
and aarch64-linux. Installation and activation belong to the consuming host.

`project-check fast` runs formatting, lint and temporary-fixture behavioral tests.
Package builds and broader integration checks are separate workloads.

## Source layout

The Python package separates responsibilities without third-party dependencies:

| File | Responsibility |
| --- | --- |
| [tabula.py](tabula.py), [cli.py](tabula/cli.py) | Source launcher, arguments, command dispatch and exit codes. |
| [markdown.py](tabula/markdown.py) | Pure Markdown parsing into sections and evidence links. |
| [cards.py](tabula/cards.py) | Card registration, graph traversal, source inspection and snapshots. |
| [receipts.py](tabula/receipts.py) | Receipt versions, schema validation and persistence. |
| [evidence.py](tabula/evidence.py) | Test-input snapshots, result imports and applicability. |
| [operations.py](tabula/operations.py) | Initialization, updates, review acknowledgements, checks and diagnosis. |
| [context.py](tabula/context.py) | Task-focused selection and byte-bounded human/JSON rendering. |
| [storage.py](tabula/storage.py), [limits.py](tabula/limits.py) | Safe file I/O, strict JSON, diagnostics and shared bounds. |

Internal callers import the owning module directly; the package initializer does
not re-export a second API. Keep `tabula.py` beside the `tabula/` directory when
running from source. The Nix wrapper includes the launcher and package Python
files, excluding tests and development caches from its runtime source.

## Development checkpoint — 2026-10-25

The implementation is split into focused modules; CLI syntax, receipt versions
and safety limits are unchanged. All 47 behavioral tests pass, including CLI runs
from outside the checkout. A 22-step comparison against the original monolith
matched stdout, stderr, exit codes and receipt bytes, covering all seven commands,
review invalidation, stale test evidence, error paths and legacy migration.
The x86_64-linux Nix package built and passed the same comparison outside the
checkout. ARM execution, host activation and model-quality benefits remain
unverified.
