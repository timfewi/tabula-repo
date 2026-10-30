# Contributing to Tabula

Start with the README and copy the learning example into a temporary directory.
Cargo is the primary build and test entrypoint. No private tooling or neighboring
checkout is needed. Nix and Just are optional conveniences.

## Source map

| Module | Responsibility |
| --- | --- |
| `src/cli.rs`, `src/help/` | Arguments, help, command dispatch and exit codes. |
| `src/operations.rs` | Initialization, updates, freshness checks and diagnosis. |
| `src/markdown.rs` | The bounded Markdown evidence dialect. |
| `src/cards.rs` | Registered cards, graph traversal and source snapshots. |
| `src/receipts.rs` | Receipt versions, schema validation and persistence. |
| `src/evidence.rs` | Caller-supplied test metadata and applicability. |
| `src/context.rs` | Complete-section selection and output budgets. |
| `src/storage.rs` | Bounded local I/O, path refusals and atomic writes. |
| `src/compat.rs`, `src/unicode.rs` | Ordered JSON, text representation and frozen character semantics. |
| `bin/`, `packaging/`, installers | Native distribution adapters. |

The library and CLI share one engine. Own Rust source forbids unsafe code;
dependencies are reviewed separately and locked in Cargo.lock. Explain the need,
license and platform implications of new dependencies.

## Verification

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo package --locked --allow-dirty
```

Use `--allow-dirty` only for local review of uncommitted work; releases must use
a reviewed clean checkout. `just check` runs formatting, Clippy and tests.
Packaging verification is described in docs/packaging.md.

On Unix, replay tests use the standard POSIX `mkfifo` utility to create named-pipe
fixtures without a shell. Embedded CLI help is checked out with LF endings on
every platform so the frozen byte comparisons also apply on Windows.

Frozen fixtures came from the reviewed Python working tree, including the
interim rebrand, under Python 3.15.0 and Unicode 17.0.0. Its source hashes and
controlled environment are recorded in tests/reference/manifest.json. The old
engine is not a regular test dependency. Fixtures preserve exact expected
outputs and file bytes. The explicit naming correction to
Tabula is documented separately; do not normalize away other differences.

Add synthetic negative fixtures for errors as well as successful behavior.
Never renew expected snapshots just to make a test pass. Native Windows refusal
cases establish a Windows contract; they do not establish Python/Windows parity.
Keep credentials, private source, logs and personal paths out of fixtures.

Describe the user problem, final behavior and checks actually exercised in a
change proposal. Update documentation claims in place. Record local results and
remaining platform gaps in docs/verification.md; do not claim hosted CI or a
release ran merely because its configuration exists.
