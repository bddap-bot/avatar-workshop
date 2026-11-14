#!/usr/bin/env bash
# usage: check.sh <unity project> <output dir> [scene path] [avatar object name] [--names]
# Without --names, differing outputs are listed by file and a differing pose by number.
set -euo pipefail
names=() quiet=(-q) args=()
for a in "$@"; do if [ "$a" = --names ]; then names=(--names) quiet=(); else args+=("$a"); fi; done
set -- "${args[@]}"
here=$(cd "$(dirname "$0")" && pwd)
out=$(realpath -m "$2")
"$here/run.sh" "$1" "$out/a" "${@:3}"
"$here/run.sh" "$1" "$out/b" "${@:3}"
(cd "$out" && diff -r ${quiet[@]+"${quiet[@]}"} --exclude=unity.log --exclude=export.txt --exclude=tips.tsv --exclude=poses.tsv a b)
if cmp -s "$out/a/tips.tsv" "$out/b/tips.tsv"; then tips=byte-identical; else tips="not byte-identical"; fi
paste "$out/a/tips.tsv" "$out/b/tips.tsv" | awk -F'\t' -v tips="$tips" '
  $1 != $6 || $2 != $7 { print "tip rows differ at line " NR; bad = 1; exit }
  { d = ($3-$8)^2 + ($4-$9)^2 + ($5-$10)^2; if (d > max) max = d }
  END { if (bad) exit 1; printf "outputs identical except tips (%s): %d tip samples, max tip difference %.3g m\n", tips, NR, sqrt(max); exit !(sqrt(max) <= 1e-5) }'
if cmp -s "$out/a/poses.tsv" "$out/b/poses.tsv"; then poses=byte-identical; else poses="not byte-identical"; fi
python3 "$here/poses.py" "$out/a/poses.tsv" "$out/b/poses.tsv" "$poses" ${names[@]+"${names[@]}"}
