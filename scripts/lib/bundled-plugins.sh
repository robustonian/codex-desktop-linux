#!/bin/bash
# Bundled-plugin staging — Linux Computer Use backend build, plugin manifest, marketplace.
#
# Sourced by install.sh. Do not run directly.
# shellcheck shell=bash

# ---- Install Linux-safe bundled plugin resources ----
find_cargo_for_linux_computer_use() {
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

build_linux_computer_use_backend() {
    local crate_dir="$SCRIPT_DIR/computer-use-linux"
    local backend_binary="$SCRIPT_DIR/target/release/codex-computer-use-linux"
    local cargo_cmd=""

    if [ ! -d "$crate_dir" ]; then
        warn "Linux Computer Use backend source not found at $crate_dir"
        return 1
    fi

    if ! cargo_cmd="$(find_cargo_for_linux_computer_use)"; then
        warn "cargo not found; Linux Computer Use plugin will be unavailable"
        return 1
    fi

    info "Building Linux Computer Use backend..."
    if ! (cd "$SCRIPT_DIR" && "$cargo_cmd" build --release -p codex-computer-use-linux >&2); then
        warn "Failed to build Linux Computer Use backend"
        return 1
    fi

    [ -x "$backend_binary" ] || {
        warn "Linux Computer Use backend binary missing after build: $backend_binary"
        return 1
    }

    echo "$backend_binary"
}

stage_linux_computer_use_plugin() {
    local target_plugins="$1"
    local plugin_template="$SCRIPT_DIR/plugins/openai-bundled/plugins/computer-use"
    local backend_binary=""
    local target_plugin="$target_plugins/computer-use"

    if [ ! -d "$plugin_template" ]; then
        warn "Linux Computer Use plugin template not found at $plugin_template"
        return 1
    fi

    if ! backend_binary="$(build_linux_computer_use_backend)"; then
        return 1
    fi

    rm -rf "$target_plugin"
    mkdir -p "$target_plugin"
    cp -R "$plugin_template/." "$target_plugin/"
    mkdir -p "$target_plugin/bin"
    cp "$backend_binary" "$target_plugin/bin/codex-computer-use-linux"
    chmod 0755 "$target_plugin/bin/codex-computer-use-linux"

    if [ -f "$ICON_SOURCE" ]; then
        mkdir -p "$target_plugin/assets"
        cp "$ICON_SOURCE" "$target_plugin/assets/app-icon.png"
    fi

    find "$target_plugin" \( -name '*:com.apple.*' -o -name '.gitkeep' \) -delete
    return 0
}

is_host_linux_elf_executable() {
    local file="$1"
    python3 - "$file" "$ARCH" <<'PY'
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
arch = sys.argv[2]
expected_machine = {
    "x86_64": 62,
    "aarch64": 183,
    "armv7l": 40,
    "armv6l": 40,
    "armhf": 40,
}.get(arch)
if expected_machine is None:
    sys.exit(1)

try:
    header = path.read_bytes()[:20]
except OSError:
    sys.exit(1)

if len(header) < 20 or header[:4] != b"\x7fELF":
    sys.exit(1)

is_little_endian = header[5] == 1
if not is_little_endian:
    sys.exit(1)

machine = int.from_bytes(header[18:20], "little")
sys.exit(0 if machine == expected_machine else 1)
PY
}

install_linux_executable_resource() {
    local source="$1"
    local destination="$2"
    local label="$3"

    if [ ! -f "$source" ]; then
        warn "Browser Use $label not found in upstream resources; skipping"
        return 1
    fi

    if ! is_host_linux_elf_executable "$source"; then
        warn "Browser Use $label is not a Linux executable for $ARCH; skipping"
        return 1
    fi

    install -m 0755 "$source" "$destination"
}

browser_use_node_repl_runtime_url() {
    case "$ARCH" in
        x86_64)
            echo "${CODEX_BROWSER_USE_NODE_REPL_RUNTIME_URL:-https://persistent.oaistatic.com/codex-primary-runtime/26.426.12240/codex-primary-runtime-linux-x64-26.426.12240.tar.xz}"
            ;;
        *)
            return 1
            ;;
    esac
}

browser_use_node_repl_runtime_sha256() {
    case "$ARCH" in
        x86_64)
            echo "${CODEX_BROWSER_USE_NODE_REPL_RUNTIME_SHA256:-db5624eb6efa36b66ec6f6dd0488cefb966e49636862aab6209a4336c1ca90c4}"
            ;;
        *)
            return 1
            ;;
    esac
}

install_node_repl_from_primary_runtime_archive() {
    local destination="$1"
    local url
    local expected_sha
    local cache_dir
    local archive
    local extract_dir
    local source

    if ! url="$(browser_use_node_repl_runtime_url)"; then
        warn "Browser Use node_repl primary-runtime fallback is unavailable for $ARCH"
        return 1
    fi
    expected_sha="$(browser_use_node_repl_runtime_sha256)"

    cache_dir="${CODEX_BROWSER_USE_RUNTIME_CACHE_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/codex-desktop/browser-use}"
    archive="$cache_dir/$(basename "$url")"
    extract_dir="$WORK_DIR/browser-use-node-repl-runtime"
    source="$extract_dir/codex-primary-runtime/dependencies/bin/node_repl"

    mkdir -p "$cache_dir" "$extract_dir"
    if [ ! -f "$archive" ]; then
        info "Downloading Browser Use node_repl fallback runtime..."
        if ! curl -L --fail --progress-bar -o "$archive.part" "$url"; then
            rm -f "$archive.part"
            warn "Failed to download Browser Use node_repl fallback runtime"
            return 1
        fi
        mv "$archive.part" "$archive"
    else
        info "Using cached Browser Use node_repl fallback runtime: $archive"
    fi

    if ! printf '%s  %s\n' "$expected_sha" "$archive" | sha256sum -c - >/dev/null 2>&1; then
        rm -f "$archive"
        warn "Browser Use node_repl fallback runtime checksum mismatch; removed cached archive"
        return 1
    fi

    if ! tar -xJf "$archive" -C "$extract_dir" codex-primary-runtime/dependencies/bin/node_repl; then
        warn "Failed to extract Browser Use node_repl from fallback runtime"
        return 1
    fi

    install_linux_executable_resource "$source" "$destination" "node_repl fallback runtime"
}

install_browser_use_node_repl_resource() {
    local upstream_source="$1"
    local destination="$2"
    local source

    for source in \
        "${CODEX_LINUX_NODE_REPL_SOURCE:-}" \
        "${CODEX_NODE_REPL_PATH:-}" \
        "${XDG_CACHE_HOME:-$HOME/.cache}/codex-runtimes/codex-primary-runtime/dependencies/bin/node_repl" \
        "$upstream_source"
    do
        [ -n "$source" ] || continue
        if install_linux_executable_resource "$source" "$destination" "node_repl runtime"; then
            return 0
        fi
    done

    install_node_repl_from_primary_runtime_archive "$destination"
}

remove_macos_sidecar_files() {
    local root="$1"
    find "$root" -type f -name '*:com.apple.*' -delete
}

patch_browser_use_site_status_allowlist_fallback() {
    local client="$1"

    if grep -q "codexLinuxSiteStatusAllowlistFallback" "$client"; then
        return 0
    fi

    python3 - "$client" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
source = path.read_text(encoding="utf-8")
needle = (
    'async fetchBlocked(t){let n=await MT(t.endpoint,{method:"GET"});'
    'if(!n.ok)throw new Error(Rt(`Browser Use cannot determine if ${t.displayUrl} is allowed. '
    'Please try again later or use another source.`));let r=await n.json();return R7(r)}'
)
replacement = (
    'async fetchBlocked(t){let n;try{n=await MT(t.endpoint,{method:"GET"})}catch(r){'
    'if(String(t?.endpoint??"").includes("/aura/site_status")&&String(r?.message??r).includes("URL is not allowlisted"))return console.warn'
    '("codexLinuxSiteStatusAllowlistFallback",t.endpoint),!1;throw r}'
    'if(!n.ok)throw new Error(Rt(`Browser Use cannot determine if ${t.displayUrl} is allowed. '
    'Please try again later or use another source.`));let r=await n.json();return R7(r)}'
)
if needle not in source:
    print(
        "WARN: Could not find Browser Use site_status allowlist fallback insertion point — leaving browser-client.mjs unchanged",
        file=sys.stderr,
    )
    raise SystemExit(0)
path.write_text(source.replace(needle, replacement, 1), encoding="utf-8")
PY
}

stage_browser_use_plugin_from_upstream() {
    local source_plugin="$1"
    local target_plugins="$2"
    local target_plugin="$target_plugins/browser-use"
    local source_client="$source_plugin/scripts/browser-client.mjs"
    local target_client="$target_plugin/scripts/browser-client.mjs"

    if [ ! -d "$source_plugin" ]; then
        warn "Browser Use bundled plugin resources not found in upstream app; skipping Browser Use"
        return 1
    fi

    if [ ! -f "$source_plugin/.codex-plugin/plugin.json" ]; then
        warn "Browser Use plugin manifest not found in upstream app; skipping Browser Use"
        return 1
    fi

    if [ ! -f "$source_client" ]; then
        warn "Browser Use browser-client.mjs not found in upstream app; skipping Browser Use"
        return 1
    fi

    rm -rf "$target_plugin"
    cp -R "$source_plugin" "$target_plugin"
    remove_macos_sidecar_files "$target_plugin"
    patch_browser_use_site_status_allowlist_fallback "$target_client"

    info "Browser Use plugin staged from upstream DMG"
    return 0
}

write_bundled_plugins_marketplace() {
    local source="$1"
    local destination="$2"
    local include_browser="$3"
    local include_computer_use="$4"

    node - "$source" "$destination" "$include_browser" "$include_computer_use" <<'NODE'
const fs = require("fs");
const path = require("path");

const sourcePath = process.argv[2];
const destinationPath = process.argv[3];
const includeBrowser = process.argv[4] === "1";
const includeComputerUse = process.argv[5] === "1";
const marketplace = JSON.parse(fs.readFileSync(sourcePath, "utf8"));
const sourcePlugins = marketplace.plugins || [];
const plugins = [];

if (includeBrowser) {
  const browserUse = sourcePlugins.find((plugin) => plugin.name === "browser-use");
  if (browserUse == null) {
    throw new Error("Bundled marketplace does not contain browser-use plugin");
  }
  plugins.push(browserUse);
}

if (includeComputerUse) {
  plugins.push({
    name: "computer-use",
    source: {
      source: "local",
      path: "./plugins/computer-use",
    },
    policy: {
      installation: "AVAILABLE",
      authentication: "ON_INSTALL",
    },
    category: "Productivity",
  });
}

marketplace.plugins = plugins;
fs.mkdirSync(path.dirname(destinationPath), { recursive: true });
fs.writeFileSync(destinationPath, `${JSON.stringify(marketplace, null, 2)}\n`);
NODE
}

install_bundled_plugin_resources() {
    local app_dir="$1"
    local upstream_resources="$app_dir/Contents/Resources"
    local source_marketplace="$upstream_resources/plugins/openai-bundled/.agents/plugins/marketplace.json"
    local source_plugin="$upstream_resources/plugins/openai-bundled/plugins/browser-use"
    local resources_dir="$INSTALL_DIR/resources"
    local bundled_plugins_dir="$resources_dir/plugins/openai-bundled"
    local include_browser=0
    local include_computer_use=0

    if [ ! -f "$source_marketplace" ]; then
        warn "Bundled plugin marketplace not found in upstream app; skipping bundled plugins"
        return 0
    fi

    mkdir -p "$bundled_plugins_dir/plugins" "$bundled_plugins_dir/.agents/plugins"

    if stage_browser_use_plugin_from_upstream "$source_plugin" "$bundled_plugins_dir/plugins"; then
        include_browser=1
    fi

    if stage_linux_computer_use_plugin "$bundled_plugins_dir/plugins"; then
        include_computer_use=1
    else
        warn "Linux Computer Use plugin will be unavailable"
    fi

    if [ "$include_browser" -eq 0 ] && [ "$include_computer_use" -eq 0 ]; then
        warn "No Linux-safe bundled plugins were staged"
        return 0
    fi

    write_bundled_plugins_marketplace "$source_marketplace" "$bundled_plugins_dir/.agents/plugins/marketplace.json" "$include_browser" "$include_computer_use"

    install_linux_executable_resource "$upstream_resources/node" "$resources_dir/node" "node runtime" || true
    install_browser_use_node_repl_resource "$upstream_resources/node_repl" "$resources_dir/node_repl" || true

    info "Linux-safe bundled plugins installed"
}
