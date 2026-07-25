#!/usr/bin/env bash
set -Eeo pipefail

if [ -z "${CODEX_ELECTRON_DISABLE_GPU_COMPOSITING+x}" ]; then
    printf '%s\n' 'env CODEX_ELECTRON_DISABLE_GPU_COMPOSITING=0'
fi
