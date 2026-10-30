# Verification evidence

## Compatibility reference

The baseline is the reviewed Python working tree, not the earlier Git commit.
It was frozen before implementation under Python 3.15.0 / Unicode 17.0.0, with
LC_ALL=C.UTF-8, COLUMNS=80, PYTHONHASHSEED=0, NO_COLOR=1 and TERM=dumb.
Source hashes are recorded in tests/reference/manifest.json. The complete old
source snapshot is outside the publication tree.

All 47 original core tests passed. Their operations and CLI invocations produced
257 frozen replay cases, grouped into the corresponding 47 native Rust tests.
The replay compares complete JSON and text outputs, stderr, exit codes, file
inventories and expected file bytes. The owner explicitly corrected product and
CLI naming to Tabula and tabula; that naming delta is the only approved output
adjustment. Context budgets, receipt formats and hashes retain the reference
behavior. Expectations were not sorted or whitespace-normalized.

Another 32 frozen CLI cases cover help, errors, abbreviated options and integer
arguments. Native tests cover atomic-write refusal, independent binary execution,
source-read deduplication, non-finite JSON, deep nesting, UTF-16/32 JSON and unsafe
Unicode paths. A complete scalar-value digest checks case-folding, word and
printable characters, whitespace and decimal digits against Unicode 17.0.0.

The old engine is not shipped or used by regular tests. POSIX FIFO/symlink replay
cases are scoped to POSIX; Windows has native reparse-point, device-name,
alternate-stream and locked-file tests. Windows tests establish an additional
platform contract, not Python/Windows parity. Finite behavioral fixtures do not
prove universal equivalence for every possible input.

## Local checks

The following checks passed on Linux x86-64 during the migration:

- Cargo formatting, Clippy with warnings denied, and all Rust tests.
- Offline Cargo package verification and explicit crate-member allowlist.
- Nix regular and static Linux-musl package builds from an isolated source export using the
  single public nixpkgs pin; native execution outside the original checkout.
- npm tarball execution through Node and Bun, direct binary execution with an
  empty PATH, freshness after a source change, and missing/version-mismatched
  platform-package refusal.
- Shell installer success, wrong-checksum refusal, existing-installation refusal,
  archive-link refusal, symlink-prefix refusal and cleanup after a failed copy.
- ShellCheck, JavaScript syntax checks, Nix formatting and workflow lint.

No model run or performance benchmark was performed. Tool availability is not
proof that every platform or integration works.

## Security and publication review

These are separate evidence scopes:

- **Current source:** the private-reference checklist lives outside the repository.
  Its source scan had no matches. The current-source credential/privacy scan
  reported no findings. Dependency licenses are recorded in THIRD_PARTY_LICENSES.
- **Dependencies:** the first OSV scan reported GHSA-qwgh-2vcv-g2f7. The SHA-256
  dependency was updated to sha2 0.11.1 and block-buffer 0.12.1. A direct OSV 2.7.0
  scan of the final Cargo.lock examined 42 packages and reported no vulnerabilities.
  The wrapper's follow-up report incorrectly described its empty result as no
  supported inputs despite reporting 42 scanned packages; the direct lockfile
  scan supplies the final evidence.
- **Source patterns:** the configured Semgrep scan was incomplete because it
  could not fully parse the shell installer. It reported no findings in parsed
  adapters and has no configured Rust rules. ShellCheck and installer tests
  passed separately; they do not substitute for a complete security review.
- **History:** the pre-repair publication scan covered all ten then-reachable
  commits (about 2.08 MB) and found no credentials. Historical infrastructure
  references and GitHub no-reply author/committer identities remain. The owner
  approved publishing the existing history; no history rewrite was performed.
- **Artifacts:** runtime archives and npm packages use explicit file allowlists;
  crate packaging is also checked against its own allowlist. Local receipts,
  Python files, caches and development environments are excluded. Artifact
  privacy checks are independent of the source and history checks. The final
  Linux archive, two npm tarballs and Cargo crate passed the external reference
  checklist (54 files, including native binaries). The release-directory secret
  scan reported no findings in the text it scanned; this is not a binary audit.

Automatic CI is intentionally limited to Ubuntu 24.04 x86-64 and ARM64 to avoid
macOS/Windows runner costs. Bun 1.4.2 runs JavaScript adapters and tests; Node.js 26
is retained for npm launcher compatibility. Actions are pinned to commit hashes,
checkout credentials are not persisted, and tool caches are disabled.
The publication review found macOS replay-fixture compilation failures and
Windows CLI byte mismatches on commit `7532d2f`. FIFO fixtures now use POSIX
`mkfifo`, with an explicit file-type and mode test. Embedded help has an LF
checkout rule. A local `core.autocrlf=true` checkout reproduced the CLI failure
before that rule and passed the unchanged frozen CLI test after it.
The next hosted run passed Rust checks on every target and exposed two later
adapter failures: CRLF `tar` listings on Windows and a symlinked temporary
directory on macOS. Crate-member parsing accepts LF and CRLF while retaining
the same allowlist; native adapter tests cover both valid and rejected members.
The package smoke canonicalizes only its own temporary directory, not installer
inputs, and retains explicit symlink-prefix and symlink-ancestor refusal tests.

The earlier five-target run on `da3ef2a` passed both Linux and both macOS jobs.
Windows passed Rust and crate checks, but its package smoke could not locate
the Bun executable. Its installer path therefore remains unverified. macOS and
Windows builds now require explicit manual verification rather than automatic CI.

Hosted Ubuntu results must be checked for the exact commit in
[GitHub Actions](https://github.com/timfewi/tabula-repo-nix/actions/workflows/check.yml).
Registry ownership and publication remain unverified. The owner-approved source
destination is `timfewi/tabula-repo-nix`; source visibility is separate from
package publication. No package release or host activation is part of this
verification. Rust, successful tests and static scans alone are not a safety proof.
