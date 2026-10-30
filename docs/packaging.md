# Native packages

The engine is a Rust library and the `tabula` binary. Every distribution starts
that binary. No route embeds an interpreter or compiles during npm installation.
Public registry coordinates are not yet approved; use local artifacts for review.

## Cargo and Nix

```sh
cargo build --locked --release
cargo package --locked --allow-dirty
nix build .#tabula
```

Cargo package verification builds the packaged source, including its locked
public dependencies. Omit `--allow-dirty` for release verification. The optional
Nix flake uses only its pinned public nixpkgs input and Cargo.lock. Nix supports
Linux and macOS on both listed architectures. The default package contains the
same `tabula` executable.

## Release targets

| Target | Runner | Archive |
| --- | --- | --- |
| `x86_64-unknown-linux-musl` | Linux x86-64 | `.tar.gz` |
| `aarch64-unknown-linux-musl` | Linux ARM64 | `.tar.gz` |
| `x86_64-apple-darwin` | macOS x86-64 | `.tar.gz` |
| `aarch64-apple-darwin` | macOS ARM64 | `.tar.gz` |
| `x86_64-pc-windows-msvc` | Windows x86-64 | `.zip` |

Automatic CI covers only the two Ubuntu Linux targets, with Bun 1.4.2 as the
adapter/test runner and Node.js 26 for npm launcher compatibility. macOS and
Windows packaging is manual; arrange those native runners explicitly before
claiming a verified release for those targets.

Build on the matching native runner with the appropriate Rust target, then:

```sh
node packaging/build.mjs TARGET /absolute/path/to/tabula /absolute/new-output-directory
```

Use `tabula.exe` on Windows. The script checks native platform and binary version,
creates the archive and SHA-256 file, and writes a platform npm package in the
output directory. It never downloads or builds a binary. Unix archives contain
one versioned directory with six regular files: `tabula`, `VERSION`, `README.md`
`REFERENCE.md`, `LICENSE` and `THIRD_PARTY_LICENSES`. Windows uses `tabula.exe` in the equivalent zip directory.

## npm and Bun

```sh
npm pack --ignore-scripts --pack-destination /absolute/output
npm pack /absolute/output/platform --ignore-scripts --pack-destination /absolute/output
```

The main `tabula-repo` package depends optionally on exactly versioned
`tabula-repo-TARGET` packages. Install both local tarballs in an empty temporary
project for testing; use `--ignore-scripts` and avoid a global install during
verification. For fully offline tests, populate the matching package directly
from its reviewed tarball; do not let a registry supply a candidate name.

`node path/to/bin/tabula.mjs --version` and
`bun path/to/bin/tabula.mjs --version` exercise the same packaged binary.
Unsupported platforms, a missing optional package or a version mismatch fail
explicitly. There is no network or compilation fallback.

## Local archive installers

```sh
sh install.sh 0.1.0 ARCHIVE CHECKED_SHA256 /absolute/unused-prefix
/absolute/unused-prefix/bin/tabula --version
```

The prefix must be normalized and free of symlink components. The installer
validates the member allowlist, types, checksum and version before installing.
It refuses existing installations. It requires a POSIX shell, tar and either
sha256sum or shasum. All generated data belongs to the selected prefix.

On Windows, invoke `install.ps1` with `-Version`, `-Archive`, `-Sha256` and an
unused absolute `-Prefix`. It uses .NET zip streams and exclusive file creation,
rejects unexpected members and reparse-point destinations, and never needs an
elevated shell. Add the selected prefix to PATH yourself.

No hosted installer URL or curl pipeline is advertised before release ownership
and artifact provenance have been reviewed.
