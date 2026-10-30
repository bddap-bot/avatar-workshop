#!/usr/bin/env bash
# usage: run.sh <unity project> <output dir> [scene path] [avatar object name]
# UNITY names the editor command (default: Unity on PATH); it needs a display (e.g. Xvfb).
set -euo pipefail
project=$(realpath "$1") out=$(realpath -m "$2") scene=${3:-} avatar=${4:-}
here=$(cd "$(dirname "$0")" && pwd)
if [ -d "$out" ] && [ -n "$(ls -A "$out")" ] && [ ! -e "$out/done" ] && [ ! -e "$out/error.txt" ]; then
  echo "$out is not an earlier recording; refusing to replace it" >&2
  exit 2
fi
manifest=$project/Packages/manifest.json
for pkg in com.vrchat.avatars lyuma.av3emulator; do
  [ -d "$project/Packages/$pkg" ] || jq -e --arg p "$pkg" '.dependencies[$p]' "$manifest" >/dev/null ||
    { echo "$project lacks $pkg; install it (e.g. vrc-get install $pkg)" >&2; exit 2; }
done
original=$(mktemp)
cp -p "$manifest" "$original"
trap 'cp -p "$original" "$manifest"; rm -f "$original"' EXIT
jq --arg p "file:$here" '.dependencies["com.avatar-workshop.reference"]=$p | .dependencies["com.unity.test-framework"] //= "1.1.33"' "$original" > "$manifest"
rm -rf "$out"; mkdir -p "$out"
env AW_OUT="$out" AW_SCENE="$scene" AW_AVATAR="$avatar" \
  "${UNITY:-Unity}" -batchmode -projectPath "$project" \
  -executeMethod AvatarWorkshop.Reference.Harness.Run -logFile "$out/unity.log"
test -f "$out/done"
