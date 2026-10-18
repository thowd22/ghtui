#!/usr/bin/env bash
# ghtui installer for Linux (x86_64 and aarch64, static musl binaries).
#
#   curl -fsSL https://github.com/thowd22/ghtui/releases/latest/download/install.sh | bash
#   curl -fsSL .../install.sh | bash -s -- --version v0.1.0
#
# Installs one file, ghtui, into ~/.local/bin (no sudo, no shell profile edits)
# after verifying its SHA-256 against the release SHA256SUMS. git and gh stay
# external tools; gh owns all GitHub authentication.
#
# Options (or environment variables):
#   --version vX.Y.Z      GHTUI_VERSION      exact release tag (default: latest)
#   --install-dir DIR     GHTUI_INSTALL_DIR  absolute directory (default: ~/.local/bin)
#   --release-base URL    GHTUI_RELEASE_BASE release root (default: GitHub releases)
#   --uninstall                              remove the installed executable only
#
# Everything is defined in functions and the single entry point is the last
# line, so a truncated `curl | bash` download cannot run a partial install.

ghtui_say() { printf 'ghtui installer: %s\n' "$*"; }
ghtui_fail() {
    printf 'ghtui installer: error: %s\n' "$*" >&2
    exit 1
}

ghtui_usage() {
    cat <<'EOF'
ghtui installer for Linux (x86_64 and aarch64, static musl binaries).
Usage: install.sh [--version vX.Y.Z] [--install-dir DIR] [--release-base URL]
       install.sh --uninstall [--install-dir DIR]

  --version vX.Y.Z    install an exact release tag (default: latest release)
  --install-dir DIR   absolute install directory (default: $HOME/.local/bin)
  --release-base URL  release root (default: https://github.com/thowd22/ghtui/releases)
  --uninstall         remove the installed ghtui executable; config, cache and
                      drafts are kept

Environment: GHTUI_VERSION, GHTUI_INSTALL_DIR, GHTUI_RELEASE_BASE.
With curl: curl -fsSL URL/install.sh | bash -s -- --version v0.1.0
No sudo is used and no shell profile is changed.
EOF
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
                *) ghtui_fail "unsupported Linux CPU architecture: $cpu (x86_64 and aarch64 are published)" ;;
            esac
            ;;
        Darwin)
            ghtui_fail 'macOS release binaries are not published yet; build from source with: cargo install --locked --git https://github.com/thowd22/ghtui'
            ;;
        *) ghtui_fail "unsupported operating system: $os (Linux only for now)" ;;
    esac
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
    curl --disable --fail --silent --show-error --location --max-redirs 10 \
        --proto "=$ghtui_proto" --proto-redir "=$ghtui_proto" \
        --connect-timeout 20 --max-time 600 --retry 2 \
        --output "$2" "$1" || ghtui_fail "download failed: $1"
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
            rm -f -- "$binary"
            ghtui_say "removed $binary (configuration, cache and drafts were kept)"
        else
            ghtui_say "nothing to remove: $binary does not exist"
        fi
        return 0
    fi

    local target
    target=$(ghtui_target)
    local tool
    for tool in curl tar mktemp uname awk; do
        command -v "$tool" >/dev/null 2>&1 || ghtui_fail "$tool is required"
    done
    ghtui_proto=$(ghtui_protocol "$base")
    base=${base%/}

    if [ -n "$version" ]; then
        case "$version" in
            v[0-9]*) ;;
            [0-9]*) version="v$version" ;;
            *) ghtui_fail "invalid version: $version (expected vX.Y.Z)" ;;
        esac
        [[ $version =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] ||
            ghtui_fail "invalid version: $version (expected vX.Y.Z)"
    fi

    [ ! -L "$install_dir" ] || [ -d "$install_dir" ] || ghtui_fail "$install_dir is a broken symlink"
    mkdir -p -- "$install_dir" || ghtui_fail "cannot create $install_dir"
    [ -w "$install_dir" ] || ghtui_fail "$install_dir is not writable (this installer never uses sudo)"
    if [ -L "$binary" ] || { [ -e "$binary" ] && [ ! -f "$binary" ]; }; then
        ghtui_fail "$binary exists and is not a regular file; refusing to replace it"
    fi

    trap ghtui_cleanup EXIT
    trap 'exit 130' INT
    trap 'exit 143' TERM
    ghtui_stage=$(mktemp -d "${TMPDIR:-/tmp}/ghtui-install.XXXXXX") || ghtui_fail 'cannot create a temporary directory'

    if [ -z "$version" ]; then
        ghtui_fetch "$base/latest/download/release-version.txt" "$ghtui_stage/latest"
        version=$(head -n 1 "$ghtui_stage/latest" | tr -d '[:space:]')
        [[ $version =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] ||
            ghtui_fail 'the latest release did not name a valid version'
    fi

    local archive="ghtui-$version-$target.tar.gz"
    ghtui_say "installing ghtui $version ($target) into $install_dir"
    ghtui_fetch "$base/download/$version/SHA256SUMS" "$ghtui_stage/SHA256SUMS"
    ghtui_fetch "$base/download/$version/$archive" "$ghtui_stage/$archive"

    local expected actual count
    count=$(awk -v name="$archive" '$2 == name || $2 == "*" name { n++ } END { print n + 0 }' "$ghtui_stage/SHA256SUMS")
    [ "$count" = 1 ] || ghtui_fail "SHA256SUMS has no single entry for $archive"
    expected=$(awk -v name="$archive" '$2 == name || $2 == "*" name { print $1 }' "$ghtui_stage/SHA256SUMS")
    [[ $expected =~ ^[0-9a-f]{64}$ ]] || ghtui_fail "malformed checksum for $archive"
    actual=$(ghtui_sha256 "$ghtui_stage/$archive")
    [ "$actual" = "$expected" ] ||
        ghtui_fail "checksum mismatch for $archive (expected $expected, got $actual); nothing was installed"

    # The archive must hold exactly one regular file named ghtui.
    local members
    members=$(tar -tzf "$ghtui_stage/$archive" 2>/dev/null) || ghtui_fail "$archive is not a valid tar.gz archive"
    [ "$members" = ghtui ] || ghtui_fail "$archive must contain exactly one file named ghtui"
    mkdir "$ghtui_stage/unpack"
    tar -xzf "$ghtui_stage/$archive" -C "$ghtui_stage/unpack" --no-same-owner ghtui ||
        ghtui_fail "cannot unpack $archive"
    local fresh="$ghtui_stage/unpack/ghtui"
    { [ -f "$fresh" ] && [ ! -L "$fresh" ]; } || ghtui_fail "$archive does not contain a regular ghtui file"
    chmod 755 "$fresh"

    local reported
    reported=$("$fresh" --version 2>/dev/null) || ghtui_fail 'the downloaded ghtui does not run on this system'
    [ "$reported" = "ghtui ${version#v}" ] ||
        ghtui_fail "downloaded binary reports '$reported', expected 'ghtui ${version#v}'"

    if [ -f "$binary" ] && cmp -s "$fresh" "$binary"; then
        ghtui_say "ghtui $version is already installed at $binary"
    else
        # Same-directory temporary file + rename keeps the old binary usable
        # until the new one is complete.
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
