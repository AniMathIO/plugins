# Build and load

Requires stable Rust, Python 3.11+ and `rustup target add wasm32-wasip2`.
Run `bash build.sh`. The resulting `target/*.amplugin` is a ZIP containing
`plugin.toml` and `plugin.wasm`; packaging needs no AniMathIO SDK installation.
Copy it into a directory and launch `ANIMATHIO_PLUGIN_DIR=/absolute/path/to/directory animathio`.
The override replaces the bundled search paths, so also copy bundled plugins if needed.
Use the editor Library to add the source, or Effects to apply a frame effect.
Circle exposes radius and color; invert exposes the invert filter.

The vendored `wit/plugin.wit` is ABI 1.1.0 from AniMathIO. Keep it and the manifest
ABI version in sync. See the repository README for compatibility and headless checks.
