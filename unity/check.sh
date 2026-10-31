#!/usr/bin/env bash
# usage: check.sh <unity project> <output dir> [scene path] [avatar object name]
# Exports the avatar with its census, records the export's wall time and peak RSS, and compares the export with
# what Unity holds (avatar-core/examples/census.rs). UNITY names the editor command (default: Unity on PATH); it
# needs a display (e.g. Xvfb), GNU time and cargo on PATH.
set -euo pipefail
project=$(realpath "$1") out=$(realpath -m "$2") scene=${3:-} avatar=${4:-}
here=$(cd "$(dirname "$0")" && pwd)
manifest=$project/Packages/manifest.json
original=$(mktemp)
cp -p "$manifest" "$original"
trap 'cp -p "$original" "$manifest"; rm -f "$original"' EXIT
jq --arg p "file:$here/com.avatarworkshop.exporter" '.dependencies["com.avatarworkshop.exporter"]=$p' "$original" > "$manifest"
rm -rf "$out"; mkdir -p "$out"
args=(-batchmode -projectPath "$project" -executeMethod AvatarWorkshop.Exporter.Batch.Run -awOut "$out/avatar.glb" -awCensus "$out/census.json" -logFile "$out/unity.log")
[ -n "$scene" ] && args+=(-awScene "$scene")
[ -n "$avatar" ] && args+=(-awAvatar "$avatar")
command time -f '%e s wall, %M KiB peak RSS' -o "$out/measure.txt" "${UNITY:-Unity}" "${args[@]}"
grep -a '^\[aw\]' "$out/unity.log" > "$out/export.txt"
cat "$out/measure.txt"
cargo run -q --release --manifest-path "$here/../avatar-core/Cargo.toml" --example census -- \
  "$out/avatar.glb" "$out/census.json" | tee "$out/census.tsv"
