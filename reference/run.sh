#!/usr/bin/env bash
# usage: run.sh <unity project> <output dir> [scene path, "probe", "human", "blend", "layering", "sampletime", "rotation", "dense", "crossfade", "generic", "gridhold", "writedefaults" or "curves"] [avatar object name]
# AW_CLIPS=<a recording of the scene's avatar> plays each clip it played alone instead of recording (README).
# UNITY names the editor command (default: Unity on PATH); it needs a display (e.g. Xvfb).
# Recordings on one machine take turns in the order they were asked for; AW_TURNS names the queue's directory.
set -euo pipefail
[ -f "$1/Packages/manifest.json" ] || { echo "the project has no Packages/manifest.json" >&2; exit 2; }
project=$(realpath "$1") out=$(realpath -m "$2") scene=${3:-} avatar=${4:-}
here=$(cd "$(dirname "$0")" && pwd)
if [ -d "$out" ] && [ -n "$(ls -A "$out")" ] && [ ! -e "$out/done" ] && [ ! -e "$out/error.txt" ]; then
  echo "the output dir is not an earlier recording; refusing to replace it" >&2
  exit 2
fi
manifest=$project/Packages/manifest.json
for pkg in com.vrchat.avatars lyuma.av3emulator; do
  [ -d "$project/Packages/$pkg" ] || jq -e --arg p "$pkg" '.dependencies[$p]' "$manifest" >/dev/null ||
    { echo "the project lacks $pkg; install it (e.g. vrc-get install $pkg)" >&2; exit 2; }
done
for pkg in com.avatar-workshop.reference com.avatarworkshop.exporter; do
  [ ! -e "$project/Packages/$pkg" ] || { echo "the project's Packages/$pkg would shadow this checkout's copy; remove it" >&2; exit 2; }
done
turns=${AW_TURNS:-/tmp/avatar-workshop-recordings}
mkdir -p "$turns"
exec 8>"$turns/.lock"
flock 8
last=$(find "$turns" -maxdepth 1 -name '[0-9]*' -printf '%f\n' | sort -n | tail -n 1)
ticket=$(printf '%012d' $((10#${last:-0} + 1)))
exec 9>"$turns/$ticket"
flock 9
exec 8>&-
trap 'rm -f "$turns/$ticket"' EXIT
asked=$(date +%s)
waited=
for t in $(find "$turns" -maxdepth 1 -name '[0-9]*' -printf '%f\n' | sort -n); do
  [ "$t" \< "$ticket" ] || break
  { exec 7<"$turns/$t"; } 2>/dev/null || continue
  until flock -n 7 || { [ -n "$waited" ] && flock -w 60 7; }; do
    ahead=$(find "$turns" -maxdepth 1 -name '[0-9]*' -printf '%f\n' | awk -v m="$ticket" '$0 < m' | wc -l)
    echo "recording waits its turn: $ahead ahead, waited $(($(date +%s) - asked)) s" >&2
    waited=1
  done
  rm -f "$turns/$t"
  exec 7<&-
done
[ -z "$waited" ] || echo "recording's turn came after $(($(date +%s) - asked)) s" >&2
original=$(mktemp)
cp -p "$manifest" "$original"
trap 'cp -p "$original" "$manifest"; rm -f "$original" "$turns/$ticket"' EXIT
jq --arg p "file:$here" --arg e "file:$(realpath "$here/../unity/com.avatarworkshop.exporter")" \
  '.dependencies["com.avatar-workshop.reference"]=$p | .dependencies["com.avatarworkshop.exporter"]=$e | .dependencies["com.unity.test-framework"] //= "1.1.33"' "$original" > "$manifest"
rm -rf "$out"; mkdir -p "$out"
env AW_OUT="$out" AW_SCENE="$scene" AW_AVATAR="$avatar" \
  setpriv --pdeathsig KILL "${UNITY:-Unity}" -batchmode -projectPath "$project" \
  -executeMethod AvatarWorkshop.Reference.Harness.Run -logFile "$out/unity.log" 9>&- &
editor=$!
stop() { kill "$editor" 2>/dev/null; wait "$editor" || true; exit "$1"; }
trap 'stop 129' HUP
trap 'stop 130' INT
trap 'stop 143' TERM
wait "$editor"
test -f "$out/done"
