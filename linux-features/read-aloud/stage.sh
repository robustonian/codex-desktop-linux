#!/bin/bash
set -Eeuo pipefail

feature_dir="$(cd "$(dirname "$0")" && pwd)"
target_dir="$INSTALL_DIR/resources/read-aloud"

mkdir -p "$target_dir"
cp "$feature_dir/bin/kokoro-stdin" "$target_dir/kokoro-stdin"
cp "$feature_dir/bin/kokoro_stdin.py" "$target_dir/kokoro_stdin.py"
chmod 0755 "$target_dir/kokoro-stdin"
chmod 0644 "$target_dir/kokoro_stdin.py"

echo "Read aloud Kokoro runner staged" >&2
