#!/bin/bash
# Native Node module rebuilds (better-sqlite3, node-pty) and Linux Electron download.
#
# Sourced by install.sh. Do not run directly.
# shellcheck shell=bash

# ---- Build native modules in a clean directory ----
version_lt() {
    [ "$1" != "$2" ] && [ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | head -n 1)" = "$1" ]
}

better_sqlite3_build_version() {
    local detected_version="$1"

    case "$ELECTRON_VERSION" in
        41.*)
            if version_lt "$detected_version" "$MIN_BETTER_SQLITE3_VERSION_FOR_ELECTRON_41"; then
                echo "$MIN_BETTER_SQLITE3_VERSION_FOR_ELECTRON_41"
                return
            fi
            ;;
    esac

    echo "$detected_version"
}

prune_native_module_build_artifacts() {
    local module_dir="$1"
    local build_dir="$module_dir/build"

    [ -d "$build_dir" ] || return 0

    # node-gyp leaves Makefiles/configs/objects with absolute build paths.
    # The packaged runtime only needs the compiled .node binaries.
    find "$build_dir" -type f ! -name "*.node" -delete 2>/dev/null || true
    find "$build_dir" -type d -empty -delete 2>/dev/null || true
    find "$module_dir" -type f -name "*.target.mk" -delete 2>/dev/null || true
}

build_native_modules() {
    local app_extracted="$1"

    # Read versions from extracted app
    local bs3_ver bs3_build_ver npty_ver
    bs3_ver=$(node -p "require('$app_extracted/node_modules/better-sqlite3/package.json').version" 2>/dev/null || echo "")
    npty_ver=$(node -p "require('$app_extracted/node_modules/node-pty/package.json').version" 2>/dev/null || echo "")

    [ -n "$bs3_ver" ] || error "Could not detect better-sqlite3 version"
    [ -n "$npty_ver" ] || error "Could not detect node-pty version"

    info "Native modules: better-sqlite3@$bs3_ver, node-pty@$npty_ver"
    bs3_build_ver="$(better_sqlite3_build_version "$bs3_ver")"
    if [ "$bs3_build_ver" != "$bs3_ver" ]; then
        warn "Using better-sqlite3@$bs3_build_ver for Electron v$ELECTRON_VERSION compatibility (DMG has $bs3_ver)"
    fi

    # Build in a CLEAN directory (asar doesn't have full source)
    local build_dir="$WORK_DIR/native-build"
    mkdir -p "$build_dir"
    cd "$build_dir"

    echo '{"private":true}' > package.json

    info "Installing fresh sources from npm..."
    npm install "electron@$ELECTRON_VERSION" --save-dev --ignore-scripts 2>&1 >&2
    npm install "better-sqlite3@$bs3_build_ver" "node-pty@$npty_ver" --ignore-scripts 2>&1 >&2

    info "Compiling for Electron v$ELECTRON_VERSION (this takes ~1 min)..."
    info "Using Electron headers: $ELECTRON_HEADERS_URL"
    npm_config_disturl="$ELECTRON_HEADERS_URL" \
    NPM_CONFIG_DISTURL="$ELECTRON_HEADERS_URL" \
    npx --yes @electron/rebuild -v "$ELECTRON_VERSION" --force --dist-url "$ELECTRON_HEADERS_URL" 2>&1 >&2

    info "Native modules built successfully"

    # Copy compiled modules back into extracted app
    rm -rf "$app_extracted/node_modules/better-sqlite3"
    rm -rf "$app_extracted/node_modules/node-pty"
    cp -r "$build_dir/node_modules/better-sqlite3" "$app_extracted/node_modules/"
    cp -r "$build_dir/node_modules/node-pty" "$app_extracted/node_modules/"
    prune_native_module_build_artifacts "$app_extracted/node_modules/better-sqlite3"
    prune_native_module_build_artifacts "$app_extracted/node_modules/node-pty"
}

# ---- Download Linux Electron ----
download_electron() {
    info "Downloading Electron v${ELECTRON_VERSION} for Linux..."

    local electron_arch
    case "$ARCH" in
        x86_64)  electron_arch="x64" ;;
        aarch64) electron_arch="arm64" ;;
        armv7l)  electron_arch="armv7l" ;;
        *)       error "Unsupported architecture: $ARCH" ;;
    esac

    local electron_zip="electron-v${ELECTRON_VERSION}-linux-${electron_arch}.zip"
    local url
    if [ -n "$ELECTRON_MIRROR" ]; then
        url="${ELECTRON_MIRROR%/}/v${ELECTRON_VERSION}/${electron_zip}"
        info "Using Electron runtime mirror: ${ELECTRON_MIRROR%/}"
    else
        url="https://github.com/electron/electron/releases/download/v${ELECTRON_VERSION}/${electron_zip}"
    fi
    local electron_cache_dir="${CODEX_ELECTRON_CACHE_DIR:-$HOME/.cache/codex-desktop/electron}"
    local cached_zip="$electron_cache_dir/$electron_zip"
    local partial_zip="$cached_zip.part"

    mkdir -p "$electron_cache_dir"
    if [ ! -f "$cached_zip" ]; then
        info "Downloading $electron_zip into cache..."
        curl -L --fail --continue-at - --progress-bar -o "$partial_zip" "$url"
        mv "$partial_zip" "$cached_zip"
    else
        info "Using cached Electron archive: $cached_zip"
    fi

    cp "$cached_zip" "$WORK_DIR/electron.zip"
    mkdir -p "$INSTALL_DIR"
    cd "$INSTALL_DIR"
    unzip -qo "$WORK_DIR/electron.zip"

    info "Electron ready"
}
