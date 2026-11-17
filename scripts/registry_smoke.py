#!/usr/bin/env python3
"""Generic load/render check of .amplugin files with a released animathio-cli.

Usage: python3 scripts/registry_smoke.py PATH/TO/animathio-cli PLUGIN_DIR [--only PLUGIN_ID]

PLUGIN_DIR holds the .amplugin files to test (it is used as ANIMATHIO_PLUGIN_DIR, so nothing
else is loaded). Every plugin must load without a host diagnostic. A `clip-source` plugin is
also rendered with its default parameters and must draw something. Other kinds are checked
for loading only: the editor cannot enumerate their parameters headlessly, so a maintainer
reviews their behaviour (plugins can add their own smoke test like community/code-snippets).
No GUI and no audio device are used.
"""
import io
import json
import os
import subprocess
import sys
import tempfile
import tomllib
import zipfile
from pathlib import Path

from PIL import Image

BAD_WORDS = ("not loaded", "failed", "trap", "placeholder", "unresolved", "panick")


def main() -> int:
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    only = sys.argv[sys.argv.index("--only") + 1] if "--only" in sys.argv else None
    if only:
        args.remove(only)
    cli, plugins = str(Path(args[0]).resolve()), Path(args[1]).resolve()
    env = dict(os.environ, ANIMATHIO_PLUGIN_DIR=str(plugins), ANIMATHIO_AUDIO="none",
               PIPEWIRE_REMOTE="animathio-silent-no-such-remote",
               PULSE_SERVER="unix:/nonexistent/animathio-silent", SDL_AUDIODRIVER="dummy")

    def run(*cmd):
        result = subprocess.run([cli, *map(str, cmd)], env=env, capture_output=True, text=True, timeout=300)
        problem = result.returncode or any(w in result.stderr.lower() for w in BAD_WORDS)
        if problem:
            raise RuntimeError(f"animathio-cli {' '.join(map(str, cmd))}\n{result.stdout}{result.stderr}")

    manifests = []
    for package in sorted(plugins.glob("*.amplugin")):
        try:
            with zipfile.ZipFile(package) as bundle:
                manifest = tomllib.loads(bundle.read("plugin.toml").decode())
        except (zipfile.BadZipFile, KeyError, tomllib.TOMLDecodeError) as e:
            print(f"FAIL {package.name}: not a valid .amplugin ({e})", file=sys.stderr)
            return 1
        if only is None or manifest["id"] == only:
            manifests.append(manifest)
    if not manifests:
        print("no plugins to test", file=sys.stderr)
        return 1

    failures = 0
    with tempfile.TemporaryDirectory(dir=plugins.parent) as temporary:
        work = Path(temporary)
        run("new-sample", "-o", work / "sample.amio")
        with zipfile.ZipFile(work / "sample.amio") as bundle:
            project = json.loads(bundle.read("project.json"))
        project["assets"] = {}
        project["tracks"] = project["tracks"][:1]
        clip = project["tracks"][0]["clips"]["1"]
        clip["animations"] = []
        clip["effects"] = []
        original_clips = project["tracks"][0]["clips"]

        def render(name, kind=None):
            project["tracks"][0]["clips"] = {} if kind is None else original_clips
            if kind is not None:
                clip["kind"] = kind
            path = work / f"{name}.amio"
            with zipfile.ZipFile(path, "w") as bundle:
                bundle.writestr("project.json", json.dumps(project))
            output = work / f"{name}.png"
            output.unlink(missing_ok=True)
            run("render", path, "--frame", "0", "-o", output)
            return Image.open(io.BytesIO(output.read_bytes())).convert("RGBA")

        try:
            empty = render("empty")  # background only: also proves every plugin in the folder loads
        except RuntimeError as e:
            print(f"FAIL loading plugins from {plugins}\n{e}", file=sys.stderr)
            return 1
        for manifest in manifests:
            pid = manifest["id"]
            try:
                if manifest["kinds"] == ["clip-source"]:
                    image = render("source", {"Plugin": {"plugin_id": pid, "params": {}}})
                    if image.tobytes() == empty.tobytes():
                        raise RuntimeError("default parameters draw nothing (the render equals the empty background)")
                    print(f"PASS {pid}: loads and renders with default parameters")
                else:
                    print(f"PASS {pid}: loads ({', '.join(manifest['kinds'])}; behaviour left to review)")
            except RuntimeError as e:
                failures += 1
                print(f"FAIL {pid}: {e}", file=sys.stderr)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
