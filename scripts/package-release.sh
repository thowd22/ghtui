#!/usr/bin/env bash
# Assemble ghtui release assets locally. Never builds, uploads or tags.
#
#   package-release.sh archive VERSION TARGET BINARY OUTDIR
#       -> OUTDIR/ghtui-VERSION-TARGET.tar.gz (one member: ghtui, mode 0755)
#   package-release.sh finish VERSION OUTDIR
#       -> OUTDIR/install.sh, OUTDIR/release-version.txt, OUTDIR/SHA256SUMS
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

command=${1:-}
case "$command" in
    archive)
        [ "$#" = 5 ] || fail 'usage: archive VERSION TARGET BINARY OUTDIR'
        version=$2 target=$3 binary=$4 out=$5
        check_version "$version"
        case "$target" in
            x86_64-unknown-linux-musl | aarch64-unknown-linux-musl) ;;
            *) fail "unsupported target: $target" ;;
        esac
        { [ -f "$binary" ] && [ ! -L "$binary" ]; } || fail "binary is not a regular file: $binary"
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
