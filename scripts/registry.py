#!/usr/bin/env python3
"""Community plugin registry tooling (stdlib only; `validate` and `index` use jsonschema).

  registry.py validate [--offline]            check community-plugins.json (submission CI)
  registry.py package PLUGIN_DIR --out DIR    build a hosted plugin and write the release assets
  registry.py resolve (--tag TAG | --path DIR)  map a release tag / plugin dir to registry data
  registry.py index [--output FILE] ...       resolve every plugin's latest release into
                                              community-index.json
  registry.py table [--check | --write]       keep the README table in sync

Format and rules: docs/registry.md.
"""
from __future__ import annotations

import argparse
import datetime
import hashlib
import io
import json
import os
import re
import subprocess
import sys
import tomllib
import urllib.error
import urllib.request
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
REGISTRY = ROOT / "community-plugins.json"
INDEX = ROOT / "community-index.json"
README = ROOT / "README.md"
HOSTED_REPO = "AniMathIO/plugins"
RESERVED_PREFIX = "org.animathio."
SEMVER = re.compile(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)")
KINDS = {"clip-source", "effect", "audio-effect", "animation-preset", "importer"}
MAX_PLUGIN_BYTES = 64 * 1024 * 1024
MAX_MANIFEST_BYTES = 64 * 1024
TABLE_START = "<!-- community-plugins:start -->"
TABLE_END = "<!-- community-plugins:end -->"


class RegistryError(Exception):
    pass


class TransientError(RegistryError):
    """GitHub could not be reached or is rate limiting: says nothing about the plugin itself."""


def load_json(path: Path):
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def semver_key(version: str):
    return tuple(int(part) for part in version.split("."))


def abi_at_most(a: str, b: str) -> bool:
    return semver_key(a) <= semver_key(b)


def annotate(level: str, message: str):
    # GitHub Actions turns these into annotations; elsewhere they are plain lines.
    print(f"::{level}::{message}", file=sys.stderr)


# --- licences ---------------------------------------------------------------------------

def approved_licenses() -> set[str]:
    lines = (ROOT / "schemas" / "osi-approved-licenses.txt").read_text().splitlines()
    return {line.strip() for line in lines if line.strip() and not line.startswith("#")}


def license_ids(expression: str) -> list[str]:
    """License identifiers of an SPDX expression (operators, parentheses and exceptions dropped)."""
    ids, skip_next = [], False
    for token in re.findall(r"[()]|[A-Za-z0-9.+-]+", expression):
        if token in "()":
            continue
        if skip_next:  # exception id after WITH
            skip_next = False
            continue
        if token in ("AND", "OR"):
            continue
        if token == "WITH":
            skip_next = True
            continue
        ids.append(token)
    return ids


# --- manifests and packages -------------------------------------------------------------

def read_manifest_text(text: str, where: str) -> dict:
    try:
        manifest = tomllib.loads(text)
    except tomllib.TOMLDecodeError as e:
        raise RegistryError(f"{where}: plugin.toml is not valid TOML: {e}")
    for key in ("api_version", "id", "name", "version", "kinds"):
        if key not in manifest:
            raise RegistryError(f"{where}: plugin.toml lacks `{key}`")
    if not SEMVER.fullmatch(str(manifest["version"])):
        raise RegistryError(f"{where}: version {manifest['version']!r} must be MAJOR.MINOR.PATCH")
    if not SEMVER.fullmatch(str(manifest["api_version"])):
        raise RegistryError(f"{where}: api_version {manifest['api_version']!r} must be MAJOR.MINOR.PATCH")
    kinds = manifest["kinds"]
    if not isinstance(kinds, list) or not kinds or not set(kinds) <= KINDS:
        raise RegistryError(f"{where}: kinds {kinds!r} must be a non-empty subset of {sorted(KINDS)}")
    return manifest


def inspect_package(data: bytes, where: str) -> dict:
    """Manifest of an .amplugin held in memory; checks the archive has the required members."""
    try:
        with zipfile.ZipFile(io.BytesIO(data)) as z:
            names = set(z.namelist())
            for required in ("plugin.toml", "plugin.wasm"):
                if required not in names:
                    raise RegistryError(f"{where}: archive lacks {required}")
            info = z.getinfo("plugin.toml")
            if info.file_size > MAX_MANIFEST_BYTES:
                raise RegistryError(f"{where}: plugin.toml is larger than {MAX_MANIFEST_BYTES} bytes")
            return read_manifest_text(z.read("plugin.toml").decode("utf-8"), where)
    except zipfile.BadZipFile:
        raise RegistryError(f"{where}: not a zip archive")


def sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def package_plugin(plugin_dir: Path, out: Path, build: bool = True) -> tuple[Path, Path]:
    manifest = read_manifest_text((plugin_dir / "plugin.toml").read_text(), str(plugin_dir))
    if build:
        subprocess.run(["bash", "build.sh"], cwd=plugin_dir, check=True)
    built = sorted((plugin_dir / "target").glob("*.amplugin"), key=lambda p: p.stat().st_mtime)
    if not built:
        raise RegistryError(f"{plugin_dir}: build.sh produced no target/*.amplugin")
    data = built[-1].read_bytes()
    packaged = inspect_package(data, str(built[-1]))
    for key in ("id", "version", "api_version"):
        if packaged[key] != manifest[key]:
            raise RegistryError(f"{built[-1]}: {key} {packaged[key]!r} differs from plugin.toml {manifest[key]!r}")
    name = f"{manifest['id']}-{manifest['version']}.amplugin"
    out.mkdir(parents=True, exist_ok=True)
    package, checksum = out / name, out / (name + ".sha256")
    package.write_bytes(data)
    checksum.write_text(f"{sha256_hex(data)}  {name}\n")
    return package, checksum


# --- release sources --------------------------------------------------------------------

class GitHub:
    """Releases and assets from the GitHub API (token optional; public repositories only)."""

    def __init__(self):
        self.token = os.environ.get("GITHUB_TOKEN") or os.environ.get("GH_TOKEN")

    def _open(self, url: str, accept: str, auth: bool):
        headers = {"User-Agent": "animathio-registry", "Accept": accept,
                   "X-GitHub-Api-Version": "2022-11-28"}
        if auth and self.token:
            headers["Authorization"] = f"Bearer {self.token}"
        try:
            return urllib.request.urlopen(urllib.request.Request(url, headers=headers), timeout=60)
        except urllib.error.HTTPError as e:
            error = TransientError if e.code in (403, 429) or e.code >= 500 else RegistryError
            raise error(f"GET {url}: HTTP {e.code}")
        except (urllib.error.URLError, TimeoutError) as e:
            raise TransientError(f"GET {url}: {getattr(e, 'reason', e)}")

    def api(self, path: str):
        with self._open(f"https://api.github.com{path}", "application/vnd.github+json", True) as r:
            return json.load(r)

    def releases(self, repo: str) -> list[dict]:
        out = []
        for page in range(1, 6):
            batch = self.api(f"/repos/{repo}/releases?per_page=100&page={page}")
            out.extend(batch)
            if len(batch) < 100:
                break
        return out

    def fetch(self, asset: dict, limit: int) -> bytes:
        # Public assets: no token, so it is never forwarded to a redirect target.
        with self._open(asset["browser_download_url"], "application/octet-stream", False) as r:
            data = r.read(limit + 1)
        if len(data) > limit:
            raise RegistryError(f"{asset['name']}: larger than {limit} bytes")
        return data


class LocalAssets:
    """Dry run: assets in a directory stand in for releases of the hosted repository, with
    the URLs GitHub would give them. `package` writes exactly this layout."""

    def __init__(self, directory: Path):
        self.directory = directory

    def api(self, path: str):
        if path.startswith(f"/repos/{HOSTED_REPO}"):
            return {"private": False, "archived": False, "disabled": False,
                    "license": {"spdx_id": "MIT"}}
        raise RegistryError(f"dry run has no data for {path}")

    def releases(self, repo: str) -> list[dict]:
        if repo != HOSTED_REPO:
            return []
        releases = {}
        for file in sorted(self.directory.glob("*.amplugin*")):
            m = re.fullmatch(r"(.+)-(\d+\.\d+\.\d+)\.amplugin(\.sha256)?", file.name)
            if not m:
                continue
            tag = f"{m.group(1)}@{m.group(2)}"
            stamp = datetime.datetime.fromtimestamp(file.stat().st_mtime, datetime.timezone.utc)
            release = releases.setdefault(tag, {
                "tag_name": tag, "draft": False, "prerelease": False, "assets": [],
                "published_at": stamp.strftime("%Y-%m-%dT%H:%M:%SZ")})
            release["assets"].append({
                "name": file.name,
                "browser_download_url": f"https://github.com/{repo}/releases/download/{tag}/{file.name}"})
        return list(releases.values())

    def fetch(self, asset: dict, limit: int) -> bytes:
        data = (self.directory / asset["name"]).read_bytes()
        if len(data) > limit:
            raise RegistryError(f"{asset['name']}: larger than {limit} bytes")
        return data


def tag_version(entry: dict, tag: str) -> str | None:
    """Version a release tag stands for, or None when the tag is not for this plugin."""
    prefix = entry["id"] + "@"
    if tag.startswith(prefix):
        version = tag[len(prefix):]
        return version if SEMVER.fullmatch(version) else None
    if "path" not in entry:  # one plugin per repository: v1.2.3 or 1.2.3
        m = re.fullmatch(r"v?(" + SEMVER.pattern + ")", tag)
        return m.group(1) if m else None
    return None


def source_url(entry: dict) -> str:
    base = f"https://github.com/{entry['repo']}"
    return f"{base}/tree/HEAD/{entry['path']}" if "path" in entry else base


def resolve_release(entry: dict, source) -> dict:
    candidates = []
    for release in source.releases(entry["repo"]):
        if release.get("draft") or release.get("prerelease"):
            continue
        version = tag_version(entry, release["tag_name"])
        if version:
            candidates.append((semver_key(version), version, release))
    if not candidates:
        raise RegistryError(f"{entry['id']}: no published release of {entry['repo']} is tagged "
                            f"{entry['id']}@<version>" + ("" if "path" in entry else ", v<version> or <version>"))
    _, version, release = max(candidates, key=lambda c: c[0])
    stem = f"{entry['id']}-{version}.amplugin"
    assets = {a["name"]: a for a in release.get("assets", [])}
    for needed in (stem, stem + ".sha256"):
        if needed not in assets:
            raise RegistryError(f"{entry['id']}: release {release['tag_name']} lacks asset {needed}")
    data = source.fetch(assets[stem], MAX_PLUGIN_BYTES)
    digest = sha256_hex(data)
    published = source.fetch(assets[stem + ".sha256"], 4096).decode("utf-8", "replace").split()
    if not published or published[0].lower() != digest:
        raise RegistryError(f"{entry['id']}: {stem}.sha256 does not match the downloaded {stem} ({digest})")
    manifest = inspect_package(data, stem)
    for key, want in (("id", entry["id"]), ("version", version)):
        if manifest[key] != want:
            raise RegistryError(f"{entry['id']}: {stem} contains plugin.toml {key} = {manifest[key]!r}, expected {want!r}")
    if manifest["api_version"] != entry["min_abi"]:
        raise RegistryError(f"{entry['id']}: {stem} needs ABI {manifest['api_version']} but "
                            f"community-plugins.json says min_abi {entry['min_abi']}; update it")
    result = {key: entry[key] for key in ("id", "name", "author", "description", "license", "repo")}
    if "path" in entry:
        result["path"] = entry["path"]
    result.update(
        source_url=source_url(entry), kinds=manifest["kinds"], min_abi=manifest["api_version"],
        version=version, download_url=assets[stem]["browser_download_url"], sha256=digest,
        size=len(data), released_at=release.get("published_at") or "")
    return result


# --- validation -------------------------------------------------------------------------

def schema_errors(instance, schema_file: str) -> list[str]:
    import jsonschema
    schema = load_json(ROOT / "schemas" / schema_file)
    validator = jsonschema.Draft202012Validator(schema, format_checker=jsonschema.FormatChecker())
    return [f"{'/'.join(str(p) for p in e.absolute_path) or '<root>'}: {e.message}"
            for e in sorted(validator.iter_errors(instance), key=lambda e: list(e.absolute_path))]


def validate(offline: bool, source=None) -> list[str]:
    errors = []
    registry = load_json(REGISTRY)
    errors += [f"community-plugins.json {e}" for e in schema_errors(registry, "community-plugins.schema.json")]
    if errors:
        return errors
    approved = approved_licenses()
    plugins = registry["plugins"]
    delisted = {d["id"] for d in registry.get("delisted", [])}
    seen_ids, seen_names = {}, {}
    source = source or GitHub()
    for entry in plugins:
        pid = entry["id"]
        where = f"{pid}:"
        if pid in seen_ids:
            errors.append(f"{where} duplicate id")
        seen_ids[pid] = entry
        if pid.lower().startswith(RESERVED_PREFIX):
            errors.append(f"{where} ids under {RESERVED_PREFIX}* are reserved for first-party plugins "
                          "(the host refuses to load them from the user plugin folder)")
        if pid in delisted:
            errors.append(f"{where} was delisted; delisted ids cannot be listed again")
        lowered = entry["name"].casefold()
        if lowered in seen_names:
            errors.append(f"{where} name {entry['name']!r} is already used by {seen_names[lowered]}")
        seen_names[lowered] = pid
        ids = license_ids(entry["license"])
        if not ids:
            errors.append(f"{where} license {entry['license']!r} has no SPDX identifier")
        bad = [i for i in ids if i not in approved]
        if bad:
            errors.append(f"{where} license identifiers not OSI-approved (or OFL): {', '.join(bad)}")
        hosted = entry["repo"] == HOSTED_REPO
        if hosted and "path" not in entry:
            errors.append(f"{where} plugins hosted in {HOSTED_REPO} need a path (community/<name>)")
        if hosted and "path" in entry:
            plugin_dir = ROOT / entry["path"]
            if not entry["path"].startswith("community/"):
                errors.append(f"{where} hosted plugins live under community/")
            if not (plugin_dir / "plugin.toml").is_file():
                errors.append(f"{where} {entry['path']}/plugin.toml does not exist")
            else:
                try:
                    manifest = read_manifest_text((plugin_dir / "plugin.toml").read_text(), entry["path"])
                    if manifest["id"] != pid:
                        errors.append(f"{where} {entry['path']}/plugin.toml has id {manifest['id']!r}")
                    if manifest["api_version"] != entry["min_abi"]:
                        errors.append(f"{where} min_abi {entry['min_abi']} differs from "
                                      f"{entry['path']}/plugin.toml api_version {manifest['api_version']}")
                    if not (plugin_dir / "build.sh").is_file():
                        errors.append(f"{where} {entry['path']}/build.sh is missing")
                except RegistryError as e:
                    errors.append(str(e))
            if not (ROOT / "LICENSE").is_file():
                errors.append(f"{where} no LICENSE file")
        if offline:
            continue
        try:
            repo = source.api(f"/repos/{entry['repo']}")
        except RegistryError as e:
            errors.append(f"{where} repository {entry['repo']} is not reachable ({e})")
            continue
        if repo.get("private"):
            errors.append(f"{where} {entry['repo']} is not public")
        if repo.get("archived") or repo.get("disabled"):
            errors.append(f"{where} {entry['repo']} is archived or disabled")
        if not hosted:
            detected = (repo.get("license") or {}).get("spdx_id")
            if detected in (None, "NOASSERTION"):
                errors.append(f"{where} GitHub finds no recognizable LICENSE file in {entry['repo']}")
            elif detected not in ids:
                errors.append(f"{where} {entry['repo']} is licensed {detected} but the entry declares {entry['license']!r}")
    return errors


# --- README table -----------------------------------------------------------------------

def cell(text: str) -> str:
    return text.replace("|", "\\|").replace("\n", " ")


def render_table(registry: dict) -> str:
    rows = ["| Name | Author | Description | License | ABI | Source |", "| --- | --- | --- | --- | --- | --- |"]
    for entry in sorted(registry["plugins"], key=lambda e: e["name"].casefold()):
        url = source_url(entry)
        rows.append(f"| [{cell(entry['name'])}]({url}) | {cell(entry['author'])} | "
                    f"{cell(entry['description'])} | {cell(entry['license'])} | {entry['min_abi']} | "
                    f"[{entry['repo']}]({url}) |")
    return "\n".join(rows)


def sync_readme(write: bool) -> bool:
    text = README.read_text()
    start, end = text.find(TABLE_START), text.find(TABLE_END)
    if start < 0 or end < start:
        raise RegistryError(f"README.md needs the {TABLE_START} / {TABLE_END} markers")
    wanted = text[:start + len(TABLE_START)] + "\n" + render_table(load_json(REGISTRY)) + "\n" + text[end:]
    if wanted == text:
        return True
    if write:
        README.write_text(wanted)
    return write


# --- index ------------------------------------------------------------------------------

def build_index(source, previous: dict, strict: bool, allow_unreleased_hosted: bool) -> tuple[dict, list[str]]:
    registry = load_json(REGISTRY)
    old = {p["id"]: p for p in previous.get("plugins", [])}
    entries, problems = [], []
    for entry in registry["plugins"]:
        try:
            entries.append(resolve_release(entry, source))
        except RegistryError as e:
            unreleased = "no published release" in str(e)
            if allow_unreleased_hosted and unreleased and entry["repo"] == HOSTED_REPO:
                annotate("notice", f"{entry['id']}: not released yet (hosted plugins are released after merge)")
                continue
            problems.append(str(e))
            if strict:
                continue
            if isinstance(e, TransientError) and entry["id"] in old:
                # GitHub hiccup: keep serving the entry that was verified earlier.
                annotate("warning", f"{e}; keeping the previous index entry")
                entries.append(old[entry["id"]])
            else:
                # Missing, deleted or unverifiable release: never keep serving it.
                annotate("warning", f"{e}; plugin left out of the index")
    index = {"$schema": "./schemas/community-index.schema.json", "schema_version": 1,
             "plugins": sorted(entries, key=lambda p: p["id"]),
             "delisted": sorted(registry.get("delisted", []), key=lambda d: d["id"])}
    return index, problems


def dump(data) -> str:
    return json.dumps(data, indent=2, ensure_ascii=False) + "\n"


# --- commands ---------------------------------------------------------------------------

def cmd_validate(args) -> int:
    errors = validate(args.offline)
    if not errors and not sync_readme(write=False):
        errors.append("README.md community table is out of date: run `python3 scripts/registry.py table --write`")
    for error in errors:
        annotate("error", error)
    print(f"registry: {'FAILED' if errors else 'ok'} ({len(errors)} problem(s))")
    return 1 if errors else 0


def cmd_package(args) -> int:
    package, checksum = package_plugin(Path(args.plugin_dir).resolve(), Path(args.out).resolve(), not args.no_build)
    print(package)
    print(checksum)
    return 0


def cmd_resolve(args) -> int:
    registry = load_json(REGISTRY)
    if args.tag:
        pid, sep, version = args.tag.rpartition("@")
        if not sep or not SEMVER.fullmatch(version):
            raise RegistryError(f"tag {args.tag!r} must be <plugin-id>@<MAJOR.MINOR.PATCH>")
        entry = next((p for p in registry["plugins"] if p["id"] == pid), None)
        if entry is None:
            raise RegistryError(f"{pid} is not in community-plugins.json; add it by pull request before releasing")
    else:
        path = args.path.strip("/")
        entry = next((p for p in registry["plugins"] if p.get("path") == path and p["repo"] == HOSTED_REPO), None)
        if entry is None:
            raise RegistryError(f"no hosted registry entry has path {path!r}")
        version = None
    if entry["repo"] != HOSTED_REPO:
        raise RegistryError(f"{entry['id']} is released from {entry['repo']}, not from {HOSTED_REPO}")
    manifest = read_manifest_text((ROOT / entry["path"] / "plugin.toml").read_text(), entry["path"])
    if manifest["id"] != entry["id"]:
        raise RegistryError(f"{entry['path']}/plugin.toml has id {manifest['id']!r}, registry says {entry['id']!r}")
    if version is not None and manifest["version"] != version:
        raise RegistryError(f"tag version {version} differs from {entry['path']}/plugin.toml version {manifest['version']}")
    version = manifest["version"]
    for key, value in (("id", entry["id"]), ("version", version), ("path", entry["path"]),
                       ("tag", f"{entry['id']}@{version}")):
        print(f"{key}={value}")
    return 0


def cmd_index(args) -> int:
    source = LocalAssets(Path(args.assets_dir)) if args.assets_dir else GitHub()
    previous = {}
    if args.previous and Path(args.previous).is_file():
        previous = load_json(Path(args.previous))
    index, problems = build_index(source, previous, args.strict, args.allow_unreleased_hosted)
    errors = schema_errors(index, "community-index.schema.json")
    for error in errors:
        annotate("error", f"index {error}")
    output = Path(args.output)
    output.write_text(dump(index))
    print(f"wrote {output} with {len(index['plugins'])} plugin(s)")
    for problem in problems:
        annotate("error" if args.strict else "warning", problem)
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary and problems:
        with open(summary, "a", encoding="utf-8") as f:
            f.write("### Registry problems\n\n" + "".join(f"- {p}\n" for p in problems))
    return 1 if errors or (args.strict and problems) else 0


def cmd_table(args) -> int:
    ok = sync_readme(write=args.write)
    if not ok:
        annotate("error", "README.md community table is out of date: run `python3 scripts/registry.py table --write`")
    return 0 if ok else 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("validate")
    p.add_argument("--offline", action="store_true", help="skip GitHub API checks")
    p.set_defaults(run=cmd_validate)
    p = sub.add_parser("package")
    p.add_argument("plugin_dir")
    p.add_argument("--out", required=True)
    p.add_argument("--no-build", action="store_true", help="reuse an existing target/*.amplugin")
    p.set_defaults(run=cmd_package)
    p = sub.add_parser("resolve")
    group = p.add_mutually_exclusive_group(required=True)
    group.add_argument("--tag")
    group.add_argument("--path")
    p.set_defaults(run=cmd_resolve)
    p = sub.add_parser("index")
    p.add_argument("--output", default=str(INDEX))
    p.add_argument("--previous", default=str(INDEX), help="keep these entries when a release cannot be resolved (non-strict)")
    p.add_argument("--assets-dir", help="dry run: use assets from this directory instead of GitHub releases")
    p.add_argument("--strict", action="store_true", help="fail when any plugin cannot be resolved")
    p.add_argument("--allow-unreleased-hosted", action="store_true",
                   help="submission CI: plugins hosted here are released after merge")
    p.set_defaults(run=cmd_index)
    p = sub.add_parser("table")
    group = p.add_mutually_exclusive_group(required=True)
    group.add_argument("--check", action="store_true")
    group.add_argument("--write", action="store_true")
    p.set_defaults(run=cmd_table)
    args = parser.parse_args()
    try:
        return args.run(args)
    except RegistryError as e:
        annotate("error", str(e))
        return 1


if __name__ == "__main__":
    sys.exit(main())
