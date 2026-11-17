#!/usr/bin/env bash
# Local dry run of .github/workflows/release-plugin.yml + index.yml: no tag, no upload.
# Usage: scripts/dry-run-release.sh PLUGIN_DIR PATH/TO/animathio-cli [OUT_DIR]
#   scripts/dry-run-release.sh community/code-snippets .host/cli/squashfs-root/usr/bin/animathio-cli
# Builds the plugin, writes the release assets (<id>-<version>.amplugin + .sha256), loads and renders
# it with the CLI, then generates community-index.json from those assets exactly as the index
# workflow would from the GitHub release, and validates it against schemas/community-index.schema.json.
set -euo pipefail
cd "$(dirname "$0")/.."
plugin=${1:?plugin directory}
cli=${2:?path to animathio-cli}
out=${3:-target/dry-run}
rm -rf "$out"; mkdir -p "$out"
eval "$(python3 scripts/registry.py resolve --path "$plugin" | sed 's/^/export REG_/')"
echo "== would tag $REG_tag"
python3 scripts/registry.py package "$plugin" --out "$out/assets"
python3 scripts/registry_smoke.py "$cli" "$out/assets" --only "$REG_id"
python3 scripts/registry.py index --assets-dir "$out/assets" --previous /dev/null --strict --output "$out/community-index.json"
(cd "$out/assets" && sha256sum -c ./*.sha256)
echo "== $out/community-index.json"
cat "$out/community-index.json"
