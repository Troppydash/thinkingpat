#!/usr/bin/env bash

set -euo pipefail

VERSION="${1:-}"
if [ -z "$VERSION" ]; then
    echo "Use ./build.sh <version>"
    exit 1
fi

ver_gt() {
    # Returns 0 (true) if $1 > $2
    [[ "$1" != "$2" ]] && [[ "$(printf '%s\n' "$1" "$2" | sort -V | tail -n1)" == "$1" ]]
}

FROZEN=$(cat ./generate/ver.txt)
if ! ver_gt $VERSION $FROZEN; then
    echo "Version $VERSION not greater than frozen $FROZEN version"
    exit 1
fi

cargo build -r
mkdir -p "./builds/$VERSION"
cp ./target/release/thinkingpat "./builds/$VERSION/thinkingpat"