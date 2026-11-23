#!/usr/bin/env python3
"""Verify Net Lab on a released AniMathIO CLI and regenerate README screenshots."""
import argparse
import io
import json
import os
from pathlib import Path
import subprocess
import zipfile

from PIL import Image

ROOT = Path(__file__).resolve().parent
WORK = ROOT / "target" / "smoke"
PLUGIN_ID = "io.github.animathio.network"
SCENARIOS = ("ping", "arp", "tcp", "dhcp", "dns", "traceroute")
TOPOLOGIES = ("single-switch", "through-router")
BACKGROUND = (12, 18, 27)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("cli", type=Path)
    parser.add_argument("plugin_dir", type=Path)
    parser.add_argument("--screenshots", action="store_true")
    args = parser.parse_args()
    for folder in ("config", "cache", "data", "tmp"):
        (WORK / folder).mkdir(parents=True, exist_ok=True)
    env = dict(
        os.environ,
        ANIMATHIO_PLUGIN_DIR=str(args.plugin_dir.resolve()),
        ANIMATHIO_AUDIO="none",
        XDG_CONFIG_HOME=str(WORK / "config"),
        XDG_CACHE_HOME=str(WORK / "cache"),
        XDG_DATA_HOME=str(WORK / "data"),
        TMPDIR=str(WORK / "tmp"),
        PIPEWIRE_REMOTE="animathio-silent-no-such-remote",
        PULSE_SERVER="unix:/nonexistent/animathio-silent",
    )
    cli = args.cli.resolve()

    def run(*options):
        command = [cli, *map(str, options)]
        result = subprocess.run(command, env=env, text=True, capture_output=True, timeout=30)
        if result.returncode or any(word in result.stderr.lower() for word in (
            "failed", "trap", "placeholder", "not loaded", "unresolved",
        )):
            raise RuntimeError(f"{command}: {result.stdout}{result.stderr}")
        return result.stdout

    sample = WORK / "sample.amio"
    sample.unlink(missing_ok=True)
    run("new-sample", "-o", sample)
    with zipfile.ZipFile(sample) as bundle:
        project = json.loads(bundle.read("project.json"))
    project["assets"] = {}
    project["width"] = 1280
    project["height"] = 720
    project["tracks"] = project["tracks"][:1]
    clip = project["tracks"][0]["clips"]["1"]
    clip["animations"] = []
    clip["effects"] = []

    def render(name, frame=0, expect_error=False, **params):
        clip["kind"] = {"Plugin": {"plugin_id": PLUGIN_ID, "params": params}}
        project_file = WORK / (name + ".amio")
        with zipfile.ZipFile(project_file, "w", zipfile.ZIP_DEFLATED) as bundle:
            bundle.writestr("project.json", json.dumps(project))
        png = WORK / (name + ".png")
        png.unlink(missing_ok=True)
        run("render", project_file, "--frame", frame, "-o", png)
        encoded = png.read_bytes()
        image = Image.open(io.BytesIO(encoded)).convert("RGB")
        colors = image.getcolors(image.width * image.height)
        # AniMathIO 2.3.0 renders plugin diagnostics as a checkerboard frame.
        # Check the authored background and saturated artwork to catch fallbacks.
        assert (image.getpixel((4, 4)) == BACKGROUND) != expect_error, name
        has_accent = any(max(rgb) - min(rgb) > 55 for _, rgb in colors)
        assert has_accent != expect_error, name
        if not expect_error:
            assert len(colors) > 30, name
        return encoded

    for topology in TOPOLOGIES:
        for scenario in SCENARIOS:
            for show_tables in (True, False):
                prefix = f"{topology}-{scenario}-tables-{show_tables}"
                samples = [
                    render(f"{prefix}-{int(progress * 100):03}", topology=topology,
                           scenario=scenario, progress=progress, show_tables=show_tables)
                    for progress in (0, 0.25, 0.5, 0.75, 1)
                ]
                assert len(set(samples)) == len(samples), prefix
            print(f"PASS {topology} / {scenario} at five progress values with tables on and off")

    midpoint = render("tcp-midpoint", scenario="tcp", progress=0.5)
    assert midpoint == render("tcp-repeat", scenario="tcp", progress=0.5)
    assert midpoint != render("tcp-brief", scenario="tcp", progress=0.5, labels="brief")
    assert midpoint != render(
        "tcp-no-panels", scenario="tcp", progress=0.5, show_tables=False, show_step_list=False,
    )
    assert midpoint != render("tcp-no-speed-lines", scenario="tcp", progress=0.5, speed_lines=False)
    assert midpoint != render("tcp-large-packet", scenario="tcp", progress=0.5, packet_size=6)
    assert midpoint != render("tcp-short-timeline", scenario="tcp", progress=0.5, steps=3)
    assert render("ping-auto-table") != render("ping-mac-table", table="mac")
    assert render("default") == render("ping-end", scenario="ping", progress=1.0)
    trace = {"scenario": "traceroute", "topology": "single-switch"}
    trace_auto = render("trace-auto-table", **trace)
    assert trace_auto == render("trace-mac-table", table="mac", **trace)
    assert trace_auto != render("trace-route-table", table="route", **trace)
    render("invalid-scenario", scenario="ospf", expect_error=True)
    render("invalid-topology", topology="token-ring", expect_error=True)

    clip["animations"] = [{"target": {"Param": "progress"}, "keys": {
        "0": {"value": {"Number": 0.0}, "interpolation": "Linear"},
        "705600000": {"value": {"Number": 1.0}, "interpolation": "Hold"},
    }}]
    assert midpoint == render("tcp-keyframed-midpoint", frame=15, scenario="tcp", progress=0)
    clip["animations"] = []
    print("PASS default frame, exact PNG repeat, parameter changes, diagnostic and host keyframes")

    project["width"], project["height"] = 480, 720
    render("portrait", scenario="dns", progress=0.7, labels="brief", table="route")
    project["width"], project["height"] = 1280, 720
    if args.screenshots:
        out = ROOT / "screenshots"
        out.mkdir(exist_ok=True)
        single = {"topology": "single-switch"}
        shots = {
            "ping": dict(scenario="ping", progress=9 / 12.75),
            "arp-flood": dict(scenario="arp", progress=2 / 8.75, **single),
            "arp-cache": dict(scenario="arp", progress=5 / 8.75, table="arp", **single),
            "tcp": dict(scenario="tcp", progress=5 / 24.75),
            "dhcp": dict(scenario="dhcp", progress=6 / 8.75, **single),
            "dns": dict(scenario="dns", progress=10 / 14.75),
            "traceroute": dict(scenario="traceroute", progress=7 / 10.75),
        }
        for name, params in shots.items():
            (out / f"{name}.png").write_bytes(render(f"screenshot-{name}", **params))
            print(f"SAVED {name}.png")


if __name__ == "__main__":
    main()
