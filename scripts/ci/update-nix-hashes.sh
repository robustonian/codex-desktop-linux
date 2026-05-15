#!/usr/bin/env bash
set -euo pipefail

REPO_DIR="${REPO_DIR:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
FLAKE_FILE="${FLAKE_FILE:-$REPO_DIR/flake.nix}"
UPSTREAM_DMG_URL="${UPSTREAM_DMG_URL:-https://persistent.oaistatic.com/codex-app-prod/Codex.dmg}"
UPSTREAM_DMG_PATH="${UPSTREAM_DMG_PATH:-/tmp/Codex.dmg}"
BUILD_LOG="${BUILD_LOG:-/tmp/codex-nix-build.log}"
VERIFY_LOG="${VERIFY_LOG:-/tmp/codex-nix-build-verify.log}"

validate_sri_hash() {
    local hash="$1"
    [[ "$hash" =~ ^sha256-[A-Za-z0-9+/=]{44}$ ]]
}

replace_flake_hash() {
    local anchor="$1"
    local key="$2"
    local new_hash="$3"

    python3 - "$FLAKE_FILE" "$anchor" "$key" "$new_hash" <<'PY'
from pathlib import Path
import re
import sys

path = Path(sys.argv[1])
anchor = sys.argv[2]
key = sys.argv[3]
new_hash = sys.argv[4]

lines = path.read_text().splitlines(keepends=True)
in_block = False
for index, line in enumerate(lines):
    if anchor in line:
        in_block = True
        continue
    if not in_block:
        continue
    if key in line:
        lines[index] = re.sub(r'sha256-[^"]+', new_hash, line, count=1)
        path.write_text("".join(lines))
        raise SystemExit(0)
    if line.strip() == "};":
        break

raise SystemExit(f"Could not find {key!r} after {anchor!r} in {path}")
PY
}

read_flake_hash() {
    local anchor="$1"
    local key="$2"

    python3 - "$FLAKE_FILE" "$anchor" "$key" <<'PY'
from pathlib import Path
import re
import sys

path = Path(sys.argv[1])
anchor = sys.argv[2]
key = sys.argv[3]

in_block = False
for line in path.read_text().splitlines():
    if anchor in line:
        in_block = True
        continue
    if not in_block:
        continue
    if key in line:
        match = re.search(r'sha256-[^"]+', line)
        if match:
            print(match.group(0))
            raise SystemExit(0)
    if line.strip() == "};":
        break

raise SystemExit(f"Could not find {key!r} after {anchor!r} in {path}")
PY
}

extract_got_sri_hash() {
    local log_path="$1"

    python3 - "$log_path" <<'PY'
from pathlib import Path
import re
import sys

text = Path(sys.argv[1]).read_text(errors="replace")
text = re.sub(r"\x1b\[[0-9;]*m", "", text)
matches = re.findall(r"got:\s*(sha256-[A-Za-z0-9+/=]{44})", text)
if not matches:
    raise SystemExit(1)
print(matches[-1])
PY
}

run_nix_build() {
    local log_path="$1"
    rm -f "$log_path"
    set +e
    nix build .#codex-desktop --no-link --print-build-logs 2>&1 | tee "$log_path"
    local status="${PIPESTATUS[0]}"
    set -e
    return "$status"
}

main() {
    mkdir -p "$(dirname "$UPSTREAM_DMG_PATH")"
    curl -fL --retry 3 -o "$UPSTREAM_DMG_PATH" "$UPSTREAM_DMG_URL"

    new_dmg_hash="$(nix hash file --sri --type sha256 "$UPSTREAM_DMG_PATH")"
    if ! validate_sri_hash "$new_dmg_hash"; then
        echo "Refusing to proceed: computed DMG hash '$new_dmg_hash' is not a valid SRI sha256." >&2
        exit 1
    fi

    current_dmg_hash="$(read_flake_hash "codexDmg = pkgs.fetchurl {" "hash = ")"
    echo "Current Codex.dmg hash:  $current_dmg_hash"
    echo "Upstream Codex.dmg hash: $new_dmg_hash"
    replace_flake_hash "codexDmg = pkgs.fetchurl {" "hash = " "$new_dmg_hash"

    # Seed the Nix store so the build can reuse the DMG that was already downloaded
    # for hashing instead of fetching the same 300MB artifact again.
    nix-store --add-fixed sha256 "$UPSTREAM_DMG_PATH" >/dev/null

    if run_nix_build "$BUILD_LOG"; then
        echo "Nix build succeeded with the current payload outputHash."
        exit 0
    fi

    new_payload_hash="$(extract_got_sri_hash "$BUILD_LOG" || true)"
    if [ -z "$new_payload_hash" ]; then
        echo "Nix build failed without a fixed-output hash mismatch; leaving log at $BUILD_LOG" >&2
        exit 1
    fi

    if ! validate_sri_hash "$new_payload_hash"; then
        echo "Refusing to proceed: extracted payload hash '$new_payload_hash' is not a valid SRI sha256." >&2
        exit 1
    fi

    current_payload_hash="$(read_flake_hash "codexDesktopPayload = pkgs.stdenv.mkDerivation {" "outputHash = ")"
    echo "Current payload outputHash: $current_payload_hash"
    echo "Actual payload outputHash:  $new_payload_hash"
    replace_flake_hash "codexDesktopPayload = pkgs.stdenv.mkDerivation {" "outputHash = " "$new_payload_hash"

    run_nix_build "$VERIFY_LOG"
    echo "Nix build succeeded after refreshing the payload outputHash."
}

case "${1:-}" in
    read-flake-hash)
        if [ "$#" -ne 3 ]; then
            echo "usage: $0 read-flake-hash <anchor> <key>" >&2
            exit 2
        fi
        read_flake_hash "$2" "$3"
        ;;
    "")
        main
        ;;
    *)
        echo "unknown command: $1" >&2
        exit 2
        ;;
esac
