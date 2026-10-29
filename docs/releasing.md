# Releasing tabula-repo

The repository and package name is `tabula-repo`. The product and CLI are Tabula
and `tabula`, in homage to *tabula rasa*. Registry ownership and the public
repository URL remain publication prerequisites.

1. Confirm ownership and availability of `tabula-repo` and its exact-version npm
   platform-package names. Configure a private vulnerability-reporting channel.
2. Review source, Git history and artifacts separately. Source checks do not
   certify history. Review author and committer metadata without rewriting it
   automatically. Keep any private reference checklist and scan reports outside
   the publication tree.
3. Build from a clean checkout without private forge credentials. Verify the
   flake lock has only the public pinned nixpkgs input. Run Cargo formatting,
   Clippy, tests and package verification directly; no external check runner is
   required.
4. Run the native platform matrix. Exercise the packaged CLI outside its checkout
   with no Python or private tools on PATH. Test the matching npm package with
   Node and Bun. Verify installer refusal paths and Windows file-lock and
   reparse-point behavior. Record unavailable runners as unverified.
5. Build Linux musl, native macOS and Windows MSVC binaries. Package with the
   reviewed scripts in docs/packaging.md. Inspect archive members and crate/npm
   file lists. No Python source, local receipts, caches or private development
   state belongs in a runtime artifact.
6. Keep Cargo.toml, package.json, the CLI, help and platform packages at the same
   explicit version. Each npm optional dependency must use that exact version.
7. Review SHA-256 values and archive provenance. Publish checksums through a
   trusted channel. Installers take a fixed version and local archive; never
   silently select a moving latest release.
8. Publish only to the approved destination after artifact and metadata review.
   Ordinary CI does not release packages or change repository visibility.

See docs/verification.md for evidence actually obtained. Prepared CI jobs are
not proof of successful hosted runs. Rust and static scans alone are not a
security audit or proof of byte parity on every possible input.
