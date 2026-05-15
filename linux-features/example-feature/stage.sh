#!/bin/bash
set -Eeuo pipefail

if [ -n "${CODEX_EXAMPLE_FEATURE_STAGE_MARKER:-}" ]; then
    printf 'example-stage:%s:%s\n' "${ARCH:-unknown}" "${INSTALL_DIR:-unknown}" > "$CODEX_EXAMPLE_FEATURE_STAGE_MARKER"
fi

echo "Example Linux feature stage hook: no-op" >&2
