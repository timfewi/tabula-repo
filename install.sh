#!/bin/sh
# Install a checksum-verified local native archive without elevated privileges.
set -eu
fail() {
  printf '%s\n' "tabula: $*" >&2
  exit 2
}
[ "$#" -ge 3 ] && [ "$#" -le 4 ] || fail 'Usage: sh install.sh VERSION ARCHIVE SHA256 [PREFIX]'
version=$1 archive=$2 expected=$3 prefix=${4:-"$HOME/.local"}
case "$version" in '' | *[!0-9.]* | .* | *.) fail 'invalid explicit version' ;; esac
[ "${#expected}" = 64 ] || fail 'expected a lowercase SHA-256 digest'
case "$expected" in *[!0-9a-f]*) fail 'expected a lowercase SHA-256 digest' ;; esac
[ -f "$archive" ] && [ ! -L "$archive" ] || fail 'archive must be a regular file'
case "$archive" in /*) ;; *) archive="$PWD/$archive" ;; esac
case "$(uname -s):$(uname -m)" in
Linux:x86_64) target=x86_64-unknown-linux-musl ;;
Linux:aarch64 | Linux:arm64) target=aarch64-unknown-linux-musl ;;
Darwin:x86_64) target=x86_64-apple-darwin ;;
Darwin:arm64) target=aarch64-apple-darwin ;;
*) fail 'unsupported platform' ;;
esac
if command -v sha256sum >/dev/null 2>&1; then actual=$(sha256sum "$archive"); else actual=$(shasum -a 256 "$archive"); fi
[ "${actual%% *}" = "$expected" ] || fail 'archive checksum mismatch'
case "$prefix" in /*) ;; *) fail 'prefix must be absolute' ;; esac
case "$prefix/" in *'/../'* | *'/./'* | *'//'*) fail 'prefix must be normalized' ;; esac
path=$prefix
while [ "$path" != / ]; do
  [ ! -L "$path" ] || fail 'symlink in prefix'
  path=$(dirname "$path")
done
for path in "$prefix/bin" "$prefix/lib" "$prefix/lib/tabula"; do [ ! -L "$path" ] || fail 'symlink in destination'; done
[ ! -e "$prefix/bin/tabula" ] && [ ! -L "$prefix/bin/tabula" ] || fail 'installation already exists'
[ ! -e "$prefix/lib/tabula/$version" ] && [ ! -L "$prefix/lib/tabula/$version" ] || fail 'installation already exists'
top="tabula-$version-$target"
listing=$(tar -tzf "$archive") || fail 'invalid archive'
[ "$(printf '%s\n' "$listing" | LC_ALL=C sort)" = "$(printf '%s\n' "$top/" "$top/LICENSE" "$top/README.md" "$top/REFERENCE.md" "$top/VERSION" "$top/THIRD_PARTY_LICENSES" "$top/tabula" | LC_ALL=C sort)" ] || fail 'unexpected or duplicate archive members'
# Only a directory followed by regular files is accepted; no links or devices.
[ "$(tar -tvzf "$archive" | cut -c 1 | LC_ALL=C sort | tr -d '\n')" = '------d' ] || fail 'non-regular archive member'
[ "$(tar -xOzf "$archive" "$top/VERSION")" = "$version" ] || fail 'archive version mismatch'
mkdir -p "$prefix/bin" "$prefix/lib/tabula"
target_dir="$prefix/lib/tabula/$version"
mkdir "$target_dir" || fail 'installation already exists'
created_binary=false
cleanup() {
  if [ "$created_binary" = true ]; then rm -f "$prefix/bin/tabula"; fi
  rm -rf "$target_dir"
}
trap cleanup EXIT
trap 'exit 2' HUP INT TERM
for name in tabula LICENSE README.md REFERENCE.md VERSION THIRD_PARTY_LICENSES; do
  tar -xOzf "$archive" "$top/$name" >"$target_dir/$name"
done
chmod 755 "$target_dir/tabula"
[ "$("$target_dir/tabula" --version)" = "Tabula $version" ] || fail 'binary platform or version mismatch'
# Noclobber prevents replacing an installation created by another installer.
(
  set -C
  exec 3>"$prefix/bin/tabula" || exit 2
  trap 'rm -f "$prefix/bin/tabula"' EXIT
  trap 'exit 2' HUP INT TERM
  cat "$target_dir/tabula" >&3 || exit 2
  trap - EXIT HUP INT TERM
) || fail 'destination already exists'
created_binary=true
chmod 755 "$prefix/bin/tabula"
trap - EXIT HUP INT TERM
printf '%s\n' 'Installed Tabula. Add your prefix/bin directory to PATH.'
