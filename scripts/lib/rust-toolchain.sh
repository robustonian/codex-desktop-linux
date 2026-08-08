#!/bin/bash

MIN_RUST_VERSION="${MIN_RUST_VERSION:-1.89.0}"

rustc_version() {
    local version

    command -v rustc >/dev/null 2>&1 || return 1
    version="$(rustc --version 2>/dev/null || true)"
    version="${version#rustc }"
    version="${version%% *}"
    version="${version%%-*}"
    [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || return 1
    printf '%s\n' "$version"
}

rust_version_at_least() {
    local actual="$1"
    local required="$2"
    local actual_major actual_minor actual_patch
    local required_major required_minor required_patch

    IFS=. read -r actual_major actual_minor actual_patch <<< "$actual"
    IFS=. read -r required_major required_minor required_patch <<< "$required"

    (( actual_major > required_major )) && return 0
    (( actual_major < required_major )) && return 1
    (( actual_minor > required_minor )) && return 0
    (( actual_minor < required_minor )) && return 1
    (( actual_patch >= required_patch ))
}

rust_toolchain_compatible() {
    local version

    command -v cargo >/dev/null 2>&1 || return 1
    version="$(rustc_version 2>/dev/null || true)"
    [ -n "$version" ] && rust_version_at_least "$version" "$MIN_RUST_VERSION"
}
