#!/usr/bin/env bash
# Builds the release binary of netsmith for THIS machine's target and packs it:
#   dist-release/netsmith-<version>-<target>.tar.gz   (.zip on Windows)
#   dist-release/netsmith-<version>-<target>.<ext>.sha256
# The archive holds the binary, LICENSE, README.md and SHA256SUMS (the checksums of those three).
#
# Steps, each of which stops the script with its reason when it fails:
#   1. the workspace version and the target;
#   2. `cargo build --release --locked -p netsmith-cli`, which must print no warning;
#   3. `netsmith --version` must print the workspace version;
#   4. a smoke test in a scratch directory: `netsmith init`, `build` and `fab` (which must end
#      with `design rules check: passed`) on the starter board, and `netsmith editor` must serve
#      the page and its script;
#   5. strip (Linux and macOS), pack, checksum.
#
# `build` and `fab` read KiCad's symbol and footprint libraries (NETSMITH_KICAD_SYMBOLS and
# NETSMITH_KICAD_FOOTPRINTS, else the binary looks beside kicad-cli and in the usual install
# places, ADR 0021), so step 4 needs a KiCad install on the machine. Without one the script stops; NETSMITH_RELEASE_SKIP_KICAD_SMOKE=1 runs the rest and
# says that build and fab were not tried.
#
# Needs Node 20 or newer to build the editor's page (and the npm registry, once): a release never
# ships the placeholder page that a build without Node embeds (ADR 0114).
#
# RELEASE_TAG, when set (the workflow sets it to the tag), must be v<version>.
# It builds for the host only (no cross compiling). Needs bash, cargo, curl, tar (zip, 7z or
# powershell on Windows) and sha256sum or shasum. Use the cargo target directory of the
# environment (CARGO_TARGET_DIR), as everywhere in this repository.
set -euo pipefail
cd "$(dirname "$0")/.."
root="$PWD"

fail() { echo "release-build: $*" >&2; exit 1; }
step() { echo "release-build: $*" >&2; }

# 1. The version, the target, the files that go in the archive.
# One sed command per -e: BSD sed (macOS) takes no `}` right after a flag, GNU sed does.
version="$(sed -n -e '/^\[workspace.package\]/,/^\[/{' -e 's/^version *= *"\(.*\)"/\1/p' -e '}' Cargo.toml | head -n1)"
[ -n "$version" ] || fail "no version under [workspace.package] in Cargo.toml"
if [ -n "${RELEASE_TAG:-}" ] && [ "$RELEASE_TAG" != "v$version" ]; then
    fail "the tag is $RELEASE_TAG but the workspace version is $version (expected v$version)"
fi
target="$(rustc -vV | sed -n 's/^host: //p')"
[ -n "$target" ] || fail "rustc -vV reports no host target"
for f in LICENSE README.md; do [ -f "$f" ] || fail "$f is missing"; done
case "$target" in *windows*) exe=".exe"; ext="zip" ;; *) exe=""; ext="tar.gz" ;; esac
name="netsmith-$version-$target"
if [ -n "$(git status --porcelain 2>/dev/null || true)" ]; then
    step "warning: the working tree has uncommitted changes; a release is built from a clean checkout of a tag"
fi

sha256() {
    if command -v sha256sum >/dev/null 2>&1; then sha256sum "$@"
    elif command -v shasum >/dev/null 2>&1; then shasum -a 256 "$@"
    else fail "neither sha256sum nor shasum is installed"; fi
}

# 2. Build. A build warning is a failure: the one that matters says the editor's page was not
#    built (no Node 20 or newer, ADR 0114) and the binary has the placeholder page. A release
#    always ships the real page, so it needs Node.
step "building $name"
log="$(mktemp)"
trap 'rm -f "$log"' EXIT
if ! cargo build --release --locked -p netsmith-cli --color never 2>&1 | tee "$log" >&2; then
    fail "cargo build failed"
fi
if grep -q "editor's page was not built" "$log"; then
    fail "the editor's page was not built, so the binary has only the placeholder page: a release needs Node 20 or newer (reason above)"
fi
if grep -q "^warning" "$log"; then
    fail "the build printed warnings (above); a release is built with none"
fi
target_dir="${CARGO_TARGET_DIR:-$root/target}"
bin="$target_dir/release/netsmith$exe"
[ -x "$bin" ] || fail "no binary at $bin"

# 3. The version.
printed="$("$bin" --version)"
[ "$printed" = "netsmith $version" ] || fail "\`netsmith --version\` printed \"$printed\", expected \"netsmith $version\""
step "$printed"

# 4. Smoke test, in a scratch directory.
scratch="$(mktemp -d)"
editor_pid=""
cleanup() {
    [ -n "$editor_pid" ] && kill "$editor_pid" 2>/dev/null || true
    rm -rf "$scratch" "$log"
}
trap cleanup EXIT
mkdir "$scratch/blink"
cd "$scratch/blink"
"$bin" init >/dev/null || fail "netsmith init failed"
[ -f netsmith.toml ] && [ -f src/main.nsm ] || fail "netsmith init wrote no netsmith.toml and src/main.nsm"

# The binary finds the libraries itself; a build that stops with "no KiCad ... libraries found"
# means there is no install here.
have_kicad=1
"$bin" build >"$scratch/build.log" 2>&1 || {
    if grep -q "no KiCad .* libraries found" "$scratch/build.log"; then
        have_kicad=0
    else
        cat "$scratch/build.log" >&2; fail "netsmith build failed on the starter board"
    fi
}
if [ "$have_kicad" = 1 ]; then
    [ -d out/Blink ] || fail "netsmith build wrote no out/Blink"
    "$bin" fab >"$scratch/fab.log" 2>&1 || { cat "$scratch/fab.log" >&2; fail "netsmith fab failed on the starter board"; }
    grep -q "design rules check: passed" "$scratch/fab.log" || { cat "$scratch/fab.log" >&2; fail "netsmith fab did not end with \"design rules check: passed\""; }
    step "smoke: init, build and fab on the starter board passed"
elif [ "${NETSMITH_RELEASE_SKIP_KICAD_SMOKE:-}" = 1 ]; then
    step "WARNING: build and fab were NOT tried: no KiCad libraries found"
else
    fail "build and fab need KiCad's libraries: set NETSMITH_KICAD_SYMBOLS and NETSMITH_KICAD_FOOTPRINTS (or NETSMITH_RELEASE_SKIP_KICAD_SMOKE=1 to skip build and fab)"
fi

"$bin" editor --port 0 >"$scratch/editor.log" 2>&1 &
editor_pid=$!
url=""
for _ in $(seq 1 100); do
    url="$(sed -n 's/^editing .* at \(http[^ ]*\) .*/\1/p' "$scratch/editor.log" | head -n1)"
    [ -n "$url" ] && break
    kill -0 "$editor_pid" 2>/dev/null || { cat "$scratch/editor.log" >&2; fail "netsmith editor stopped"; }
    sleep 0.1
done
[ -n "$url" ] || { cat "$scratch/editor.log" >&2; fail "netsmith editor printed no address"; }
page="$(curl -fsS "$url")" || fail "the editor did not serve $url"
echo "$page" | grep -q "<title>netsmith</title>" || fail "the editor's page at $url has no <title>netsmith</title>"
script_path="$(echo "$page" | sed -n 's/.*<script[^>]* src="\([^"]*\)".*/\1/p' | head -n1)"
[ -n "$script_path" ] || fail "the editor's page names no script"
size="$(curl -fsS "${url%/}$script_path" | wc -c)"
[ "$size" -gt 100000 ] || fail "the editor's script $script_path is $size bytes; the page is not in the binary"
step "smoke: the editor serves its page and a $size byte script"
kill "$editor_pid" 2>/dev/null || true
editor_pid=""
cd "$root"

# 5. Strip, pack, checksum.
out="$root/dist-release"
stage="$scratch/$name"
mkdir -p "$out" "$stage"
cp "$bin" "$stage/netsmith$exe"
case "$target" in
    *linux* | *apple*) command -v strip >/dev/null 2>&1 && strip "$stage/netsmith$exe" || step "no strip; the binary keeps its symbols" ;;
esac
cp LICENSE README.md "$stage/"
(cd "$stage" && sha256 "netsmith$exe" LICENSE README.md >SHA256SUMS)
archive="$out/$name.$ext"
rm -f "$archive" "$archive.sha256"
if [ "$ext" = "zip" ]; then
    if command -v zip >/dev/null 2>&1; then (cd "$scratch" && zip -qr "$archive" "$name")
    elif command -v 7z >/dev/null 2>&1; then (cd "$scratch" && 7z a -tzip -bso0 "$archive" "$name")
    else
        win_archive="$(cygpath -w "$archive" 2>/dev/null || echo "$archive")"
        win_stage="$(cygpath -w "$stage" 2>/dev/null || echo "$stage")"
        powershell -NoProfile -Command "Compress-Archive -Path '$win_stage' -DestinationPath '$win_archive'"
    fi
else
    tar -C "$scratch" -czf "$archive" "$name"
fi
[ -f "$archive" ] || fail "no archive was written at $archive"
(cd "$out" && sha256 "$name.$ext" >"$name.$ext.sha256")
step "wrote $archive ($(wc -c <"$archive") bytes) and $archive.sha256"
echo "$archive"
