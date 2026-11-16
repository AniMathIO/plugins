# AniMathIO plugins

A cargo-generate template, working examples and community index for AniMathIO 2.
MIT licensed, like AniMathIO. Plugins are sandboxed WebAssembly components.

## Contents

- [Community plugins](#community-plugins)
- [Installing a plugin](#installing-a-plugin)
- [Quick start: write your own](#quick-start)
- [Test in AniMathIO](#test-in-animathio)
- [ABI compatibility](#abi-compatibility)

## Community plugins

| Name | Description | Source | License | CI | ABI |
| --- | --- | --- | --- | --- | --- |
| [Code Snippets](community/code-snippets) | Syntax-highlighted code with reveal and token morph animations | [Source](https://github.com/AniMathIO/plugins/tree/main/community/code-snippets) | MIT; font OFL-1.1 | [Plugins CI](https://github.com/AniMathIO/plugins/actions/workflows/ci.yml) | 1.1.0 |

Submit a PR adding a row under the rules in
[CONTRIBUTING.md](CONTRIBUTING.md). A listing is not an endorsement or security audit.

## Installing a plugin

Plugins are `.amplugin` files. AniMathIO loads every bundle in your user plugin folder at
startup; restart the editor after adding or removing one:

| OS | User plugin folder |
| --- | --- |
| Linux | `~/.local/share/animathio/plugins` |
| macOS | `~/Library/Application Support/animathio/plugins` |
| Windows | `%APPDATA%\animathio\plugins` |

Community plugins are never bundled or enabled by AniMathIO itself: you install the ones
you want. They run in the same sandbox as first-party plugins (no network or file access,
memory and time limits), and they cannot replace the bundled `org.animathio.*` plugins.
Build a plugin from its folder with `bash build.sh` (see each plugin's README) and copy
the resulting `.amplugin` into the folder above.

## Quick start

Install stable Rust, Python 3.11+ and cargo-generate, then:

```sh
cargo install cargo-generate --locked
rustup target add wasm32-wasip2
cargo generate gh:AniMathIO/plugins template
cd <generated-project>
bash build.sh
```

Choose a unique reverse-domain ID (for example `org.example.my-circle`), display
name and author when prompted. Change the ID default before publishing.
The template draws a circle with keyframeable radius and color using cubic paths.
`examples/circle` shows that source; `examples/invert` implements a frame effect
that inverts RGB and preserves alpha. Each is standalone: run its `build.sh`.

## Test in AniMathIO

Copy your `.amplugin` into the user plugin folder (see [Installing a plugin](#installing-a-plugin))
and restart the editor, then add a source from the Library or apply invert through Effects.
For quick iteration you can instead start the editor with
`ANIMATHIO_PLUGIN_DIR=/absolute/path/to/plugins animathio`; that override is searched
first, so include the bundled plugins in that directory if your project uses them.

For a headless host check, install Pillow (`python3 -m pip install Pillow`), download the Linux deb from the
[v2.0.0 release](https://github.com/AniMathIO/AniMathIO/releases/tag/v2.0.0),
extract with `dpkg-deb -x`, and run `python3 scripts/smoke.py PATH/TO/animathio-cli
PATH/TO/PLUGIN_DIRECTORY`. CI builds the template and both examples, packages them,
and renders each source and the effect using that released CLI. No audio or GUI is used.

## ABI compatibility

The vendored `wit/plugin.wit` in each project is copied unchanged from
[AniMathIO v2.0.0](https://github.com/AniMathIO/AniMathIO/blob/v2.0.0/crates/plugin-api/wit/plugin.wit).
The vendored WIT is ABI **1.1.0**, with `wit-bindgen = 0.40.0`, independently of
AniMathIO's application version. Frozen ABI 1.0.0 components still load; changing
manifest text alone does not upgrade a component. Preserve parameter IDs and
saved-value semantics. Follow the canonical
[authoring and compatibility guide](https://github.com/AniMathIO/AniMathIO/blob/main/docs/plugins.md)
for WIT worlds, host imports, schema, packaging and sandbox limits.
The host provides bounded services; plugins have no direct network or filesystem access.
