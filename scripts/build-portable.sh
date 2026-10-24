#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
output=${1:-$root/dist}
version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$root/Cargo.toml" | head -n 1)
case "$version" in
    *[!0-9.a-zA-Z+-]*|"") echo "invalid Paper version: $version" >&2; exit 1 ;;
esac
case "$(uname -m)" in
    x86_64) architecture=x86_64 ;;
    aarch64|arm64) architecture=aarch64 ;;
    *) echo "unsupported architecture: $(uname -m)" >&2; exit 1 ;;
esac
case "$output" in
    /*) ;;
    *) output="$root/$output" ;;
esac

temporary=$(mktemp -d)
cleanup() {
    [ -n "${temporary:-}" ] || return 0
    case "$temporary" in
        "${TMPDIR:-/tmp}"/tmp.*) ;;
        *) echo "refusing to remove unexpected temporary path: $temporary" >&2; return 0 ;;
    esac
    [ ! -L "$temporary" ] || return 0
    [ ! -e "$temporary" ] || rm -rf -- "$temporary"
}
trap cleanup EXIT HUP INT TERM

"$root/scripts/package-stage.sh" "$temporary/stage"

# Binaries linked with FFMPEG_DIR set look for FFmpeg in $ORIGIN/../lib/skwd-paper,
# so a private runtime travels in that directory with its LGPL license and source.
ffmpeg_prefix=${SKWD_FFMPEG_PREFIX:-}
if [ -n "$ffmpeg_prefix" ]; then
    if [ ! -d "$ffmpeg_prefix/lib" ] || [ ! -d "$ffmpeg_prefix/share/licenses/ffmpeg" ]; then
        echo "not a private FFmpeg runtime: $ffmpeg_prefix" >&2
        exit 1
    fi
    cp -a "$ffmpeg_prefix"/lib/*.so.* "$temporary/stage/usr/lib/skwd-paper/"
    mkdir -p "$temporary/stage/usr/share/licenses/skwd-paper/ffmpeg"
    cp -a "$ffmpeg_prefix/share/licenses/ffmpeg/." \
        "$temporary/stage/usr/share/licenses/skwd-paper/ffmpeg/"
fi

mkdir -p "$output"
archive="skwd-paper_${version}_linux-${architecture}.tar.xz"
tar -C "$temporary/stage" -cJf "$output/$archive" usr
(cd "$output" && sha256sum "$archive" > SHA256SUMS)
echo "built $output/$archive"
