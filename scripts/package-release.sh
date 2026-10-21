#!/usr/bin/env bash
# Assemble ghtui release assets locally. Never builds, uploads or tags.
#
#   package-release.sh archive VERSION TARGET BINARY OUTDIR
#       -> OUTDIR/ghtui-VERSION-TARGET.tar.gz (one member: ghtui, mode 0755)
#   package-release.sh finish VERSION OUTDIR
#       -> OUTDIR/install.sh, OUTDIR/release-version.txt, OUTDIR/SHA256SUMS
#
# TARGET is one of x86_64-unknown-linux-musl, aarch64-unknown-linux-musl,
# x86_64-apple-darwin or aarch64-apple-darwin, and BINARY's own header must
# match it (64-bit ELF or thin 64-bit Mach-O for that CPU). The archive step
# uses GNU tar flags for reproducible archives, so run it on Linux.
#
# The layout matches what scripts/install.sh downloads from
# <base>/download/VERSION/<asset> and <base>/latest/download/<asset>.
set -euo pipefail
export LC_ALL=C

fail() {
    printf 'package-release: %s\n' "$*" >&2
    exit 1
}
check_version() {
    [[ $1 =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] || fail "invalid version: $1"
}
sha256() {
    if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | awk '{print $1}'; else shasum -a 256 "$1" | awk '{print $1}'; fi
}
# Same header reading as scripts/install.sh: the target a binary is built for.
binary_target() {
    local bytes
    bytes=$(od -An -tu1 -N20 -- "$1" 2>/dev/null) || bytes=
    # shellcheck disable=SC2086 # split the decimal byte list into fields
    set -- $bytes
    if [ "$#" -ge 20 ] && [ "$1 $2 $3 $4 $5 $6" = '127 69 76 70 2 1' ]; then
        case "${19} ${20}" in
            '62 0') echo x86_64-unknown-linux-musl && return 0 ;;
            '183 0') echo aarch64-unknown-linux-musl && return 0 ;;
        esac
    elif [ "$#" -ge 8 ] && [ "$1 $2 $3 $4" = '207 250 237 254' ]; then
        case "$5 $6 $7 $8" in
            '7 0 0 1') echo x86_64-apple-darwin && return 0 ;;
            '12 0 0 1') echo aarch64-apple-darwin && return 0 ;;
        esac
    fi
    echo unknown
}

command=${1:-}
case "$command" in
    archive)
        [ "$#" = 5 ] || fail 'usage: archive VERSION TARGET BINARY OUTDIR'
        version=$2 target=$3 binary=$4 out=$5
        check_version "$version"
        case "$target" in
            x86_64-unknown-linux-musl | aarch64-unknown-linux-musl | x86_64-apple-darwin | aarch64-apple-darwin) ;;
            *) fail "unsupported target: $target" ;;
        esac
        { [ -f "$binary" ] && [ ! -L "$binary" ]; } || fail "binary is not a regular file: $binary"
        built=$(binary_target "$binary")
        [ "$built" = "$target" ] || fail "$binary is built for $built, not $target"
        mkdir -p "$out"
        stage=$(mktemp -d)
        trap 'rm -rf -- "$stage"' EXIT
        cp -- "$binary" "$stage/ghtui"
        chmod 755 "$stage/ghtui"
        name="ghtui-$version-$target.tar.gz"
        tar -C "$stage" --owner=0 --group=0 --numeric-owner --mtime=@0 -cf - ghtui | gzip -n -9 >"$out/$name"
        printf '%s\n' "$out/$name"
        ;;
    finish)
        [ "$#" = 3 ] || fail 'usage: finish VERSION OUTDIR'
        version=$2 out=$3
        check_version "$version"
        here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
        cp -- "$here/install.sh" "$out/install.sh"
        printf '%s\n' "$version" >"$out/release-version.txt"
        count=0
        : >"$out/SHA256SUMS.tmp"
        for file in "$out"/ghtui-"$version"-*.tar.gz "$out/install.sh"; do
            [ -f "$file" ] || continue
            case "$file" in *.tar.gz) count=$((count + 1)) ;; esac
            printf '%s  %s\n' "$(sha256 "$file")" "$(basename -- "$file")" >>"$out/SHA256SUMS.tmp"
        done
        [ "$count" -gt 0 ] || fail "no ghtui-$version-*.tar.gz archives in $out"
        mv -f -- "$out/SHA256SUMS.tmp" "$out/SHA256SUMS"
        cat "$out/SHA256SUMS"
        ;;
    *) fail 'usage: package-release.sh archive VERSION TARGET BINARY OUTDIR | finish VERSION OUTDIR' ;;
esac
