# Tabula

**A short guide to your repository, with a reminder when its explanations need review.**

The repository is `tabula-repo`; the command is `tabula`.

Tabula takes its name from *tabula rasa*. Start with a blank page, explain how
your project works, and link your explanation to the source. Tabula records the
linked files' hashes. When those bytes change, it identifies the guide sections
that deserve another look.

For example, a guide explains authentication and links to `src/login.rs`. After
that file changes, `tabula check` reports the affected explanation. Tabula does
not infer what changed or rewrite your prose.

Tabula is a native Rust library and command-line program. It runs offline without
Python, an account, API key, AI model, service or Nix installation. Its seven
commands do not execute project code or fetch linked URLs.

## Try it

From this checkout, with a Rust toolchain:

```sh
cargo build --locked --release
./target/release/tabula --help
./target/release/tabula init /path/to/project --purpose "Explain how this project works."
```

For a build-only checkout, replace `tabula` below with `./target/release/tabula`.

The project needs a README or another recognized declaring file. Open the new
`.tabula/ENTRY.md`, replace the starter list with an explanation, then run:

```sh
tabula update /path/to/project
tabula check /path/to/project
```

A guide might contain:

```markdown
# Greeting app

This project formats a greeting for a supplied name.

## Main flow

The [greeting function](../src/greet.js) returns a string.
It does not read files or contact a server.
```

Links are relative to the card: `../src/greet.js` points from `.tabula` into `src`.
The [learning example](examples/hello/README.md) is ready to copy into a temporary
folder. Commit the guide; add `/.tabula/state.json` to the project's `.gitignore`
for the generated local receipt. Tabula does not change ignore rules or commit
anything for you.

**Fresh hashes do not prove that an explanation is correct or that tests passed.**
Review the prose and run tests separately.

## Commands

| Command | Purpose |
| --- | --- |
| `tabula init . --purpose "..."` | Create a starter guide without overwriting one. |
| `tabula update .` | Record current guide and source hashes. |
| `tabula check .` | Report changed files and affected explanations. |
| `tabula doctor .` | Diagnose missing files, unsupported links and limits. |
| `tabula context . --files src/greet.js --max-bytes 4000` | Select complete relevant sections within a UTF-8 byte budget. |
| `tabula test-snapshot . --files src/greet.js --json` | Describe source hashes for a separately run test. |
| `tabula import-test . --json < result.json` | Record caller-supplied test metadata. |

Add `--json` for scripts. `check` exits 0 for current bytes, 1 for stale bytes and
2 for invalid input. Review acknowledgements and test assertions remain separate
from byte freshness. See the [reference](docs/reference.md) for formats, topic
cards, Markdown syntax, limits and receipt migration.

## Build and installation routes

Version **0.1.0** is prepared locally under the MIT license. The source repository
is [timfewi/tabula-repo-nix](https://github.com/timfewi/tabula-repo-nix). Registry
names in manifests are candidates, not a claim that an official package is published.

| Route | Local use |
| --- | --- |
| Cargo | `cargo install --path . --locked` builds the native `tabula` command. |
| Nix | `nix run .#tabula -- --help`; `devenv shell` reuses the locked development shell; `nix develop` remains available. |
| npm / Bun | Pack the main package and the matching native platform package, then install both local tarballs. See [packaging](docs/packaging.md). |
| Linux / macOS archive | `sh install.sh VERSION ARCHIVE CHECKED_SHA256 /absolute/unused-prefix` |
| Windows archive | `./install.ps1 -Version VERSION -Archive ARCHIVE -Sha256 CHECKED_SHA256 -Prefix C:\path\to\unused-directory` |

Archive installers verify an explicit SHA-256 and version and refuse existing
installations. They use user-owned destinations and require no elevated rights.
They never download code. Obtain checksums from a trusted release channel.
Archives include `REFERENCE.md` with the complete CLI contract; other relative
documentation links in this README refer to the source checkout.

The npm/Bun launcher starts the exactly versioned optional platform package. It
has no download hook, interpreter embedding or compilation fallback. Node 22+
or Bun runs the launcher; Cargo and Nix run the binary directly.

Targets are Linux and macOS on x86-64 and ARM64, and Windows x86-64. Linux release
binaries use musl. Automatic CI runs only on Ubuntu x86-64 and ARM64, using
Bun 1.4.2 for JavaScript adapters and Node.js 26 for npm compatibility checks.
macOS and Windows remain manual build targets to avoid paid runner costs.
Verification results and remaining platform gaps are recorded in
[verification](docs/verification.md). A declared target is not proof of a successful build.

## Existing guides

The name and primary command are **Tabula**. `.tabula`, review evidence and test
evidence retain their existing formats. Receipt versions 1–3 are readable;
migration is explicit with `tabula update --upgrade`. Python module imports are
replaced by the Rust library. The Nix `default` and `tabula` attributes remain
as the supported package attributes.

A `.project-checks.json` in a user's project remains an ordinary declaring file
for `init`. Tabula can cite and hash it; it never invokes a runner from it.

## Development and privacy

Native `devenv.nix` reuses the locked Flake shell with Git-filtered inputs, not
the former whole-tree `path:.` snapshot. Stage authorized new source files before
Flake evaluation; ignored caches stay outside the source snapshot. Automatic
native activation requires owner approval; `devenv shell -- <command>` is explicit.

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

`just check` is an optional shortcut. The repository builds independently with
Cargo; Nix has one public, pinned `nixpkgs` input. See [contributing](CONTRIBUTING.md)
for the source map and frozen compatibility tests, and [release preparation](docs/releasing.md)
for publication prerequisites.

Keep secrets and personal information out of guides and linked files. Offline
operation does not prevent a caller from sharing a guide with another service.
Tabula is not a sandbox against concurrent hostile filesystem changes. See
[security](SECURITY.md). No performance advantage is claimed without measurement.

## License

MIT. See [LICENSE](LICENSE).
