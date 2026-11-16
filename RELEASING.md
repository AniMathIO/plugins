# Releasing plugins hosted in this repository

Plugins under `community/` are released from this repository; the editor downloads the assets
of these releases (see [docs/registry.md](docs/registry.md)). The plugin must be listed in
[`community-plugins.json`](community-plugins.json) (added by pull request) first.

1. Set `version` in the plugin's `plugin.toml` (and `Cargo.toml`) and merge it to `main`.
2. Tag that commit `<plugin-id>@<version>` and push the tag:

   ```sh
   git checkout main && git pull --ff-only
   git tag io.github.animathio.code-snippets@0.1.0
   git push origin io.github.animathio.code-snippets@0.1.0
   ```

   or, without a local tag, run the workflow against `main`:

   ```sh
   gh workflow run release-plugin.yml --repo AniMathIO/plugins --ref main -f path=community/code-snippets
   ```

3. `release-plugin.yml` checks that the tag is on `main`, that `plugin.toml` has this id and
   version, builds the plugin with its `build.sh`, loads and renders it with the released
   AniMathIO CLI, and creates the release with `<id>-<version>.amplugin` and
   `<id>-<version>.amplugin.sha256`. It then calls `index.yml`, which commits the refreshed
   `community-index.json` to `main`.
4. Check that the run is green, that
   <https://raw.githubusercontent.com/AniMathIO/plugins/main/community-index.json> lists the new
   version, and that `sha256sum -c` passes on the downloaded assets.

Versions are immutable: if a release is wrong, publish the next patch version. To withdraw a bad
release, `gh release delete <tag> --cleanup-tag --yes` and run the *Community index* workflow; the
previous release becomes the latest again (or the plugin leaves the index if there is none).

## Dry run

```sh
scripts/dry-run-release.sh community/code-snippets path/to/animathio-cli
```

builds the plugin, writes the assets, smoke-tests them and generates a `community-index.json`
from the local assets exactly as the index workflow does from the release (output in
`target/dry-run/`, which is git-ignored).

## First release: Code Snippets 0.1.0

Plugin id `io.github.animathio.code-snippets` (it was `org.animathio.community.code-snippets`
before: ids under `org.animathio.*` are reserved and the host would not load them from the user
plugin folder). After the registry pull request is merged:

```sh
git checkout main && git pull --ff-only
git tag io.github.animathio.code-snippets@0.1.0
git push origin io.github.animathio.code-snippets@0.1.0
```
