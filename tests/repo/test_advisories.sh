#!/usr/bin/env bash
# Advisory hygiene, read from deny.toml and Cargo.lock only (offline, no cargo-deny).
#   1. the [advisories] ignore list is exactly the expected set
#   2. every ignored ID has a comment directly above it naming its dependency path
#   3. Cargo.lock has no rustls-pemfile package
#   4. Cargo.lock's time package is >= 0.3.47
#   5. crates/common declares rust-version = "1.88"
#   7. every hickory-proto and hickory-net package in Cargo.lock is >= 0.26.1
#   6. the comment check can fail: it is run over a fixture with one uncommented ID
#
# Phase 2 tightens EXPECTED_IGNORES to the three hickory/paste-free remainder;
# edit only that variable (ADLC-Test-Change).
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

EXPECTED_IGNORES="RUSTSEC-2026-0206 RUSTSEC-2026-0192 RUSTSEC-2024-0436"

fails=0
fail() { echo "FAIL: $1"; fails=1; }

# parse_ignores FILE: prints "ID<TAB>comment text" per ignore entry. The comment is
# the run of '#' lines directly above the ID (a blank line or a non-ID line cuts
# it off); IDs listed back to back share the comment above the first of them.
parse_ignores() {
  awk '
    /^\[/ { inadv = ($0 == "[advisories]"); inign = 0; next }
    inadv && /^ignore[ \t]*=[ \t]*\[/ { inign = 1; buf = ""; next }
    inign && /^\][ \t]*$/ { inign = 0; next }
    inign && /^[ \t]*#/ { sub(/^[ \t]*#[ \t]*/, ""); buf = buf " " $0; prev = "c"; next }
    inign && /^[ \t]*$/ { buf = ""; next }
    inign && match($0, /"RUSTSEC-[0-9]+-[0-9]+"/) {
      id = substr($0, RSTART + 1, RLENGTH - 2)
      printf "%s\t%s\n", id, buf
    }
  ' "$1"
}

# check_ignores FILE EXPECTED: fails (returns 1) on an empty parse, a set mismatch,
# or an ID whose comment lacks the keywords for its dependency path.
check_ignores() {
  local file=$1 expected=$2 rc=0 parsed ids n id comment
  parsed=$(parse_ignores "$file")
  n=$(printf '%s' "$parsed" | grep -c . )
  echo "read $file: $n ignore entries"
  [ "$n" -gt 0 ] || { echo "FAIL: $file: ignore list parsed to zero IDs"; return 1; }
  ids=$(printf '%s\n' "$parsed" | cut -f1 | sort | tr '\n' ' ')
  want=$(printf '%s\n' $expected | sort | tr '\n' ' ')
  if [ "$ids" != "$want" ]; then
    echo "FAIL: $file: ignore list is [${ids% }], expected [${want% }]"; rc=1
  fi
  while IFS=$'\t' read -r id comment; do
    [ -n "$id" ] || continue
    if [ -z "${comment// /}" ]; then
      echo "FAIL: $file: $id has no comment naming its dependency path"; rc=1; continue
    fi
    # keywords per ID, matched case-insensitively against the comment text
    case $id in
      RUSTSEC-2026-0206|RUSTSEC-2026-0192)
        echo "$comment" | grep -qiE 'resvg|usvg' && echo "$comment" | grep -qi 'label' \
          || { echo "FAIL: $file: $id comment must mention resvg/usvg and label"; rc=1; } ;;
      RUSTSEC-2024-0436)
        echo "$comment" | grep -qi 'utoipa' \
          || { echo "FAIL: $file: $id comment must mention utoipa"; rc=1; } ;;
      RUSTSEC-2026-0118|RUSTSEC-2026-0119)
        echo "$comment" | grep -qF '0.26.1' && echo "$comment" | grep -qi 'mhost' \
          || { echo "FAIL: $file: $id comment must mention 0.26.1 and mhost"; rc=1; } ;;
    esac
  done <<< "$parsed"
  return $rc
}

# lock_version FILE NAME: version of the first package NAME in a Cargo.lock.
lock_version() {
  awk -v n="$2" '$0 == "name = \"" n "\"" { f = 1; next }
    f && /^version = / { gsub(/[" ]|version=/, "", $0); sub(/^version=/, ""); print; exit }' "$1" |
    sed 's/^version = //; s/"//g'
}

check_lock() {
  local lock=$1 pkgs tv
  pkgs=$(grep -c '^name = ' "$lock")
  echo "read $lock: $pkgs packages"
  [ "$pkgs" -gt 0 ] || { echo "FAIL: $lock: no packages parsed"; return 1; }
  local rc=0
  if grep -q '^name = "rustls-pemfile"$' "$lock"; then
    echo "FAIL: $lock still has the rustls-pemfile package"; rc=1
  fi
  tv=$(lock_version "$lock" time)
  echo "time version in $lock: ${tv:-none}"
  if [ -z "$tv" ]; then
    echo "FAIL: $lock has no time package"; rc=1
  elif [ "$(printf '%s\n0.3.47\n' "$tv" | sort -t. -k1,1n -k2,2n -k3,3n | head -1)" != "0.3.47" ]; then
    echo "FAIL: $lock: time $tv is older than 0.3.47"; rc=1
  fi
  local hv name nh=0
  for name in hickory-proto hickory-net; do
    while read -r hv; do
      [ -n "$hv" ] || continue
      nh=$((nh + 1))
      if [ "$(printf '%s\n0.26.1\n' "$hv" | sort -t. -k1,1n -k2,2n -k3,3n | head -1)" != "0.26.1" ]; then
        echo "FAIL: $lock: $name $hv is older than 0.26.1"; rc=1
      fi
    done < <(awk -v n="$name" '$0 == "name = \"" n "\"" { f = 1; next }
      f && /^version = / { v = $3; gsub(/"/, "", v); print v; f = 0 }' "$lock")
  done
  echo "hickory packages in $lock: $nh"
  [ "$nh" -gt 0 ] || { echo "FAIL: $lock has no hickory-proto or hickory-net package"; rc=1; }
  return $rc
}

# The checks run on the real files.
check_ignores deny.toml "$EXPECTED_IGNORES" || fails=1
check_lock Cargo.lock || fails=1
grep -qE '^rust-version[ \t]*=[ \t]*"1\.88"' crates/common/Cargo.toml \
  || fail "crates/common/Cargo.toml does not declare rust-version = \"1.88\""

# C10: the same check over a fixture with one uncommented ID must report it.
fixture=tests/repo/fixtures/advisories/deny-missing-comment.toml
out=$(check_ignores "$fixture" "$EXPECTED_IGNORES" 2>&1); frc=$?
if [ "$frc" -eq 0 ] || ! echo "$out" | grep -q 'RUSTSEC-2024-0436 has no comment'; then
  fail "the comment check did not report the uncommented ID in $fixture"
  echo "$out"
fi

[ "$fails" -eq 0 ] || exit 1
echo "PASS: advisories ignore list, lockfile and rust-version"
