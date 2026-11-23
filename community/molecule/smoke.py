#!/usr/bin/env python3
"""Render and check Molecule Studio frames with the released AniMathIO CLI."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import zipfile

from PIL import Image, ImageChops

ROOT = Path(__file__).resolve().parent
WORK = ROOT / "target" / "smoke"
PLUGIN_ID = "io.github.animathio.molecule"
BACKGROUND = (249, 248, 244)
OXYGEN = (255, 13, 13)
NITROGEN = (48, 80, 248)
ACCENT = (255, 158, 40)
CAFFEINE = "CN1C=NC2=C1C(=O)N(C)C(=O)N2C"
SIZE = (1280, 720)
HOST_ERRORS = ("failed", "trap", "placeholder", "not loaded", "panic")


def pixels(image, color):
    return sum(count for count, rgb in image.getcolors(image.width * image.height) if rgb == color)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("cli", type=lambda path: Path(path).resolve())
    parser.add_argument("plugin_dir", type=lambda path: Path(path).resolve())
    parser.add_argument("--screenshots", action="store_true", help="refresh the README screenshots")
    args = parser.parse_args()
    WORK.mkdir(parents=True, exist_ok=True)
    env = dict(
        os.environ,
        ANIMATHIO_PLUGIN_DIR=str(args.plugin_dir),
        ANIMATHIO_PLUGIN_SETTINGS=str(WORK / "plugin-settings.json"),
        ANIMATHIO_USER_PLUGIN_DIR=str(WORK / "user-plugins"),
        ANIMATHIO_AUDIO="none",
        XDG_CONFIG_HOME=str(WORK / "config"),
        XDG_DATA_HOME=str(WORK / "data"),
        XDG_CACHE_HOME=str(WORK / "cache"),
        TMPDIR=str(WORK),
        PIPEWIRE_REMOTE="animathio-silent-no-such-remote",
        PULSE_SERVER="unix:/nonexistent/animathio-silent",
        SDL_AUDIODRIVER="dummy",
    )

    def run(*command):
        return subprocess.run(
            [args.cli, *map(str, command)], env=env, capture_output=True, text=True, timeout=30
        )

    sample = WORK / "sample.amio"
    sample.unlink(missing_ok=True)
    run("new-sample", "-o", sample).check_returncode()
    with zipfile.ZipFile(sample) as bundle:
        project = json.loads(bundle.read("project.json"))
    project.update(width=SIZE[0], height=SIZE[1], assets={}, background=[*BACKGROUND, 255])
    project["tracks"] = project["tracks"][:1]
    clip = project["tracks"][0]["clips"]["1"]
    clip.update(animations=[], effects=[])
    background = Image.new("RGB", SIZE, BACKGROUND)

    def render(name, *, frame=0, diagnostic=None, **params):
        clip["kind"] = {"Plugin": {"plugin_id": PLUGIN_ID, "params": params}}
        path = WORK / f"{name}.amio"
        with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as bundle:
            bundle.writestr("project.json", json.dumps(project))
        if diagnostic:
            # PNG preview substitutes an error placeholder. Export exposes the
            # guest diagnostic and fails before encoding an invalid clip.
            result = run("export", path, "--to", 0.034, "-o", WORK / f"{name}.webm", "--overwrite")
            report = result.stdout + result.stderr
            assert result.returncode and diagnostic in report and "trap" not in report.lower(), report
            print(f"PASS {name} ({diagnostic})")
            return None
        png = WORK / f"{name}.png"
        png.unlink(missing_ok=True)
        result = run("render", path, "--frame", frame, "-o", png)
        report = result.stdout + result.stderr
        if result.returncode or any(word in result.stderr.lower() for word in HOST_ERRORS):
            raise RuntimeError(f"{name}\n{report}")
        with Image.open(png) as image:
            raster = image.convert("RGB")
        extent = ImageChops.difference(raster, background).getbbox()
        if extent:
            x0, y0, x1, y1 = extent
            assert 0 < x0 < x1 < raster.width and 0 < y0 < y1 < raster.height, (name, extent)
            ink_or_oxygen = sum(
                count for count, (r, g, b) in raster.getcolors(raster.width * raster.height)
                if (r < 200 and g < 200 and b < 205) or (r > 225 and max(g, b) < 180)
            )
            assert ink_or_oxygen > 30, name
        else:
            assert params.get("progress") == 0.0, f"{name} unexpectedly blank"
        print(f"PASS {name}")
        return raster

    frames = {}
    for percent in (0, 25, 50, 75, 100):
        name = f"caffeine-{percent:03}"
        frames[name] = render(name, preset="caffeine", progress=percent / 100)
    caffeine = frames["caffeine-100"]
    assert caffeine.tobytes() == render("default").tobytes()
    assert frames["caffeine-000"].tobytes() == background.tobytes()
    assert len({image.tobytes() for image in frames.values()}) == len(frames)
    assert pixels(caffeine, OXYGEN) > 50 and pixels(caffeine, NITROGEN) > 50
    assert frames["caffeine-050"].tobytes() == render("repeat", progress=0.5).tobytes()

    for mode in ("atom", "together"):
        image = render(f"{mode}-050", progress=0.5, reveal_by=mode)
        assert image.tobytes() != frames["caffeine-050"].tobytes()
        assert caffeine.tobytes() == render(f"{mode}-100", reveal_by=mode).tobytes()

    clip["animations"] = [{
        "target": {"Param": "progress"},
        "keys": {
            "0": {"value": {"Number": 0.0}, "interpolation": "Linear"},
            "705600000": {"value": {"Number": 1.0}, "interpolation": "Hold"},
        },
    }]
    assert frames["caffeine-050"].tobytes() == render("keyframed", frame=15, progress=0.0).tobytes()
    clip["animations"] = []

    # Explicit defaults mirror a clip created by the editor before choosing a preset.
    stored = dict(smiles=CAFFEINE, product_smiles="", mode="structure")
    for preset in ("water", "benzene", "glucose"):
        frames[preset] = render(preset, preset=preset, **stored)
        assert frames[preset].tobytes() != caffeine.tobytes()
    assert pixels(frames["water"], OXYGEN) > 100
    assert pixels(frames["benzene"], OXYGEN) == 0
    annotated = render("highlights", preset="benzene", progress=0, highlight="1,4-6,b0,b2-3")
    assert annotated.tobytes() == background.tobytes()
    frames["benzene-highlight"] = render(
        "benzene-highlight", preset="benzene", highlight="1,4-6,b0,b2-3", atom_numbers=True
    )
    assert frames["benzene-highlight"].tobytes() != frames["benzene"].tobytes()
    mono = render("monochrome", cpk=False)
    assert pixels(mono, OXYGEN) == 0 and pixels(mono, NITROGEN) == 0
    render("alternate-double-bond", preset="aspirin", bond_order_animation="double-stroke")
    frames["water-lone-pairs"] = render("water-lone-pairs", preset="water", show_lone_pairs=True)
    render("hydrogens", preset="ethanol", show_hydrogens=True, show_carbons="labeled")

    for percent in (0, 42, 65, 100):
        name = f"combustion-{percent:03}"
        frames[name] = render(
            name, preset="ethanol-combustion", arrow_label="combustion",
            progress=percent / 100, **stored,
        )
    assert frames["combustion-000"].tobytes() == background.tobytes()
    assert pixels(frames["combustion-042"], ACCENT) == 0
    assert pixels(frames["combustion-065"], ACCENT) > 60
    assert pixels(frames["combustion-100"], ACCENT) > pixels(frames["combustion-065"], ACCENT)
    assert frames["combustion-100"].tobytes() == render(
        "custom-reaction", smiles="CCO.O=O.O=O.O=O>>O=C=O.O=C=O.O.O.O",
        arrow_label="combustion",
    ).tobytes()
    frames["aspirin-synthesis"] = render("aspirin-synthesis", preset="aspirin-synthesis")
    render("separate-reaction", mode="reaction", smiles="CC(O)=O.CCO", product_smiles="CCOC(C)=O.O")

    render("bounded-chain", smiles="C" * 128)
    render("bounded-fragments", mode="reaction", smiles=".".join(["O"] * 8),
           product_smiles=".".join(["O"] * 8))
    render("invalid-smiles", smiles="C1CC", diagnostic="invalid-input")
    render("oversized-smiles", smiles="C" * 513, diagnostic="limit-exceeded")
    render("oversized-reaction", mode="reaction", smiles="O", product_smiles="C" * 128,
           diagnostic="limit-exceeded")
    if args.screenshots:
        out = ROOT / "screenshots"
        out.mkdir(exist_ok=True)
        for name, image in frames.items():
            if name != "combustion-000":
                image.save(out / f"{name}.png", optimize=True)
    print("All released-host checks passed")


if __name__ == "__main__":
    main()
