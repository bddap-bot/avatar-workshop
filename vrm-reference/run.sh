#!/usr/bin/env bash
# usage: vrm-reference/run.sh AVATAR.vrm > AVATAR.springs.tsv
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
[ -d "$here/node_modules" ] || npm ci --prefix "$here" --ignore-scripts --no-audit --no-fund >&2
node "$here/springs.mjs" "$1"
