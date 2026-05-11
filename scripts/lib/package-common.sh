#!/bin/bash

info() {
    echo "[INFO] $*" >&2
}

error() {
    echo "[ERROR] $*" >&2
    exit 1
}

ensure_file_exists() {
    local path="$1"
    local label="$2"
    [ -f "$path" ] || error "Missing $label: $path"
}

ensure_app_layout() {
    [ -d "$APP_DIR" ] || error "Missing app directory: $APP_DIR. Run ./install.sh first."
    [ -x "$APP_DIR/start.sh" ] || error "Missing launcher: $APP_DIR/start.sh"
}

sed_escape_replacement() {
    printf '%s' "$1" | sed -e 's/[\/&]/\\&/g'
}

render_desktop_entry() {
    local target="$1"
    local package_name
    local display_name
    local comment

    package_name="$(sed_escape_replacement "$PACKAGE_NAME")"
    display_name="$(sed_escape_replacement "${PACKAGE_DISPLAY_NAME:-Codex Desktop}")"
    comment="$(sed_escape_replacement "${PACKAGE_COMMENT:-Run Codex Desktop on Linux}")"

    sed \
        -e "s/codex-desktop/$package_name/g" \
        -e "s/^Name=.*/Name=$display_name/g" \
        -e "s/^Comment=.*/Comment=$comment/g" \
        "$DESKTOP_TEMPLATE" > "$target"
    chmod 0644 "$target"
}

render_packaged_runtime_helper() {
    local target="$1"
    local package_name

    package_name="$(sed_escape_replacement "$PACKAGE_NAME")"
    sed -e "s/codex-desktop/$package_name/g" "$PACKAGED_RUNTIME_SOURCE" > "$target"
    chmod 0644 "$target"
}

updater_binary_is_stale() {
    local binary="$1"

    [ -x "$binary" ] || return 0

    local source
    for source in "$REPO_DIR/Cargo.toml" "$REPO_DIR/Cargo.lock"; do
        if [ -f "$source" ] && [ "$source" -nt "$binary" ]; then
            return 0
        fi
    done

    while IFS= read -r -d '' source; do
        if [ "$source" -nt "$binary" ]; then
            return 0
        fi
    done < <(find "$REPO_DIR/updater" -type f -print0 2>/dev/null)

    return 1
}

find_cargo_command() {
    if command -v cargo >/dev/null 2>&1; then
        command -v cargo
        return 0
    fi

    if [ -x "$HOME/.cargo/bin/cargo" ]; then
        echo "$HOME/.cargo/bin/cargo"
        return 0
    fi

    return 1
}

ensure_updater_binary() {
    local cargo_cmd=""

    if [ -x "$UPDATER_BINARY_SOURCE" ] && ! updater_binary_is_stale "$UPDATER_BINARY_SOURCE"; then
        return
    fi

    [ -f "$REPO_DIR/Cargo.toml" ] || error "Missing updater binary: $UPDATER_BINARY_SOURCE"
    cargo_cmd="$(find_cargo_command)" || error "cargo is required to build codex-update-manager.
Install the Rust toolchain:
  bash scripts/install-deps.sh        # auto-installs via rustup
  # or manually: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"

    info "Building codex-update-manager release binary"
    "$cargo_cmd" build --release -p codex-update-manager >&2
    [ -x "$UPDATER_BINARY_SOURCE" ] || error "Failed to build updater binary: $UPDATER_BINARY_SOURCE"
}

stage_common_package_files() {
    local root="$1"
    local app_root="$root/opt/$PACKAGE_NAME"
    local polkit_policy="$REPO_DIR/packaging/linux/com.github.ilysenko.codex-desktop-linux.update.policy"

    ensure_file_exists "$polkit_policy" "polkit policy"

    mkdir -p \
        "$root/opt" \
        "$root/usr/bin" \
        "$root/usr/lib/systemd/user" \
        "$root/usr/share/applications" \
        "$root/usr/share/icons/hicolor/256x256/apps" \
        "$root/usr/share/polkit-1/actions"

    rm -rf "$app_root"
    cp -aT "$APP_DIR" "$app_root"
    mkdir -p "$app_root/.codex-linux"
    cp "$ICON_SOURCE" "$app_root/.codex-linux/$PACKAGE_NAME.png"
    render_desktop_entry "$root/usr/share/applications/$PACKAGE_NAME.desktop"
    cp "$ICON_SOURCE" "$root/usr/share/icons/hicolor/256x256/apps/$PACKAGE_NAME.png"
    cp "$UPDATER_BINARY_SOURCE" "$root/usr/bin/codex-update-manager"
    chmod 0755 "$root/usr/bin/codex-update-manager"
    cp "$UPDATER_SERVICE_SOURCE" "$root/usr/lib/systemd/user/codex-update-manager.service"
    chmod 0644 "$root/usr/lib/systemd/user/codex-update-manager.service"
    cp "$polkit_policy" "$root/usr/share/polkit-1/actions/com.github.ilysenko.codex-desktop-linux.update.policy"
    chmod 0644 "$root/usr/share/polkit-1/actions/com.github.ilysenko.codex-desktop-linux.update.policy"
    render_packaged_runtime_helper "$app_root/.codex-linux/codex-packaged-runtime.sh"
}

stage_update_builder_bundle() {
    local root="$1"
    local update_builder_root="$root/opt/$PACKAGE_NAME/update-builder"
    local node_runtime_source="$APP_DIR/resources/node-runtime"

    mkdir -p \
        "$update_builder_root/scripts" \
        "$update_builder_root/scripts/lib" \
        "$update_builder_root/scripts/patches" \
        "$update_builder_root/launcher" \
        "$update_builder_root/linux-features" \
        "$update_builder_root/packaging/linux" \
        "$update_builder_root/assets"

    cp "$REPO_DIR/install.sh" "$update_builder_root/install.sh"
    cp "$REPO_DIR/launcher/start.sh.template" "$update_builder_root/launcher/start.sh.template"
    cp "$REPO_DIR/Cargo.toml" "$update_builder_root/Cargo.toml"
    cp "$REPO_DIR/Cargo.lock" "$update_builder_root/Cargo.lock"
    cp -r "$REPO_DIR/computer-use-linux" "$update_builder_root/computer-use-linux"
    cp -r "$REPO_DIR/updater" "$update_builder_root/updater"
    mkdir -p "$update_builder_root/plugins/openai-bundled/plugins"
    cp -r "$REPO_DIR/plugins/openai-bundled/plugins/computer-use" \
        "$update_builder_root/plugins/openai-bundled/plugins/computer-use"
    cp "$REPO_DIR/scripts/build-deb.sh" "$update_builder_root/scripts/build-deb.sh"
    cp "$REPO_DIR/scripts/build-rpm.sh" "$update_builder_root/scripts/build-rpm.sh"
    cp "$REPO_DIR/scripts/build-pacman.sh" "$update_builder_root/scripts/build-pacman.sh"
    cp "$REPO_DIR/scripts/rebuild-candidate.sh" "$update_builder_root/scripts/rebuild-candidate.sh"
    cp "$REPO_DIR/scripts/patch-linux-window-ui.js" "$update_builder_root/scripts/patch-linux-window-ui.js"
    cp -r "$REPO_DIR/scripts/patches/." "$update_builder_root/scripts/patches/"
    cp "$REPO_DIR/scripts/lib/package-common.sh" "$update_builder_root/scripts/lib/package-common.sh"
    cp "$REPO_DIR/scripts/lib/patch-chrome-plugin.js" "$update_builder_root/scripts/lib/patch-chrome-plugin.js"
    cp "$REPO_DIR/scripts/lib/node-runtime.sh" "$update_builder_root/scripts/lib/node-runtime.sh"
    cp "$REPO_DIR/scripts/lib/install-helpers.sh" "$update_builder_root/scripts/lib/install-helpers.sh"
    cp "$REPO_DIR/scripts/lib/process-detection.sh" "$update_builder_root/scripts/lib/process-detection.sh"
    cp "$REPO_DIR/scripts/lib/dmg.sh" "$update_builder_root/scripts/lib/dmg.sh"
    cp "$REPO_DIR/scripts/lib/native-modules.sh" "$update_builder_root/scripts/lib/native-modules.sh"
    cp "$REPO_DIR/scripts/lib/asar-patch.sh" "$update_builder_root/scripts/lib/asar-patch.sh"
    cp "$REPO_DIR/scripts/lib/webview-install.sh" "$update_builder_root/scripts/lib/webview-install.sh"
    cp "$REPO_DIR/scripts/lib/bundled-plugins.sh" "$update_builder_root/scripts/lib/bundled-plugins.sh"
    cp "$REPO_DIR/scripts/lib/linux-features.js" "$update_builder_root/scripts/lib/linux-features.js"
    cp "$REPO_DIR/scripts/lib/linux-features.sh" "$update_builder_root/scripts/lib/linux-features.sh"
    cp "$REPO_DIR/scripts/lib/linux-update-bridge-patch.js" "$update_builder_root/scripts/lib/linux-update-bridge-patch.js"
    cp "$REPO_DIR/scripts/lib/patch-report.js" "$update_builder_root/scripts/lib/patch-report.js"
    cp "$REPO_DIR/scripts/lib/rebuild-report.sh" "$update_builder_root/scripts/lib/rebuild-report.sh"
    cp "$REPO_DIR/packaging/linux/control" "$update_builder_root/packaging/linux/control"
    cp "$REPO_DIR/packaging/linux/codex-desktop.spec" "$update_builder_root/packaging/linux/codex-desktop.spec"
    cp "$REPO_DIR/packaging/linux/codex-desktop.desktop" "$update_builder_root/packaging/linux/codex-desktop.desktop"
    cp "$REPO_DIR/packaging/linux/codex-packaged-runtime.sh" "$update_builder_root/packaging/linux/codex-packaged-runtime.sh"
    cp "$REPO_DIR/packaging/linux/com.github.ilysenko.codex-desktop-linux.update.policy" \
        "$update_builder_root/packaging/linux/com.github.ilysenko.codex-desktop-linux.update.policy"
    cp "$REPO_DIR/packaging/linux/codex-update-manager-user-service.sh" \
        "$update_builder_root/packaging/linux/codex-update-manager-user-service.sh"
    cp "$REPO_DIR/packaging/linux/PKGBUILD.template" "$update_builder_root/packaging/linux/PKGBUILD.template"
    cp "$REPO_DIR/packaging/linux/codex-desktop.install" "$update_builder_root/packaging/linux/codex-desktop.install"
    cp "$UPDATER_SERVICE_SOURCE" "$update_builder_root/packaging/linux/codex-update-manager.service"
    cp "$REPO_DIR/packaging/linux/codex-update-manager.postinst" "$update_builder_root/packaging/linux/codex-update-manager.postinst"
    cp "$REPO_DIR/packaging/linux/codex-update-manager.prerm" "$update_builder_root/packaging/linux/codex-update-manager.prerm"
    cp -r "$REPO_DIR/linux-features/." "$update_builder_root/linux-features/"
    rm -f "$update_builder_root/linux-features/features.json"
    cp "$REPO_DIR/packaging/linux/codex-update-manager.postrm" "$update_builder_root/packaging/linux/codex-update-manager.postrm"
    cp "$REPO_DIR/assets/codex.png" "$update_builder_root/assets/codex.png"
    if [ -d "$node_runtime_source" ]; then
        cp -a "$node_runtime_source" "$update_builder_root/node-runtime"
    else
        error "Missing managed Node.js runtime: $node_runtime_source. Run ./install.sh first."
    fi
}

write_launcher_stub() {
    local root="$1"

    cat > "$root/usr/bin/$PACKAGE_NAME" <<SCRIPT
#!/bin/bash
exec /opt/$PACKAGE_NAME/start.sh "\$@"
SCRIPT
    chmod 0755 "$root/usr/bin/$PACKAGE_NAME"
}
