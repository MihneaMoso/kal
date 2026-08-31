#!/usr/bin/env bash
# Build the Kal Dioxus wasm web bundle and assemble a GitHub Pages-ready
# deploy directory. Used by BOTH the pages workflow (every master push) and the
# release workflow (every version tag), so there is a single source of truth for
# producing /kal/app/.
#
#   scripts/build-web.sh            # assembles ./_deploy (site at root, app at /app)
#
# Requires: `dx` CLI on PATH and the wasm32-unknown-unknown target installed.
# Outputs the deploy directory to "$1" (default: ./_deploy):
#   site/ index.html...  -> served at /kal/      (landing page)
#   app/  index.html...  -> served at /kal/app/  (Dioxus wasm app)
set -euo pipefail
cd "$(dirname "$0")/.."

DEPLOY="${1:-_deploy}"
rm -rf "$DEPLOY" web/app
mkdir -p "$DEPLOY/app"

# 1. Build the app for the web. base_path = "kal/app" (app/Dioxus.toml) makes
#    the generated index.html reference /kal/app/*. --release gives a smaller,
#    faster wasm. dx writes the bundle under web/app/public/.
dx bundle --platform web --release -p kal-app --out-dir web/app

# 2. Flatten the dx output so the app files sit directly in web/app/
#    (web/app/index.html, web/app/wasm/...).
shopt -s dotglob
mv web/app/public/* web/app/
rmdir web/app/public
shopt -u dotglob

# 3. Assemble the deploy tree. web/site is the landing page (relative URLs,
#    works at /kal/); web/app is the app (base_path "kal/app", works at
#    /kal/app/). .nojekyll stops GH Pages from running Jekyll on the wasm files.
cp -rT web/site "$DEPLOY"
cp -rT web/app "$DEPLOY/app"
touch "$DEPLOY/.nojekyll"

echo "Web bundle assembled at $DEPLOY:"
find "$DEPLOY" -maxdepth 2 -type f | sort
