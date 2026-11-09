#!/usr/bin/env bash
#
# Build and install mrs.
#
# Defaults to a user-local install under ~/.local, which needs no sudo.
# Pass --system for /usr/local, or --prefix DIR for a custom location.

set -euo pipefail

APP="mrs"
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
CARGO="${CARGO:-cargo}"

PREFIX="${PREFIX:-}"
SYSTEM=0
UNINSTALL=0
BUILD=1
SUDO=()

die() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

info() {
    printf '%s\n' "$*"
}

usage() {
    cat <<EOF
Usage: $(basename "$0") [options]

Build and install $APP. By default installs for the current user under
~/.local, so no sudo is needed.

Options:
  --system         Install system-wide under /usr/local (uses sudo if needed).
  --prefix DIR     Install under DIR instead (default: ~/.local).
  --uninstall      Remove a previous installation instead of installing.
  --no-build       Skip "cargo build --release" and use the existing binary.
  -h, --help       Show this help.

Environment:
  PREFIX           Same as --prefix.
  CARGO            Cargo executable to use (default: cargo).

Examples:
  ./install.sh                     # install for the current user
  ./install.sh --system            # install under /usr/local
  ./install.sh --uninstall         # remove the user-local installation
  ./install.sh --prefix /opt/mrs   # custom prefix
EOF
}

path_writable() {
    local dir="$1"
    while [[ ! -e "$dir" ]]; do
        local parent
        parent="$(dirname -- "$dir")"
        [[ "$parent" == "$dir" ]] && break
        dir="$parent"
    done
    [[ -w "$dir" ]]
}

run() {
    "${SUDO[@]}" "$@"
}

detect_sudo() {
    SUDO=()
    if [[ "${EUID}" -ne 0 ]] && ! path_writable "$PREFIX"; then
        if command -v sudo >/dev/null 2>&1; then
            SUDO=(sudo)
        else
            die "cannot write to $PREFIX and sudo is not available; run as root or use --prefix"
        fi
    fi
}

update_caches() {
    local share="$PREFIX/share"
    if command -v update-desktop-database >/dev/null 2>&1; then
        run update-desktop-database "$share/applications" >/dev/null 2>&1 || true
    fi
    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
        run gtk-update-icon-cache -f -t "$share/icons/hicolor" >/dev/null 2>&1 || true
    fi
}

install_app() {
    local binary="$SCRIPT_DIR/target/release/$APP"

    if (( BUILD )); then
        command -v "$CARGO" >/dev/null 2>&1 || die "cargo not found; install a Rust toolchain first"
        info "Building $APP (release)…"
        "$CARGO" build --release --manifest-path "$SCRIPT_DIR/Cargo.toml"
    fi

    [[ -x "$binary" ]] || die "binary not found at $binary; run without --no-build first"

    detect_sudo
    run mkdir -p "$PREFIX/bin" "$PREFIX/share/applications" "$PREFIX/share/icons/hicolor"

    info "Installing binary to $PREFIX/bin/$APP"
    run install -Dm755 "$binary" "$PREFIX/bin/$APP"

    info "Installing desktop entry to $PREFIX/share/applications/$APP.desktop"
    run install -Dm644 "$SCRIPT_DIR/assets/$APP.desktop" \
        "$PREFIX/share/applications/$APP.desktop"

    local icon size
    for icon in "$SCRIPT_DIR"/assets/icons/"$APP"-*.png; do
        [[ -e "$icon" ]] || continue
        size="$(basename -- "$icon" .png)"
        size="${size#"$APP"-}"
        info "Installing ${size}x${size} icon"
        run install -Dm644 "$icon" \
            "$PREFIX/share/icons/hicolor/${size}x${size}/apps/$APP.png"
    done

    update_caches

    info "Installed $APP to $PREFIX"
    case ":${PATH}:" in
    *":$PREFIX/bin:"*) ;;
    *) info "Note: $PREFIX/bin is not on your PATH; add it to run '$APP' from a terminal." ;;
    esac
}

uninstall_app() {
    detect_sudo

    info "Removing $PREFIX/bin/$APP"
    run rm -f "$PREFIX/bin/$APP"

    info "Removing desktop entry"
    run rm -f "$PREFIX/share/applications/$APP.desktop"

    info "Removing icons"
    local icons=("$PREFIX"/share/icons/hicolor/*/apps/"$APP".png)
    if [[ -e "${icons[0]}" ]]; then
        run rm -f "${icons[@]}"
    fi

    update_caches
    info "Uninstalled $APP from $PREFIX"
}

while (($#)); do
    case "$1" in
    -h | --help)
        usage
        exit 0
        ;;
    --system)
        SYSTEM=1
        shift
        ;;
    --prefix)
        [[ $# -ge 2 ]] || die "--prefix needs a directory"
        PREFIX="$2"
        shift 2
        ;;
    --prefix=*)
        PREFIX="${1#*=}"
        shift
        ;;
    --uninstall)
        UNINSTALL=1
        shift
        ;;
    --no-build)
        BUILD=0
        shift
        ;;
    --)
        shift
        break
        ;;
    *) die "unknown option: $1 (try --help)" ;;
    esac
done

if [[ -z "$PREFIX" ]]; then
    if (( SYSTEM )); then
        PREFIX="/usr/local"
    else
        PREFIX="$HOME/.local"
    fi
fi

# Normalise to an absolute path.
case "$PREFIX" in
/*) ;;
*) PREFIX="$PWD/$PREFIX" ;;
esac

if (( UNINSTALL )); then
    uninstall_app
else
    install_app
fi
