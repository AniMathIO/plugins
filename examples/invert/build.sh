#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
export CARGO_BUILD_JOBS=3 CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0
runner=(cargo)
if [[ "$OSTYPE" == linux* ]]; then runner=(nice -n 19 ionice -c3 cargo); fi
"${runner[@]}" build --target wasm32-wasip2 --release --locked
python3 - <<'PACKAGE'
import pathlib, tomllib, zipfile
root = pathlib.Path('.')
name = tomllib.loads((root / 'Cargo.toml').read_text())['package']['name']
out = root / 'target' / (name + '.amplugin')
with zipfile.ZipFile(out, 'w', zipfile.ZIP_DEFLATED) as z:
    z.write('plugin.toml', 'plugin.toml')
    z.write(f"target/wasm32-wasip2/release/{name.replace('-', '_')}.wasm", 'plugin.wasm')
print(out)
PACKAGE
