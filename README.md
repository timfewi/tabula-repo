# tabula-repo-nix

A small, offline tool for portable repository context. `tabula` creates a short
`.tabula/ENTRY.md`, records the source files it cites and reports stale evidence.
The card works in any editor or agent without NixOS, a service or a database.

```sh
python3 tabula.py init /path/to/repo --purpose 'Explain what this repository owns.'
# Edit .tabula/ENTRY.md to explain components, data flow and task entrypoints.
python3 tabula.py update /path/to/repo
python3 tabula.py check /path/to/repo --json
```

`init` never replaces an existing card. `update` refreshes only the receipt in
`.tabula/state.json`; it never rewrites explanations. Review changed source and
correct the card before updating. `check` compares the current card and cited
source with that receipt. Hash equality proves byte freshness, not architectural
correctness. No command executes project code, evaluates Nix, starts models,
follows remote links or searches outside the selected repository.

Use inline Markdown links relative to ENTRY.md (encode spaces as `%20`) for source evidence, such as
`[parser](../src/parser.py)`. Cite files, not directories; a link to a directory
is navigation only. External URLs and same-document anchors are navigation,
never fetched. The card should explain purpose, ownership, flow, interfaces,
checks and important limits in its own words, with details linked on demand.
Do not duplicate a full README or source inventory. No special instruction
precedence is attached to the card.

The entrypoint is capped at 10,000 UTF-8 bytes. This is a size budget, not a
model-token measurement. Source reads are bounded to 2 MiB per file and 64
cited files. Symlinks, special files, paths outside the repo and credential or
runtime paths are rejected. These checks protect ordinary local use; this
operator-owned process is not an isolation boundary against concurrent hostile
filesystem mutation. Never put credentials in a card or its cited source.

Nix exposes `packages.<system>.default` with the `tabula` command on x86_64-linux
and aarch64-linux. Run from source with Python 3.11 or newer, standard library
only. Host installation and activation remain with the consuming host.

Development: `project-check fast` runs formatting, lint and behavioral tests.
The tests use temporary repositories and include stale, missing, oversized and
unsafe inputs. No network or model access is needed.

## Development checkpoint — 2026-10-03

Implemented the three offline commands and nine behavioral tests. The fast gate
passes Python/Nix lint, formatting and tests. Both Linux package outputs were
evaluated; the x86_64 package was built and its CLI exercised. ARM execution and
host installation remain unverified. Three workspace pilot cards have freshness
receipts; their architecture was reviewed against source. Model-quality benefits
remain an experiment in coding-agents-nix, not a claim made by this tool.
