#!/usr/bin/env python3
"""Build and smoke-test registry entries (submission CI).

Usage: python3 scripts/check_submission.py CLI INDEX_JSON WORK_DIR [--base community-plugins.json | --all]
                                            [--build-external]

Targets are the entries that differ from --base (a previous copy of community-plugins.json;
new entries and changed fields count) or every entry with --all. For each target:
  * hosted in AniMathIO/plugins: build it with its build.sh and package it like a release;
  * elsewhere: download the release asset the index points at (already SHA-256 checked by
    `registry.py index`); with --build-external also clone the repository at that release tag
    and build it with its build.sh (third-party code runs: only for pull requests that add or
    change the entry, in a step without credentials). The built package must carry the same id,
    version and ABI as the release.
Then every collected .amplugin is loaded and rendered with the released CLI.
"""
import json
import re
import shutil
import subprocess
import sys
import urllib.request
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import registry  # noqa: E402


def targets(current: dict, base_file: str | None) -> list[dict]:
    if base_file is None:
        return current["plugins"]
    old = {p["id"]: p for p in json.loads(Path(base_file).read_text())["plugins"]} if Path(base_file).is_file() else {}
    return [p for p in current["plugins"] if old.get(p["id"]) != p]


def main() -> int:
    argv = sys.argv[1:]
    cli, index_file, work = argv[0], Path(argv[1]), Path(argv[2]).resolve()
    base = argv[argv.index("--base") + 1] if "--base" in argv else None
    if "--all" not in argv and base is None:
        print("pass --base FILE or --all", file=sys.stderr)
        return 2
    build_external = "--build-external" in argv
    index = {p["id"]: p for p in json.loads(index_file.read_text())["plugins"]}
    chosen = targets(registry.load_json(registry.REGISTRY), None if "--all" in argv else base)
    shutil.rmtree(work, ignore_errors=True)
    plugins = work / "plugins"
    plugins.mkdir(parents=True)
    failures = []
    for entry in chosen:
        pid = entry["id"]
        print(f"== {pid}")
        try:
            if entry["repo"] == registry.HOSTED_REPO:
                package, _ = registry.package_plugin(registry.ROOT / entry["path"], plugins)
                print(f"built {package.name}")
                continue
            released = index.get(pid)
            if released is None:
                raise registry.RegistryError("no release in the index (publish a release first)")
            asset = {"name": Path(released["download_url"]).name, "browser_download_url": released["download_url"]}
            data = registry.GitHub().fetch(asset, registry.MAX_PLUGIN_BYTES)
            if registry.sha256_hex(data) != released["sha256"]:
                raise registry.RegistryError("download does not match the index checksum")
            (plugins / asset["name"]).write_bytes(data)
            if not build_external:
                print("source build skipped (it runs for pull requests that add or change an entry)")
                continue
            tag = re.search(r"/releases/download/([^/]+)/", released["download_url"]).group(1)
            src = work / "src" / pid
            subprocess.run(["git", "clone", "--quiet", "--depth", "1", "--branch", tag,
                            f"https://github.com/{entry['repo']}.git", str(src)], check=True, timeout=300)
            plugin_dir = src / entry.get("path", "")
            if not (plugin_dir / "build.sh").is_file():
                raise registry.RegistryError(f"{entry.get('path', '.')}/build.sh not found in {entry['repo']}@{tag} "
                                             "(use the plugin template layout)")
            built, _ = registry.package_plugin(plugin_dir, work / "built")
            manifest = registry.inspect_package(built.read_bytes(), built.name)
            for key, want in (("id", released["id"]), ("version", released["version"]), ("api_version", released["min_abi"])):
                if manifest[key] != want:
                    raise registry.RegistryError(f"source at {tag} builds {key} {manifest[key]!r}, release has {want!r}")
            print(f"source at {tag} builds for wasm32-wasip2 and matches the release")
        except (registry.RegistryError, subprocess.SubprocessError, OSError) as e:
            failures.append(pid)
            registry.annotate("error", f"{pid}: {e}")
    if failures:
        return 1
    if not any(plugins.glob("*.amplugin")):
        print("no registry entries changed; nothing to smoke-test")
        return 0
    return subprocess.run([sys.executable, str(Path(__file__).with_name("registry_smoke.py")), cli, str(plugins)]).returncode


if __name__ == "__main__":
    sys.exit(main())
