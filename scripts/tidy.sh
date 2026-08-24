#!/usr/bin/env sh
# Reclaim disk from build artifacts while keeping incremental state.
#
#   scripts/tidy.sh          # drop dx serve's parallel artifact trees
#   scripts/tidy.sh --deep   # full cargo clean (~2 min rebuild afterwards)
#
# Steady-state sizes on Linux x86_64 (see RULES.md):
#   after cargo build + test : ~1.4G  (accepted project budget)
#   after dx serve           : +~2G   (dx keeps its own trees) -> run tidy
set -e
cd "$(dirname "$0")/.."
rm -rf target/x86_64-unknown-linux-gnu target/desktop-dev target/dx target/tmp
if [ "${1:-}" = "--deep" ]; then
    cargo clean
fi
du -sh target
