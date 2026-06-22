#!/bin/bash
set -Eeuo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
PACKAGE_NAME="codex-desktop"
SYSTEM_ROOT="/opt/$PACKAGE_NAME"
SYSTEM_BUILD_INFO="$SYSTEM_ROOT/.codex-linux/build-info.json"
SYSTEM_ASSETS_ROOT="$SYSTEM_ROOT/content/webview/assets"
REMOTE_AVAILABILITY_MARKER="codexLinuxRemoteControlProfileAvailability||"
REMOTE_TABS_MARKER="codexLinuxRemoteControlProfileTabsAvailable"

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
Usage: bash scripts/verify-installed.sh [PACKAGE_FILE]

Verifies that the installed /opt/codex-desktop payload matches a built native
package from dist/ and that the Linux profile remote-control patches are
present in the installed webview assets.

When PACKAGE_FILE is omitted, the newest native package in dist/ is used.
EOF
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

    [ -n "$package_path" ] || error "Could not find a native package in $REPO_DIR/dist"
    printf '%s\n' "$package_path"
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

package_build_info_sha256() {
    local package_path="$1"
    local tmpdir build_info_path

    tmpdir="$(mktemp -d)"

    case "$package_path" in
        *.deb)
            command -v dpkg-deb >/dev/null 2>&1 || error "dpkg-deb is required to inspect $package_path"
            dpkg-deb -x "$package_path" "$tmpdir"
            ;;
        *.rpm)
            command -v rpm2cpio >/dev/null 2>&1 || error "rpm2cpio is required to inspect $package_path"
            command -v cpio >/dev/null 2>&1 || error "cpio is required to inspect $package_path"
            (cd "$tmpdir" && rpm2cpio "$package_path" | cpio -idm --quiet)
            ;;
        *.pkg.tar.*)
            command -v tar >/dev/null 2>&1 || error "tar is required to inspect $package_path"
            tar -xf "$package_path" -C "$tmpdir"
            ;;
        *)
            error "Unsupported package file: $package_path"
            ;;
    esac

    build_info_path="$tmpdir/opt/$PACKAGE_NAME/.codex-linux/build-info.json"
    [ -f "$build_info_path" ] || error "Package is missing Linux build metadata: $build_info_path"
    sha256sum "$build_info_path" | awk '{ print $1 }'
    rm -rf "$tmpdir"
}

file_sha256() {
    local path="$1"

    [ -f "$path" ] || error "Missing file: $path"
    sha256sum "$path" | awk '{ print $1 }'
}

asset_marker_present() {
    local prefix="$1"
    local marker="$2"
    local asset

    [ -d "$SYSTEM_ASSETS_ROOT" ] || return 1
    asset="$(
        find "$SYSTEM_ASSETS_ROOT" -maxdepth 1 -type f -name "${prefix}*.js" -printf '%f\n' \
            | sort \
            | head -n 1
    )"
    [ -n "$asset" ] || return 1
    grep -Fq "$marker" "$SYSTEM_ASSETS_ROOT/$asset"
}

main() {
    case "${1:-}" in
        -h|--help)
            usage
            exit 0
            ;;
    esac

    local package_file="${1:-}"
    if [ -z "$package_file" ]; then
        package_file="$(latest_built_package_file)"
    fi

    [ -f "$package_file" ] || error "Package file not found: $package_file"
    package_file="$(cd "$(dirname "$package_file")" && pwd)/$(basename "$package_file")"

    local expected_version installed_version expected_build_info_sha installed_build_info_sha failed=0
    expected_version="$(package_file_version "$package_file")"
    installed_version="$(installed_package_version)"
    expected_build_info_sha="$(package_build_info_sha256 "$package_file")"
    installed_build_info_sha="$(file_sha256 "$SYSTEM_BUILD_INFO")"

    echo "Package file: $package_file"
    echo "Expected package version: $expected_version"
    echo "Installed package version: $installed_version"
    if [ "$installed_version" != "$expected_version" ]; then
        warn "Installed package version does not match the package file"
        failed=1
    fi

    echo "Expected build-info SHA256: $expected_build_info_sha"
    echo "Installed build-info SHA256: $installed_build_info_sha"
    if [ "$installed_build_info_sha" != "$expected_build_info_sha" ]; then
        warn "Installed Linux build metadata does not match the package file"
        failed=1
    fi

    if asset_marker_present "use-plugin-install-flow-" "$REMOTE_AVAILABILITY_MARKER"; then
        echo "Profile availability marker: present"
    else
        warn "Profile availability marker is missing from installed webview assets"
        failed=1
    fi

    if asset_marker_present "remote-connections-settings-" "$REMOTE_TABS_MARKER"; then
        echo "Remote connections settings tabs marker: present"
    else
        warn "Remote connections settings tabs marker is missing from installed webview assets"
        failed=1
    fi

    if [ "$failed" -ne 0 ]; then
        cat >&2 <<EOF

Installed Codex Desktop does not match the built package.
Install the package, then fully quit and restart Codex Desktop:

  sudo apt-get install -y "$package_file"
  codex-desktop --profile desktop_fugu

EOF
        exit 1
    fi

    info "Installed Codex Desktop matches the package and required profile remote-control patches are present"
}

main "$@"
