#!/bin/bash
set -Eeuo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
PACKAGE_NAME="codex-desktop"
SYSTEM_APP_ASAR="/opt/$PACKAGE_NAME/resources/app.asar"
LOCAL_APP_ASAR="$REPO_DIR/codex-app/resources/app.asar"
SYSTEM_BUILD_INFO="/opt/$PACKAGE_NAME/.codex-linux/build-info.json"
LOCAL_BUILD_INFO="$REPO_DIR/codex-app/.codex-linux/build-info.json"

info() {
    echo "[INFO] $*" >&2
}

warn() {
    echo "[WARN] $*" >&2
}

error() {
    echo "[ERROR] $*" >&2
    exit 1
}

usage() {
    cat <<'EOF'
Usage: bash scripts/install-latest.sh

Installs or updates Codex Desktop on Linux in one command:
  1. Installs host dependencies when needed
  2. Downloads the latest upstream Codex.dmg
  3. Rebuilds codex-app/ from that DMG
  4. Builds the native package for this distro
  5. Installs that package
  6. Verifies the installed package version and Linux build metadata
  7. Prints the current, built, and final installed versions

Notes:
  - This command may prompt for sudo during dependency installation and package install.
  - It always runs ./install.sh --fresh so the cached DMG is replaced with the latest upstream build.
EOF
}

node_major() {
    local version major

    command -v node >/dev/null 2>&1 || return 1
    version="$(node -v 2>/dev/null || true)"
    major="${version#v}"
    major="${major%%.*}"
    case "$major" in
        ''|*[!0-9]*) return 1 ;;
    esac
    printf '%s\n' "$major"
}

version_major() {
    local version="$1"

    version="${version#*:}"
    if [[ "$version" =~ ^([0-9]+) ]]; then
        printf '%s\n' "${BASH_REMATCH[1]}"
        return 0
    fi

    return 1
}

detect_package_distro() {
    if command -v apt-get >/dev/null 2>&1; then
        printf '%s\n' "apt"
    elif command -v dnf5 >/dev/null 2>&1; then
        printf '%s\n' "dnf5"
    elif command -v dnf >/dev/null 2>&1; then
        printf '%s\n' "dnf"
    elif command -v pacman >/dev/null 2>&1; then
        printf '%s\n' "pacman"
    elif command -v zypper >/dev/null 2>&1; then
        printf '%s\n' "zypper"
    elif command -v rpm >/dev/null 2>&1; then
        printf '%s\n' "rpm"
    else
        printf '%s\n' "unknown"
    fi
}

system_nodejs_major() {
    local distro="$1"
    local version

    case "$distro" in
        apt)
            version="$(dpkg-query -W -f='${Version}\n' nodejs 2>/dev/null || true)"
            ;;
        pacman)
            version="$(pacman -Q nodejs 2>/dev/null | awk '{print $2}' || true)"
            ;;
        dnf|dnf5|rpm|zypper)
            version="$(rpm -q --queryformat '%{VERSION}\n' nodejs 2>/dev/null || true)"
            ;;
        *)
            return 1
            ;;
    esac

    [ -n "$version" ] || return 1
    version_major "$version"
}

system_nodejs_ready() {
    local distro="$1"
    local major

    major="$(system_nodejs_major "$distro" 2>/dev/null || true)"
    [ -n "$major" ] && [ "$major" -ge 20 ]
}

have_modern_7zip() {
    if command -v 7zz >/dev/null 2>&1 && 7zz 2>&1 | grep -qm 1 "7-Zip"; then
        return 0
    fi

    if ! command -v 7z >/dev/null 2>&1; then
        return 1
    fi

    ! 7z 2>&1 | grep -m 1 "7-Zip" | grep -q "16.02"
}

ensure_cargo_on_path() {
    if command -v cargo >/dev/null 2>&1; then
        return 0
    fi

    if [ -f "$HOME/.cargo/env" ]; then
        # shellcheck disable=SC1090
        . "$HOME/.cargo/env"
    fi

    command -v cargo >/dev/null 2>&1
}

dependencies_ready() {
    local major
    local distro

    for cmd in node npm npx python3 curl unzip make g++; do
        command -v "$cmd" >/dev/null 2>&1 || return 1
    done

    major="$(node_major 2>/dev/null || true)"
    [ -n "$major" ] && [ "$major" -ge 20 ] || return 1
    have_modern_7zip || return 1
    ensure_cargo_on_path || return 1
    distro="$(detect_package_distro)"
    if [ "$distro" != "unknown" ]; then
        system_nodejs_ready "$distro" || return 1
    fi

    return 0
}

installed_package_version() {
    if command -v pacman >/dev/null 2>&1; then
        local version
        version="$(pacman -Q "$PACKAGE_NAME" 2>/dev/null | awk '{print $2}' || true)"
        if [ -n "$version" ]; then
            printf '%s\n' "$version"
            return 0
        fi
    fi

    if command -v dpkg-query >/dev/null 2>&1; then
        local version
        version="$(dpkg-query -W -f='${Version}\n' "$PACKAGE_NAME" 2>/dev/null || true)"
        if [ -n "$version" ]; then
            printf '%s\n' "$version"
            return 0
        fi
    fi

    if command -v rpm >/dev/null 2>&1; then
        local version
        version="$(rpm -q --queryformat '%{VERSION}-%{RELEASE}\n' "$PACKAGE_NAME" 2>/dev/null || true)"
        if [ -n "$version" ] && [ "$version" != "$PACKAGE_NAME is not installed" ]; then
            printf '%s\n' "$version"
            return 0
        fi
    fi

    printf '%s\n' "not-installed"
}

extract_app_version_from_asar() {
    local asar_path="$1"
    local tmpdir version

    [ -f "$asar_path" ] || return 1

    tmpdir="$(mktemp -d)"
    if ! (
        cd "$tmpdir" &&
        npx --yes asar extract-file "$asar_path" package.json >/dev/null 2>&1
    ); then
        rm -rf "$tmpdir"
        return 1
    fi

    if [ ! -f "$tmpdir/package.json" ]; then
        rm -rf "$tmpdir"
        return 1
    fi

    version="$(
        node - "$tmpdir/package.json" <<'NODE'
const fs = require('node:fs');
const packagePath = process.argv[2];
const pkg = JSON.parse(fs.readFileSync(packagePath, 'utf8'));
process.stdout.write(String(pkg.version ?? ''));
NODE
    )"
    rm -rf "$tmpdir"

    [ -n "$version" ] || return 1
    printf '%s\n' "$version"
}

installed_app_version() {
    if extract_app_version_from_asar "$SYSTEM_APP_ASAR" 2>/dev/null; then
        return 0
    fi
    printf '%s\n' "not-installed"
}

local_built_app_version() {
    local version
    version="$(extract_app_version_from_asar "$LOCAL_APP_ASAR" 2>/dev/null || true)"
    [ -n "$version" ] || error "Could not determine the rebuilt Codex App version from $LOCAL_APP_ASAR"
    printf '%s\n' "$version"
}

package_file_version() {
    local package_path="$1"
    local version=""

    case "$package_path" in
        *.deb)
            command -v dpkg-deb >/dev/null 2>&1 || error "dpkg-deb is required to inspect $package_path"
            version="$(dpkg-deb -f "$package_path" Version 2>/dev/null || true)"
            ;;
        *.rpm)
            command -v rpm >/dev/null 2>&1 || error "rpm is required to inspect $package_path"
            version="$(rpm -qp --queryformat '%{VERSION}-%{RELEASE}\n' "$package_path" 2>/dev/null || true)"
            ;;
        *.pkg.tar.*)
            command -v tar >/dev/null 2>&1 || error "tar is required to inspect $package_path"
            version="$(tar -xOf "$package_path" .PKGINFO 2>/dev/null | awk -F' = ' '$1 == "pkgver" { print $2; exit }' || true)"
            ;;
        *)
            error "Unsupported package file: $package_path"
            ;;
    esac

    [ -n "$version" ] || error "Could not determine package version from $package_path"
    printf '%s\n' "$version"
}

build_info_sha256() {
    local build_info_path="$1"

    [ -f "$build_info_path" ] || error "Missing Linux build metadata: $build_info_path"
    sha256sum "$build_info_path" | awk '{ print $1 }'
}

latest_built_package_file() {
    local package_path

    package_path="$(
        find "$REPO_DIR/dist" -maxdepth 1 -type f \
            \( -name "${PACKAGE_NAME}_*.deb" -o -name "${PACKAGE_NAME}-*.rpm" -o -name "${PACKAGE_NAME}-*.pkg.tar.*" \) \
            -printf '%T@ %p\n' 2>/dev/null \
            | sort -n \
            | tail -n 1 \
            | cut -d' ' -f2-
    )"

    [ -n "$package_path" ] || error "Could not find the package that make package produced in $REPO_DIR/dist"
    printf '%s\n' "$package_path"
}

install_package_file() {
    local package_path="$1"

    case "$package_path" in
        *.deb)
            make -C "$REPO_DIR" install DEB="$package_path"
            ;;
        *.rpm)
            make -C "$REPO_DIR" install RPM="$package_path"
            ;;
        *.pkg.tar.*)
            make -C "$REPO_DIR" install PKG="$package_path"
            ;;
        *)
            error "Unsupported package file: $package_path"
            ;;
    esac
}

print_kv() {
    printf '  %-26s %s\n' "$1" "$2"
}

main() {
    case "${1:-}" in
        -h|--help)
            usage
            exit 0
            ;;
        "")
            ;;
        *)
            error "Unknown argument: $1"
            ;;
    esac

    cd "$REPO_DIR"

    if dependencies_ready; then
        info "Required build dependencies already available; skipping scripts/install-deps.sh"
    else
        info "Installing or verifying host dependencies"
        bash "$REPO_DIR/scripts/install-deps.sh"
        ensure_cargo_on_path || error "cargo is still unavailable after scripts/install-deps.sh"
    fi

    local current_package_version current_app_version
    current_package_version="$(installed_package_version)"
    current_app_version="$(installed_app_version)"

    echo "Current installed versions:"
    print_kv "Codex App" "$current_app_version"
    print_kv "Linux package" "$current_package_version"
    echo

    info "Rebuilding codex-app from the latest upstream DMG"
    ./install.sh --fresh

    local built_app_version built_build_info_sha256
    built_app_version="$(local_built_app_version)"
    built_build_info_sha256="$(build_info_sha256 "$LOCAL_BUILD_INFO")"
    info "Building native package"
    make package

    local package_file built_package_version
    package_file="$(latest_built_package_file)"
    built_package_version="$(package_file_version "$package_file")"

    echo "Built latest version:"
    print_kv "Codex App" "$built_app_version"
    print_kv "Linux package" "$built_package_version"
    print_kv "Package file" "$package_file"
    echo

    info "Installing built package"
    install_package_file "$package_file"

    local final_package_version final_app_version final_build_info_sha256
    final_package_version="$(installed_package_version)"
    final_app_version="$(installed_app_version)"
    final_build_info_sha256="$(build_info_sha256 "$SYSTEM_BUILD_INFO")"

    [ "$final_package_version" = "$built_package_version" ] || error "Installed Linux package version ($final_package_version) does not match the built package version ($built_package_version from $package_file)"
    [ "$final_app_version" = "$built_app_version" ] || error "Installed Codex App version ($final_app_version) does not match the rebuilt version ($built_app_version)"
    [ "$final_build_info_sha256" = "$built_build_info_sha256" ] || error "Installed Linux build metadata does not match the rebuilt app metadata (installed $final_build_info_sha256, expected $built_build_info_sha256)"

    echo "Final installed versions:"
    print_kv "Codex App" "$final_app_version"
    print_kv "Linux package" "$final_package_version"
    echo

    if [ "$current_app_version" = "$final_app_version" ]; then
        info "Codex App is already at the latest upstream version ($final_app_version)"
    else
        info "Codex App updated: $current_app_version -> $final_app_version"
    fi
}

main "$@"
