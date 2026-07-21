#!/bin/bash
set -Eeuo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
APP_LAUNCHER="$REPO_DIR/codex-app/start.sh"
INSTALL_DIR="${CODEX_DESKTOP_USER_BIN_DIR:-${CODEX_INSTALL_DIR:-$HOME/.local/bin}}"
WRAPPER="$INSTALL_DIR/codex-desktop"
FORCE=0

usage() {
    cat <<'EOF'
Usage: bash scripts/install-user-launcher.sh [--force]

Installs a user-local codex-desktop launcher that runs this checkout's
codex-app/start.sh. This is useful for testing a rebuilt app without replacing
the system package under /opt/codex-desktop.

Environment:
  CODEX_DESKTOP_USER_BIN_DIR=/path/to/bin   Override install dir
EOF
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        -h|--help)
            usage
            exit 0
            ;;
        --force)
            FORCE=1
            ;;
        *)
            echo "Unknown argument: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
    shift
done

[ -x "$APP_LAUNCHER" ] || {
    echo "Missing local Codex Desktop launcher: $APP_LAUNCHER" >&2
    echo "Run make build-app or bash scripts/install-latest.sh before installing the user launcher." >&2
    exit 1
}

mkdir -p "$INSTALL_DIR"

if [ -e "$WRAPPER" ] && [ "$FORCE" -ne 1 ]; then
    if ! grep -qF "Managed by codex-desktop-linux scripts/install-user-launcher.sh" "$WRAPPER" 2>/dev/null; then
        echo "Refusing to overwrite existing launcher: $WRAPPER" >&2
        echo "Re-run with --force if you want to replace it." >&2
        exit 1
    fi
fi

tmp="$(mktemp "$INSTALL_DIR/.codex-desktop.XXXXXX")"
cleanup() {
    rm -f "$tmp"
}
trap cleanup EXIT

cat > "$tmp" <<EOF
#!/usr/bin/env bash
# Managed by codex-desktop-linux scripts/install-user-launcher.sh
set -euo pipefail

TARGET_LAUNCHER=$(printf '%q' "$APP_LAUNCHER")
TARGET_DIR=$(printf '%q' "$REPO_DIR/codex-app")

truthy() {
    case "\${1:-}" in
        1|true|TRUE|yes|YES|on|ON) return 0 ;;
        *) return 1 ;;
    esac
}

launch_args_allow_existing_app() {
    if truthy "\${CODEX_MULTI_LAUNCH:-}"; then
        return 0
    fi

    local arg
    for arg in "\$@"; do
        case "\$arg" in
            -h|--help|--diagnose-scaling|--new-instance|--multi-instance|--multi-launch)
                return 0
                ;;
        esac
    done

    return 1
}

current_user_pid() {
    local pid="\$1"
    local uid

    [[ "\$pid" =~ ^[0-9]+\$ ]] || return 1
    [ -r "/proc/\$pid/status" ] || return 1
    uid="\$(awk '/^Uid:/ {print \$2; exit}' "/proc/\$pid/status" 2>/dev/null || true)"
    [ "\$uid" = "\$(id -u)" ]
}

foreign_codex_desktop_pid() {
    local app_id="\${CODEX_LINUX_APP_ID:-\${CODEX_APP_ID:-codex-desktop}}"
    local proc_cmdline pid cmdline arg0

    for proc_cmdline in /proc/[0-9]*/cmdline; do
        [ -r "\$proc_cmdline" ] || continue
        pid="\${proc_cmdline#/proc/}"
        pid="\${pid%/cmdline}"
        current_user_pid "\$pid" || continue
        cmdline="\$(tr '\\0' ' ' < "\$proc_cmdline" 2>/dev/null || true)"
        [[ "\$cmdline" == *"--app-id=\$app_id"* ]] || continue
        [[ "\$cmdline" != *" --type="* ]] || continue
        arg0="\$(tr '\\0' '\\n' < "\$proc_cmdline" 2>/dev/null | sed -n '1p')"
        case "\$arg0" in
            "\$TARGET_DIR/electron"|"\$TARGET_DIR/electron "*) continue ;;
            */electron|*/electron\ *) ;;
            *) continue ;;
        esac
        printf '%s\\n' "\$pid"
        return 0
    done

    return 1
}

if ! launch_args_allow_existing_app "\$@"; then
    if pid="\$(foreign_codex_desktop_pid)"; then
        install_dir="\$(dirname "\$(tr '\\0' '\\n' < "/proc/\$pid/cmdline" 2>/dev/null | sed -n '1p')" 2>/dev/null || echo unknown)"
        cat >&2 <<MSG
Codex Desktop is already running from \$install_dir (pid \$pid).
This user launcher points at \$TARGET_DIR.

Fully quit the running Codex Desktop first, then run:
  codex-desktop --profile desktop_fugu

Use --new-instance only if you explicitly want a side-by-side test instance.
MSG
        exit 1
    fi
fi

exec "\$TARGET_LAUNCHER" "\$@"
EOF
chmod 0755 "$tmp"
mv "$tmp" "$WRAPPER"
trap - EXIT

path_status="not-on-path"
seen_system=0
IFS=:
for entry in $PATH; do
    case "$entry" in
        "$INSTALL_DIR")
            if [ "$seen_system" -eq 0 ]; then
                path_status="before-system"
            else
                path_status="after-system"
            fi
            break
            ;;
        /usr/bin|/bin)
            seen_system=1
            ;;
    esac
done
unset IFS

echo "Installed user launcher: $WRAPPER"
echo "Target launcher: $APP_LAUNCHER"
case "$path_status" in
    before-system)
        echo "$INSTALL_DIR is before /usr/bin on PATH."
        ;;
    after-system)
        echo "WARN: $INSTALL_DIR is on PATH, but after /usr/bin. Move it earlier to override the system package." >&2
        ;;
    *)
        echo "WARN: $INSTALL_DIR is not on PATH. Add it before /usr/bin to override the system package." >&2
        ;;
esac
echo "Run 'hash -r' in existing shells, then 'type -a codex-desktop' to confirm the first entry is $WRAPPER."
