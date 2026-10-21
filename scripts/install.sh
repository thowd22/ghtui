#!/usr/bin/env bash
# ghtui installer for Linux and macOS (x86_64 and aarch64).
#
#   curl -fsSL https://github.com/thowd22/ghtui/releases/latest/download/install.sh | bash
#   curl -fsSL .../install.sh | bash -s -- --version v0.1.0
#
# Installs one file, ghtui, into ~/.local/bin (no sudo, no shell profile edits)
# after verifying its SHA-256 against the release SHA256SUMS and checking that
# it is built for this OS and CPU. git and gh stay external tools; gh owns all
# GitHub authentication.
#
# Options (or environment variables):
#   --version vX.Y.Z      GHTUI_VERSION      exact release tag (default: latest)
#   --install-dir DIR     GHTUI_INSTALL_DIR  absolute directory (default: ~/.local/bin)
#   --release-base URL    GHTUI_RELEASE_BASE release root (default: GitHub releases)
#   --uninstall                              remove the installed executable only
#
# The script needs Bash 3.2 or newer, curl, tar, gzip and the usual POSIX
# tools; it never needs Rust, Node or jq. Everything is defined in functions
# and the single entry point is the last line, so a truncated `curl | bash`
# download cannot run a partial install.

ghtui_say() { printf 'ghtui installer: %s\n' "$*"; }
ghtui_fail() {
    printf 'ghtui installer: error: %s\n' "$*" >&2
    exit 1
}

ghtui_usage() {
    cat <<'EOF'
ghtui installer for Linux and macOS (x86_64 and aarch64).
Usage: install.sh [--version vX.Y.Z] [--install-dir DIR] [--release-base URL]
       install.sh --uninstall [--install-dir DIR]

  --version vX.Y.Z    install an exact release tag (default: latest release)
  --install-dir DIR   absolute install directory (default: $HOME/.local/bin)
  --release-base URL  release root (default: https://github.com/thowd22/ghtui/releases)
  --uninstall         remove the installed ghtui executable; configuration,
                      cache, drafts and repositories are kept

Environment: GHTUI_VERSION, GHTUI_INSTALL_DIR, GHTUI_RELEASE_BASE.
With curl: curl -fsSL URL/install.sh | bash -s -- --version v0.1.0
No sudo is used and no shell profile is changed.
EOF
}

# Versions are plain vMAJOR.MINOR.PATCH with an optional pre-release suffix.
# grep -E rather than [[ =~ ]] keeps every function parseable by a POSIX sh,
# so running the script with sh reaches the clear "needs Bash" message.
ghtui_valid_version() {
    printf '%s\n' "$1" | grep -Eqx 'v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?'
}

ghtui_target() {
    local os cpu
    os=$(uname -s) || ghtui_fail 'cannot determine the operating system'
    cpu=$(uname -m) || ghtui_fail 'cannot determine the CPU architecture'
    case "$os" in
        Linux)
            case "$cpu" in
                x86_64 | amd64) printf 'x86_64-unknown-linux-musl\n' ;;
                aarch64 | arm64) printf 'aarch64-unknown-linux-musl\n' ;;
                *) ghtui_fail "unsupported Linux CPU architecture: $cpu (x86_64 and aarch64 are supported)" ;;
            esac
            ;;
        Darwin)
            # A shell running under Rosetta reports x86_64 on Apple silicon;
            # the native arm64 build is the right one there.
            if [ "$cpu" = x86_64 ] && [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || :)" = 1 ]; then
                cpu=arm64
            fi
            case "$cpu" in
                x86_64) printf 'x86_64-apple-darwin\n' ;;
                arm64 | aarch64) printf 'aarch64-apple-darwin\n' ;;
                *) ghtui_fail "unsupported macOS CPU architecture: $cpu (x86_64 and arm64 are supported)" ;;
            esac
            ;;
        *) ghtui_fail "unsupported operating system: $os (Linux and macOS are supported)" ;;
    esac
}

# Print the target a native executable is built for, read from its header
# without running it: 64-bit little-endian ELF for Linux, thin 64-bit
# Mach-O for macOS. Anything else prints "unknown".
ghtui_binary_target() {
    local bytes
    bytes=$(od -An -tu1 -N20 -- "$1" 2>/dev/null) || bytes=
    # shellcheck disable=SC2086 # split the decimal byte list into fields
    set -- $bytes
    if [ "$#" -ge 20 ] && [ "$1 $2 $3 $4 $5 $6" = '127 69 76 70 2 1' ]; then
        case "${19} ${20}" in
            '62 0') printf 'x86_64-unknown-linux-musl\n' && return 0 ;;
            '183 0') printf 'aarch64-unknown-linux-musl\n' && return 0 ;;
        esac
    elif [ "$#" -ge 8 ] && [ "$1 $2 $3 $4" = '207 250 237 254' ]; then
        case "$5 $6 $7 $8" in
            '7 0 0 1') printf 'x86_64-apple-darwin\n' && return 0 ;;
            '12 0 0 1') printf 'aarch64-apple-darwin\n' && return 0 ;;
        esac
    fi
    printf 'unknown\n'
}

ghtui_sha256() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{print $1}'
    else
        ghtui_fail 'sha256sum (coreutils) or shasum is required to verify the download'
    fi
}

# Only HTTPS is accepted for remote releases. file:// and loopback http:// are
# accepted so a release can be verified from a local directory before upload.
ghtui_protocol() {
    case "$1" in
        https://*) printf 'https\n' ;;
        file:///*) printf 'file\n' ;;
        http://127.0.0.1/* | http://127.0.0.1:* | http://localhost/* | http://localhost:*) printf 'http\n' ;;
        *) ghtui_fail "release base must be an https:// URL (got: $1)" ;;
    esac
}

ghtui_fetch() {
    # --disable must come first: it stops ~/.curlrc from changing the transfer.
    # Redirects may only stay on the same protocol (HTTPS for real releases),
    # and every transfer is bounded in time and size.
    curl --disable --fail --silent --show-error --location --max-redirs 10 \
        --proto "=$ghtui_proto" --proto-redir "=$ghtui_proto" \
        --tlsv1.2 --connect-timeout 20 --max-time 600 --max-filesize 209715200 \
        --retry 2 --output "$2" "$1" || ghtui_fail "download failed: $1"
}

# An existing destination must look like a ghtui executable before this
# installer replaces or removes it, so an unrelated file named ghtui is kept.
ghtui_owned() {
    LC_ALL=C grep -qaF 'ghtui' -- "$1" 2>/dev/null
}

# shellcheck disable=SC2317 # invoked by the EXIT trap below
ghtui_cleanup() {
    if [ -n "${ghtui_stage:-}" ] && [ -d "$ghtui_stage" ]; then
        rm -rf -- "$ghtui_stage"
    fi
    if [ -n "${ghtui_partial:-}" ] && [ -e "$ghtui_partial" ]; then
        rm -f -- "$ghtui_partial"
    fi
}

ghtui_path_hint() {
    case ":${PATH:-}:" in
        *":$1:"*)
            local found
            found=$(command -v ghtui 2>/dev/null || :)
            if [ -n "$found" ] && [ "$found" != "$1/ghtui" ]; then
                ghtui_say "warning: another ghtui earlier on PATH shadows this one: $found"
                ghtui_say "remove it, or put $1 before its directory on your PATH"
            fi
            ;;
        *)
            ghtui_say "$1 is not on your PATH. Add it, then open a new shell:"
            # shellcheck disable=SC2016 # $PATH is meant literally in the hint.
            printf '  bash/zsh: echo '\''export PATH="%s:$PATH"'\'' >> ~/.bashrc   (or ~/.zshrc)\n' "$1"
            printf '  fish:     fish_add_path %s\n' "$1"
            ;;
    esac
}

ghtui_main() {
    if [ -z "${BASH_VERSION:-}" ]; then
        ghtui_fail 'this installer needs Bash 3.2 or newer; run it with: curl -fsSL URL/install.sh | bash'
    fi
    set -eu
    umask 022
    export LC_ALL=C

    local version=${GHTUI_VERSION:-}
    local install_dir=${GHTUI_INSTALL_DIR:-}
    local base=${GHTUI_RELEASE_BASE:-https://github.com/thowd22/ghtui/releases}
    local uninstall=0 arg
    while [ "$#" -gt 0 ]; do
        arg=$1
        shift
        case "$arg" in
            --version | --install-dir | --release-base)
                [ "$#" -gt 0 ] || ghtui_fail "$arg needs a value"
                case "$arg" in
                    --version) version=$1 ;;
                    --install-dir) install_dir=$1 ;;
                    --release-base) base=$1 ;;
                esac
                shift
                ;;
            --uninstall) uninstall=1 ;;
            -h | --help)
                ghtui_usage
                return 0
                ;;
            *) ghtui_fail "unknown option: $arg (see --help)" ;;
        esac
    done

    if [ -z "$install_dir" ]; then
        [ -n "${HOME:-}" ] || ghtui_fail 'HOME is not set; pass --install-dir DIR'
        install_dir="$HOME/.local/bin"
    fi
    case "$install_dir" in
        /*) ;;
        *) ghtui_fail "install directory must be an absolute path: $install_dir" ;;
    esac
    while [ "${install_dir%/}" != "$install_dir" ]; do install_dir=${install_dir%/}; done
    [ -n "$install_dir" ] || ghtui_fail 'refusing to install into /'
    local binary="$install_dir/ghtui"

    if [ "$uninstall" = 1 ]; then
        if [ -L "$binary" ]; then
            ghtui_fail "$binary is a symlink not created by this installer; remove it yourself"
        elif [ -f "$binary" ]; then
            ghtui_owned "$binary" || ghtui_fail "$binary does not look like ghtui; refusing to remove it"
            rm -f -- "$binary"
            ghtui_say "removed $binary (configuration, cache, drafts and repositories were kept)"
        elif [ -e "$binary" ]; then
            ghtui_fail "$binary is not a regular file; refusing to remove it"
        else
            ghtui_say "nothing to remove: $binary does not exist"
        fi
        return 0
    fi

    local target
    target=$(ghtui_target)
    local tool
    for tool in curl tar gzip mktemp awk grep od; do
        command -v "$tool" >/dev/null 2>&1 ||
            ghtui_fail "$tool is required; install it with your system package manager, then run this installer again"
    done
    ghtui_proto=$(ghtui_protocol "$base")
    base=${base%/}

    if [ -n "$version" ]; then
        case "$version" in
            v[0-9]*) ;;
            [0-9]*) version="v$version" ;;
            *) ghtui_fail "invalid version: $version (expected vX.Y.Z)" ;;
        esac
        ghtui_valid_version "$version" || ghtui_fail "invalid version: $version (expected vX.Y.Z)"
    fi

    [ ! -L "$install_dir" ] || [ -d "$install_dir" ] || ghtui_fail "$install_dir is a broken symlink"
    mkdir -p -- "$install_dir" || ghtui_fail "cannot create $install_dir"
    [ -w "$install_dir" ] || ghtui_fail "$install_dir is not writable (this installer never uses sudo)"
    if [ -L "$binary" ] || { [ -e "$binary" ] && [ ! -f "$binary" ]; }; then
        ghtui_fail "$binary exists and is not a regular file; refusing to replace it"
    fi
    if [ -f "$binary" ] && ! ghtui_owned "$binary"; then
        ghtui_fail "$binary does not look like ghtui; move it away or pass --install-dir DIR"
    fi

    trap ghtui_cleanup EXIT
    trap 'exit 130' INT
    trap 'exit 143' TERM
    ghtui_stage=$(mktemp -d "${TMPDIR:-/tmp}/ghtui-install.XXXXXX") || ghtui_fail 'cannot create a temporary directory'

    # "Latest" is resolved exactly once; every later request names that version.
    if [ -z "$version" ]; then
        ghtui_fetch "$base/latest/download/release-version.txt" "$ghtui_stage/latest"
        version=$(head -n 1 "$ghtui_stage/latest" | tr -d '[:space:]')
        ghtui_valid_version "$version" || ghtui_fail 'the latest release did not name a valid version'
    fi

    local archive="ghtui-$version-$target.tar.gz"
    ghtui_say "installing ghtui $version ($target) into $install_dir"
    ghtui_fetch "$base/download/$version/SHA256SUMS" "$ghtui_stage/SHA256SUMS"

    local expected actual count
    count=$(awk -v name="$archive" '$2 == name || $2 == "*" name { n++ } END { print n + 0 }' "$ghtui_stage/SHA256SUMS")
    if [ "$count" = 0 ] && grep -Eq '^[0-9a-f]{64}  \*?ghtui-.*\.tar\.gz$' "$ghtui_stage/SHA256SUMS"; then
        ghtui_fail "release $version has no ghtui build for $target; build from source instead: https://github.com/thowd22/ghtui#build-from-source"
    fi
    [ "$count" = 1 ] || ghtui_fail "SHA256SUMS has no single entry for $archive"
    expected=$(awk -v name="$archive" '$2 == name || $2 == "*" name { print $1 }' "$ghtui_stage/SHA256SUMS")
    printf '%s\n' "$expected" | grep -Eqx '[0-9a-f]{64}' || ghtui_fail "malformed checksum for $archive"

    ghtui_fetch "$base/download/$version/$archive" "$ghtui_stage/$archive"
    actual=$(ghtui_sha256 "$ghtui_stage/$archive")
    [ "$actual" = "$expected" ] ||
        ghtui_fail "checksum mismatch for $archive (expected $expected, got $actual); nothing was installed"

    # The archive must hold exactly one regular file named ghtui. Names and
    # types are checked from the listing before anything is extracted.
    local members kinds
    members=$(tar -tzf "$ghtui_stage/$archive" 2>/dev/null) || ghtui_fail "$archive is not a valid tar.gz archive"
    [ "$members" = ghtui ] || ghtui_fail "$archive must contain exactly one file named ghtui"
    kinds=$(tar -tvzf "$ghtui_stage/$archive" 2>/dev/null | awk '{ print substr($0, 1, 1) }') || ghtui_fail "$archive is not a valid tar.gz archive"
    [ "$kinds" = - ] || ghtui_fail "$archive must contain exactly one regular file named ghtui"
    mkdir "$ghtui_stage/unpack"
    tar --no-same-owner -xzf "$ghtui_stage/$archive" -C "$ghtui_stage/unpack" ghtui ||
        ghtui_fail "cannot unpack $archive"
    local fresh="$ghtui_stage/unpack/ghtui"
    { [ -f "$fresh" ] && [ ! -L "$fresh" ]; } || ghtui_fail "$archive does not contain a regular ghtui file"

    # The executable's own header must name this target before it is ever run.
    local built
    built=$(ghtui_binary_target "$fresh")
    [ "$built" = "$target" ] ||
        ghtui_fail "$archive holds a binary built for $built, not $target; nothing was installed"
    chmod 755 "$fresh"

    local reported
    reported=$("$fresh" --version 2>/dev/null) || ghtui_fail 'the downloaded ghtui does not run on this system; nothing was installed'
    [ "$reported" = "ghtui ${version#v}" ] ||
        ghtui_fail "downloaded binary reports '$reported', expected 'ghtui ${version#v}'"

    if [ -f "$binary" ] && cmp -s "$fresh" "$binary"; then
        ghtui_say "ghtui $version is already installed at $binary"
    else
        # Same-directory temporary file + rename keeps the old binary usable
        # until the new one is complete; rename is atomic on one filesystem.
        ghtui_partial="$install_dir/.ghtui.partial.$$"
        cp -- "$fresh" "$ghtui_partial"
        chmod 755 "$ghtui_partial"
        mv -f -- "$ghtui_partial" "$binary"
        ghtui_partial=
        ghtui_say "installed ghtui $version at $binary"
    fi

    ghtui_path_hint "$install_dir"
    command -v git >/dev/null 2>&1 || ghtui_say 'note: git is not installed; ghtui needs it for local repositories (https://git-scm.com/downloads)'
    command -v gh >/dev/null 2>&1 || ghtui_say 'note: gh is not installed; ghtui uses it for GitHub (https://cli.github.com/), then run: gh auth login'
    return 0
}

{
    ghtui_main "$@"
    exit "$?"
}
