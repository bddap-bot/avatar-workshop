#!/usr/bin/env bash
set -euo pipefail
run=$(cd "$(dirname "$0")/../reference" && pwd)/run.sh
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
export AW_TURNS=$work/turns UNITY=$work/unity
mkdir -p "$work/project/Packages/com.vrchat.avatars" "$work/project/Packages/lyuma.av3emulator"
echo '{"dependencies":{}}' > "$work/project/Packages/manifest.json"
cat > "$UNITY" <<'EOF'
#!/usr/bin/env bash
name=$(basename "$AW_OUT")
echo $$ > "$AW_OUT/pid"
echo "start $name" >> "$AW_LOG"
read -rt "${AW_FAKE_SECONDS:-2}" <> "$AW_IDLE" || true
echo "end $name" >> "$AW_LOG"
touch "$AW_OUT/done"
EOF
chmod +x "$UNITY"
export AW_LOG=$work/log AW_IDLE=$work/idle
mkfifo "$AW_IDLE"
fail() { echo "FAIL: $*" >&2; tail -n +1 "$AW_LOG" "$work"/*.err >&2; ls "$AW_TURNS" >&2; exit 1; }
tickets() { find "$AW_TURNS" -maxdepth 1 -name '[0-9]*' 2>/dev/null | wc -l; }
until_tickets() { local _; for _ in $(seq 200); do [ "$(tickets)" -ge "$1" ] && return; sleep 0.05; done; fail "no ticket $1"; }
until_line() { local _; for _ in $(seq 200); do grep -qx "$1" "$AW_LOG" 2>/dev/null && return; sleep 0.05; done; fail "no line '$1'"; }
record() { "$run" "$work/project" "$work/out/$1" 2> "$work/$1.err"; }
expect() { [ "$(cat "$AW_LOG")" = "$(printf '%s\n' "$@")" ] || fail "order"; : > "$AW_LOG"; }

: > "$AW_LOG"
record a & pa=$!
until_tickets 1; record b & pb=$!
until_tickets 2; record c & pc=$!
wait "$pa" "$pb" "$pc"
expect "start a" "end a" "start b" "end b" "start c" "end c"
grep -q '^recording waits its turn: 1 ahead' "$work/b.err" || fail "b did not say it waits"
grep -q '^recording waits its turn: 2 ahead' "$work/c.err" || fail "c did not say it waits"
grep -q "^recording's turn came after" "$work/c.err" || fail "c did not say its turn came"
[ ! -s "$work/a.err" ] || fail "a waited"
echo "asked a, b, c together: ran a, b, c; c said: $(head -n 1 "$work/c.err")"

{ record x1; record x2; } & px=$!
until_line "start x1"; record y & py=$!
wait "$px" "$py"
expect "start x1" "end x1" "start y" "end y" "start x2" "end x2"
echo "x1 then x2 back to back, y asked during x1: ran x1, y, x2"

for sig in TERM KILL; do
  AW_FAKE_SECONDS=600 "$run" "$work/project" "$work/out/$sig" 2> "$work/$sig.err" & ps=$!
  until_line "start $sig"
  record t & pt=$!
  until_tickets 2
  kill -"$sig" "$ps"
  wait "$ps" || true
  wait "$pt"
  expect "start $sig" "start t" "end t"
  [ ! -e "/proc/$(cat "$work/out/$sig/pid")" ] || fail "the editor outlived its recording after $sig"
  echo "$sig sent to a recording's script alone: its editor stopped and the next ran"
done
echo "recording turns: ok"
