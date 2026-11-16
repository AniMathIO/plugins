# Contributing

## Listing a community plugin

Community plugins are listed in [`community-plugins.json`](community-plugins.json) and show up in
the editor's Plugins tab (AniMathIO 2.2+). To list yours, open a pull request that adds one entry
(the format and the generated index are described in [docs/registry.md](docs/registry.md)):

```json
{
  "id": "com.example.spiral",
  "name": "Spiral",
  "author": "Ada Example",
  "description": "A keyframeable spiral clip source",
  "repo": "ada/animathio-spiral",
  "repo_id": 123456789,
  "license": "MIT",
  "min_abi": "1.1.0"
}
```

then run `python3 scripts/registry.py table --write` to refresh the README table (CI checks it).
Add `"path": "<subdirectory>"` if the plugin lives in a subdirectory of a repository with
several plugins. A plugin developed in this repository goes under `community/<name>` and uses
`"repo": "AniMathIO/plugins"` with that `path`.

### Rules

* **Id**: a unique reverse-domain id you control (`com.example.spiral`, `io.github.<user>.<name>`),
  lowercase, never `org.animathio.*` (reserved for first-party plugins; the host will not load
  such an id from the user plugin folder). It must equal `id` in `plugin.toml`. Ids are never reused.
* **Repository identity**: `repo_id` is the numeric GitHub id of your repository
  (`gh api repos/<owner>/<name> --jq .id`). New releases are indexed automatically, so if the
  repository is renamed, transferred or re-created the index stops serving it until a maintainer
  re-reviews the entry (open a PR updating `repo`/`repo_id`).
* **Source and license**: public GitHub repository with a recognizable `LICENSE`. The `license`
  field is an SPDX expression whose identifiers are all OSI-approved (or SIL OFL for bundled
  fonts and assets) and include the license GitHub detects in your repository. Keep the notices
  for anything you bundle inside the `.amplugin` (like `community/code-snippets`).
* **Build**: use the plugin template layout (`cargo generate gh:AniMathIO/plugins template`): a
  `build.sh` that builds for `wasm32-wasip2` and packages `target/<name>.amplugin` with
  `plugin.toml` and `plugin.wasm`.
* **Release before you submit** (plugins outside this repository): publish a GitHub release
  tagged `<id>@<version>`, or `v<version>`/`<version>` when the repository has a single plugin and
  no `path`, carrying exactly two assets, `<id>-<version>.amplugin` and
  `<id>-<version>.amplugin.sha256` (`sha256sum` format). `plugin.toml` must have the same `id` and
  `version`, and `api_version` must equal `min_abi`. Recipe for your repository
  (`.github/workflows/release.yml`, triggered by pushing a tag such as `v0.1.0`):

  ```yaml
  on:
    push:
      tags: ["v*"]
  permissions:
    contents: write
  jobs:
    release:
      runs-on: ubuntu-latest
      env:
        GH_TOKEN: ${{ github.token }}
      steps:
        - uses: actions/checkout@v4
        - run: rustup target add wasm32-wasip2 && bash build.sh
        - run: |
            id=$(python3 -c "import tomllib; print(tomllib.load(open('plugin.toml','rb'))['id'])")
            version=$(python3 -c "import tomllib; print(tomllib.load(open('plugin.toml','rb'))['version'])")
            cp target/*.amplugin "$id-$version.amplugin"
            sha256sum "$id-$version.amplugin" > "$id-$version.amplugin.sha256"
            gh release create "$GITHUB_REF_NAME" "$id-$version.amplugin" "$id-$version.amplugin.sha256" --generate-notes
  ```

  Pin actions by digest if you prefer; releases are immutable, so ship fixes as a new version.
  New versions need no pull request: the index picks up the latest release within hours.
* **Behaviour**: document parameters and the host services you use. No network access or
  telemetry beyond what the documented sandbox permits (plugins cannot open sockets or files).
  A source plugin must draw something with its default parameters.
* **Honest listing**: do not imply that a listing is a security audit or endorsement.

### What CI checks

`.github/workflows/registry.yml` validates the JSON against the schema, that ids are unique, not
reserved and not delisted, that the repository exists and is public, the license, that the README
table is in sync, that the latest release resolves (assets, SHA-256 and manifest all agree), that
the source builds for `wasm32-wasip2` with its `build.sh` and produces the same id, version and
ABI, and that the released package loads and renders with the released AniMathIO CLI. Run the
same checks locally with `python3 scripts/registry.py validate` and
`python3 scripts/registry_smoke.py PATH/TO/animathio-cli DIR_WITH_AMPLUGIN`.

For a pull request that adds or changes your entry, CI runs your repository's `build.sh` at the
release tag (so `build.rs` and dependencies execute on a GitHub runner). That step gets no token
or secrets and the workflow is read-only; pushes to `main` never build third-party source.

### Review

A maintainer reviews every listing before merge: the description, the license, whether the plugin
does what it says and uses the sandbox as documented. Review is not a security audit and a
merged listing is not an endorsement. Updates (new versions) do not need review; changes to a
listing's metadata do.

### Delisting

A maintainer can delist a plugin (malicious or deceptive behaviour, an unresponsive broken
release, licence problems, or at the author's request) by moving its entry to `delisted` with
a reason in `community-plugins.json`. The next index run removes it from Browse; users who installed it
get a notice with the reason in the editor. The id stays reserved. Authors can ask for delisting
by opening an issue or a pull request.

## Template, examples and the vendored ABI

Template/example changes must pass CI including generated-template builds and released-host smoke
renders. Keep the vendored ABI unchanged unless deliberately upgrading the contract and
documenting compatibility. Use Conventional Commits.
