#!/usr/bin/env bash
# Verifies the C ABI contract the Kotlin/widget shims compile against:
#   - every function declared in widgets/kal_ffi.h is exported by the built
#     kal-ffi shared library (a rename/removal fails the pipeline, not the app);
#   - the header's KAL_BRIDGE_VERSION macro matches the compiled constant.
#
# This is the C-side half of the guide §13 signature-compatibility gate; the
# Kotlin-declaration half is added with the Kotlin project (phase 1).
#
#   scripts/check-bridge-abi.sh                  # builds target/debug/libkal_ffi.so
#   scripts/check-bridge-abi.sh path/to/lib.so   # check an existing artifact
set -euo pipefail
cd "$(dirname "$0")/.."

LIB="${1:-target/debug/libkal_ffi.so}"
HEADER="widgets/kal_ffi.h"
SOURCE="crates/kal-ffi/src/lib.rs"

if [ ! -f "$HEADER" ]; then
  echo "missing $HEADER" >&2
  exit 1
fi

# Build the artifact under test unless the caller passed one explicitly: a
# stale library would make this gate report on code that no longer exists.
if [ $# -eq 0 ]; then
  echo "building kal-ffi for $(uname -s) ..."
  cargo build -p kal-ffi
fi
if [ ! -f "$LIB" ]; then
  echo "kal-ffi shared library not found at $LIB" >&2
  exit 1
fi

NM="${NM:-nm}"
if ! command -v "$NM" >/dev/null; then
  echo "nm not found; set NM to a compatible tool" >&2
  exit 1
fi

# Declared functions: `kal_<name>(` occurrences in the header.
declared="$(grep -oE 'kal_[a-z_]+' "$HEADER" | sort -u)"
if [ -z "$declared" ]; then
  echo "no kal_* declarations found in $HEADER" >&2
  exit 1
fi

exports="$("$NM" -D --defined-only "$LIB" 2>/dev/null || "$NM" --defined-only "$LIB")"

status=0
count=0
for sym in $declared; do
  # Only check names that are actual declarations (followed by '(').
  if ! grep -qE "(^|[^a-z_])${sym}\(" "$HEADER"; then
    continue
  fi
  count=$((count + 1))
  if ! printf '%s\n' "$exports" | grep -qE "[[:space:]]T[[:space:]]+${sym}$"; then
    echo "missing export: $sym (declared in $HEADER)" >&2
    status=1
  fi
done

# Version macro must match the compiled constant.
hdr_ver="$(grep -oE '^#define[[:space:]]+KAL_BRIDGE_VERSION[[:space:]]+[0-9]+' "$HEADER" | grep -oE '[0-9]+$')"
src_ver="$(grep -oE 'KAL_BRIDGE_VERSION: u32 = [0-9]+' "$SOURCE" | grep -oE '[0-9]+$')"
if [ -z "$hdr_ver" ] || [ -z "$src_ver" ]; then
  echo "could not read the bridge version from $HEADER / $SOURCE" >&2
  status=1
elif [ "$hdr_ver" != "$src_ver" ]; then
  echo "bridge version drift: header=$hdr_ver code=$src_ver" >&2
  status=1
fi

if [ "$status" -eq 0 ]; then
  echo "kal-ffi ABI OK: $count declared symbols present, bridge version $src_ver"
fi
exit "$status"
