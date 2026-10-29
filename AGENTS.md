# Repository guide

Tabula creates and checks portable repository guides. Start with README.md and
.tabula/ENTRY.md. Keep source and documentation in English.

Preserve human explanations in ENTRY.md and derived freshness evidence in
state.json. Commands never execute project code, fetch remote links, discover
unregistered files recursively or read credentials. Preserve existing guides and
unrelated work. Follow the bounded Markdown dialect in docs/reference.md.

Use native Rust with `#![forbid(unsafe_code)]`. Keep dependencies small, public,
licensed and locked. JavaScript, shell and PowerShell are distribution adapters;
they must not implement a second guide engine. Python is not a build, runtime,
packaging or regular-test dependency.

Run `cargo fmt --all -- --check`, `cargo clippy --locked --all-targets -- -D warnings`
and `cargo test --locked` for source changes; `just check` is equivalent. Packaging
changes also require a Cargo package verification and an affected local package
smoke test. Keep temporary outputs outside the checkout. Never regenerate frozen
compatibility expectations to hide a behavior change.

Keep factual verification status in docs/verification.md. Publication, registry
names, history changes and installation on an operator's system are separate
from local development and artifact verification.
