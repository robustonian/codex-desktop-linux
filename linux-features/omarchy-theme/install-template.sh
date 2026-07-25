#!/usr/bin/env bash
set -Eeuo pipefail

warn() {
    echo "WARN: omarchy-theme: $*" >&2
}

truthy() {
    case "${1:-}" in
        1|true|TRUE|yes|YES|on|ON) return 0 ;;
        *) return 1 ;;
    esac
}

if [ -z "${HOME:-}" ] || [ -z "${CODEX_LINUX_FEATURES_DIR:-}" ]; then
    warn "HOME or CODEX_LINUX_FEATURES_DIR is unavailable; skipping Omarchy template setup"
    exit 0
fi

source_path="$CODEX_LINUX_FEATURES_DIR/omarchy-theme/codex-desktop.css.tpl"
if [ ! -f "$source_path" ]; then
    warn "template source not found at $source_path"
    exit 0
fi

omarchy_home="$HOME/.config/omarchy"
target_dir="$omarchy_home/themed"
target_path="$target_dir/codex-desktop.css.tpl"
generated_path="$omarchy_home/current/theme/codex-desktop.css"

if [ -f "$target_path" ] && ! cmp -s "$source_path" "$target_path"; then
    warn "$target_path already exists with local changes; leaving it untouched"
elif [ ! -f "$target_path" ]; then
    if ! mkdir -p "$target_dir" || ! install -m 0644 "$source_path" "$target_path"; then
        warn "could not install Omarchy template at $target_path"
        exit 0
    fi
    echo "Installed Omarchy Codex Desktop theme template at $target_path" >&2
fi

if [ -s "$generated_path" ]; then
    exit 0
fi

if ! truthy "${CODEX_OMARCHY_THEME_AUTO_REFRESH:-1}"; then
    warn "theme CSS is not generated yet; run 'omarchy theme refresh'"
    exit 0
fi

if ! command -v omarchy >/dev/null 2>&1; then
    warn "Omarchy CLI not found; run 'omarchy theme refresh' after it is available"
    exit 0
fi

refresh_timeout_seconds="${CODEX_OMARCHY_THEME_REFRESH_TIMEOUT_SECONDS:-15}"
case "$refresh_timeout_seconds" in
    [1-9]|[1-5][0-9]|60) ;;
    *)
        warn "refresh timeout must be a whole number between 1 and 60 seconds; using 15 seconds"
        refresh_timeout_seconds=15
        ;;
esac

if ! command -v timeout >/dev/null 2>&1; then
    warn "timeout command not found; skipping automatic refresh to avoid blocking app launch"
    exit 0
fi

if ! timeout --kill-after=2s "${refresh_timeout_seconds}s" omarchy theme refresh; then
    warn "'omarchy theme refresh' timed out or failed; run it manually to generate the Codex stylesheet"
fi
