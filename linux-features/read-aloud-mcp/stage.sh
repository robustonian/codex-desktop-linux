#!/bin/bash
set -Eeuo pipefail

plugin_template="$SCRIPT_DIR/plugins/openai-bundled/plugins/read-aloud"
runner_source="$SCRIPT_DIR/linux-features/read-aloud/bin"
installer_source="$SCRIPT_DIR/linux-features/read-aloud/install-kokoro-runtime.sh"
target_plugin="$INSTALL_DIR/resources/plugins/openai-bundled/plugins/read-aloud"
target_marketplace="$INSTALL_DIR/resources/plugins/openai-bundled/.agents/plugins/marketplace.json"

find_cargo_for_read_aloud_mcp() {
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

build_read_aloud_mcp_backend() {
    local source_binary="$SCRIPT_DIR/target/release/codex-read-aloud-linux"
    local cargo_cmd=""

    if [ -n "${CODEX_LINUX_READ_ALOUD_MCP_SOURCE:-}" ]; then
        [ -x "$CODEX_LINUX_READ_ALOUD_MCP_SOURCE" ] || {
            echo "Read Aloud MCP source is not executable: $CODEX_LINUX_READ_ALOUD_MCP_SOURCE" >&2
            return 1
        }
        echo "Using prebuilt Read Aloud MCP backend" >&2
        printf '%s\n' "$CODEX_LINUX_READ_ALOUD_MCP_SOURCE"
        return 0
    fi

    if ! cargo_cmd="$(find_cargo_for_read_aloud_mcp)"; then
        echo "cargo not found; Read Aloud MCP plugin will be unavailable" >&2
        return 1
    fi

    echo "Building Read Aloud MCP backend..." >&2
    if ! (cd "$SCRIPT_DIR" && "$cargo_cmd" build --release -p codex-read-aloud-linux >&2); then
        echo "Failed to build Read Aloud MCP backend" >&2
        return 1
    fi

    [ -x "$source_binary" ] || {
        echo "Read Aloud MCP backend missing after build: $source_binary" >&2
        return 1
    }
    printf '%s\n' "$source_binary"
}

write_read_aloud_marketplace_entry() {
    local marketplace="$1"
    node - "$marketplace" <<'NODE'
const fs = require("node:fs");
const path = require("node:path");

const marketplacePath = process.argv[2];
let marketplace = { plugins: [] };
try {
  marketplace = JSON.parse(fs.readFileSync(marketplacePath, "utf8"));
} catch (_error) {
  marketplace = { plugins: [] };
}
if (!Array.isArray(marketplace.plugins)) {
  marketplace.plugins = [];
}
marketplace.plugins = marketplace.plugins.filter((plugin) => plugin?.name !== "read-aloud");
marketplace.plugins.push({
  name: "read-aloud",
  source: {
    source: "local",
    path: "./plugins/read-aloud",
  },
  policy: {
    installation: "AVAILABLE",
    authentication: "ON_INSTALL",
  },
  category: "Productivity",
});
fs.mkdirSync(path.dirname(marketplacePath), { recursive: true });
fs.writeFileSync(marketplacePath, `${JSON.stringify(marketplace, null, 2)}\n`);
NODE
}

[ -d "$plugin_template" ] || {
    echo "Read Aloud plugin template not found at $plugin_template" >&2
    exit 1
}
[ -f "$runner_source/kokoro-stdin" ] || {
    echo "Read Aloud Kokoro runner not found at $runner_source/kokoro-stdin" >&2
    exit 1
}
[ -f "$runner_source/kokoro_stdin.py" ] || {
    echo "Read Aloud Kokoro Python runner not found at $runner_source/kokoro_stdin.py" >&2
    exit 1
}

backend_binary="$(build_read_aloud_mcp_backend)"

rm -rf "$target_plugin"
mkdir -p "$target_plugin"
cp -R "$plugin_template/." "$target_plugin/"
mkdir -p "$target_plugin/bin"
cp "$backend_binary" "$target_plugin/bin/codex-read-aloud-linux"
cp "$runner_source/kokoro-stdin" "$target_plugin/bin/kokoro-stdin"
cp "$runner_source/kokoro_stdin.py" "$target_plugin/bin/kokoro_stdin.py"
if [ -f "$installer_source" ]; then
    cp "$installer_source" "$target_plugin/bin/install-kokoro-runtime.sh"
    chmod 0755 "$target_plugin/bin/install-kokoro-runtime.sh"
fi
chmod 0755 "$target_plugin/bin/codex-read-aloud-linux" "$target_plugin/bin/kokoro-stdin"
chmod 0644 "$target_plugin/bin/kokoro_stdin.py"

if [ -f "${ICON_SOURCE:-}" ]; then
    mkdir -p "$target_plugin/assets"
    cp "$ICON_SOURCE" "$target_plugin/assets/app-icon.png"
fi

find "$target_plugin" \( -name '*:com.apple.*' -o -name '.gitkeep' \) -delete
write_read_aloud_marketplace_entry "$target_marketplace"

echo "Read Aloud MCP plugin staged" >&2
