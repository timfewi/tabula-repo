# Tabula

Tabula creates and checks portable repository guides. Start with the
[README](../README.md), [reference](../docs/reference.md) and
[contributor guide](../CONTRIBUTING.md).

## Guide engine

The [CLI](../src/cli.rs) dispatches seven explicit commands to the Rust library.
[Operations](../src/operations.rs) keep freshness, review and caller-supplied test
assertions separate. The [card reader](../src/cards.rs) resolves registered cards
using the bounded [Markdown parser](../src/markdown.rs).
[Receipts](../src/receipts.rs) retain versions 1–3 and explicit migration.
[Context selection](../src/context.rs) preserves whole sections within byte budgets.
[Storage](../src/storage.rs) bounds reads and refuses unsafe evidence paths.

## Distribution and verification

[Cargo](../Cargo.toml) builds a native binary without an interpreter.
The [Nix flake](../flake.nix) uses one public pinned input. The
[Node/Bun launcher](../bin/tabula.mjs) starts an exact-version platform package;
[packaging](../packaging/build.mjs) takes an already-built native binary.
[Shell](../install.sh) and [PowerShell](../install.ps1) installers verify local
archives and refuse overwrites. The [replay tests](../tests/reference.rs) compare
frozen behavior. [Verification](../docs/verification.md) distinguishes local
results from unexecuted platform and publication checks.
