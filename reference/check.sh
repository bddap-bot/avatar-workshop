#!/usr/bin/env bash
# usage: check.sh <unity project> <output dir>
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
out=$(realpath -m "$2")
"$here/run.sh" "$1" "$out/a"
"$here/run.sh" "$1" "$out/b"
diff -r --exclude=unity.log --exclude=tips.tsv "$out/a" "$out/b"
if cmp -s "$out/a/tips.tsv" "$out/b/tips.tsv"; then tips=byte-identical; else tips="not byte-identical"; fi
paste "$out/a/tips.tsv" "$out/b/tips.tsv" | awk -F'\t' -v tips="$tips" '
  $1 != $6 || $2 != $7 { print "tip rows differ at line " NR; bad = 1; exit }
  { d = ($3-$8)^2 + ($4-$9)^2 + ($5-$10)^2; if (d > max) max = d }
  END { if (bad) exit 1; printf "outputs identical except tips (%s): %d tip samples, max tip difference %.3g m\n", tips, NR, sqrt(max); exit !(sqrt(max) <= 1e-5) }'
