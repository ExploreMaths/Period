#!/usr/bin/env bash
# Differential test: every example must produce identical stdout, stderr, and
# exit code on all execution paths:
#   default            — JIT tiers enabled (specialized + generic Cranelift)
#   PERIOD_NO_JIT=1    — bytecode VM only
#   PERIOD_NO_BYTECODE=1 — tree-walking interpreter only
set -euo pipefail

cd "$(dirname "$0")/../../period"

PERIOD="./target/debug/period"
if [[ "$OSTYPE" == "msys" || "$OSTYPE" == "win32" || "$OSTYPE" == "cygwin" ]]; then
  PERIOD="./target/debug/period.exe"
fi

EXAMPLES_DIR="../examples"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

mismatch=0
count=0

run_mode() {
  local file="$1" mode="$2" out="$3"
  local code=0
  case "$mode" in
    default) "$PERIOD" "$file" > "$out" 2>&1 || code=$? ;;
    vm)      PERIOD_NO_JIT=1 "$PERIOD" "$file" > "$out" 2>&1 || code=$? ;;
    tree)    PERIOD_NO_BYTECODE=1 "$PERIOD" "$file" > "$out" 2>&1 || code=$? ;;
  esac
  echo "$code" >> "$out.code"
}

for file in "$EXAMPLES_DIR"/*.period; do
  base="$(basename "$file")"
  count=$((count + 1))
  run_mode "$file" default "$TMP/$base.default"
  run_mode "$file" vm "$TMP/$base.vm"
  run_mode "$file" tree "$TMP/$base.tree"
  ok=yes
  if ! diff -q "$TMP/$base.default" "$TMP/$base.vm" > /dev/null 2>&1 \
     || ! diff -q "$TMP/$base.default.code" "$TMP/$base.vm.code" > /dev/null 2>&1; then
    echo "MISMATCH (default vs VM): $base"
    diff "$TMP/$base.default" "$TMP/$base.vm" | head -10 || true
    ok=no
  fi
  if ! diff -q "$TMP/$base.default" "$TMP/$base.tree" > /dev/null 2>&1 \
     || ! diff -q "$TMP/$base.default.code" "$TMP/$base.tree.code" > /dev/null 2>&1; then
    echo "MISMATCH (default vs tree-walk): $base"
    diff "$TMP/$base.default" "$TMP/$base.tree" | head -10 || true
    ok=no
  fi
  if [[ "$ok" == "yes" ]]; then
    echo "    consistent: $base"
  else
    mismatch=$((mismatch + 1))
  fi
done

echo ""
echo "Examples checked: $count"
echo "Mismatches:       $mismatch"

if (( mismatch > 0 )); then
  exit 1
fi
