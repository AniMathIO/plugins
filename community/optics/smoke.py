#!/usr/bin/env python3
"""Released-host integration checks. All files stay under this plugin's target/.
Usage: python3 smoke.py /path/to/animathio-cli /path/to/plugin-dir [--screenshots]
"""
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import zipfile
from PIL import Image

ROOT = Path(__file__).resolve().parent
WORK = ROOT / 'target' / 'smoke'
WORK.mkdir(parents=True, exist_ok=True)
CLI = str(Path(sys.argv[1]).resolve())
PLUGIN_ID = 'io.github.animathio.optics'

plugins_dir = Path(sys.argv[2]).resolve()
if not any(plugins_dir.glob('*.amplugin')):
    raise SystemExit('pass the directory containing the built optics.amplugin')
ENV = dict(os.environ, ANIMATHIO_PLUGIN_DIR=str(plugins_dir),
           ANIMATHIO_PLUGIN_SETTINGS=str(WORK / 'community-plugins.json'),
           ANIMATHIO_USER_PLUGIN_DIR=str(WORK / 'user-plugins'),
           XDG_CONFIG_HOME=str(WORK / 'config'), XDG_CACHE_HOME=str(WORK / 'cache'),
           XDG_DATA_HOME=str(WORK / 'data'),
           ANIMATHIO_AUDIO='none', PIPEWIRE_REMOTE='animathio-silent-no-such-remote',
           PULSE_SERVER='unix:/nonexistent/animathio-silent', SDL_AUDIODRIVER='dummy')

def run(*args):
    result = subprocess.run([CLI, *map(str, args)], env=ENV, text=True, capture_output=True, timeout=120)
    if result.returncode or any(s in result.stderr.lower() for s in ['failed', 'trap', 'placeholder', 'not loaded']):
        raise RuntimeError(result.stdout + result.stderr)
    return result.stdout

sample = WORK / 'sample.amio'
sample.unlink(missing_ok=True)
run('new-sample', '-o', sample)
with zipfile.ZipFile(sample) as z:
    project = json.loads(z.read('project.json'))
project['assets'] = {}
project['width'] = 1280
project['height'] = 720
project['background'] = [17, 20, 28, 255]
project['tracks'] = project['tracks'][:1]
clip = project['tracks'][0]['clips']['1']
clip['animations'] = []
clip['effects'] = []

def render(name, frame=0, **params):
    clip['kind'] = {'Plugin': {'plugin_id': PLUGIN_ID,
                             'params': dict(params)}}
    file = WORK / (name + '.amio')
    with zipfile.ZipFile(file, 'w', zipfile.ZIP_DEFLATED) as z:
        z.writestr('project.json', json.dumps(project))
    png = WORK / (name + '.png')
    png.unlink(missing_ok=True)
    run('render', file, '--frame', frame, '-o', png)
    image = Image.open(io.BytesIO(png.read_bytes())).convert('RGB')
    # A placeholder or blank canvas must not pass as a successful render:
    # require a real spread of pixels away from the project background.
    colors = dict((rgb, count) for count, rgb in image.getcolors(image.width * image.height))
    drawn = sum(count for rgb, count in colors.items()
                if max(abs(rgb[0] - 17), abs(rgb[1] - 20), abs(rgb[2] - 28)) > 30)
    assert drawn > 300, name
    print('PASS', name)
    return image

# Every setup at five progress steps; rays must grow (frames differ) and the
# rendering must be byte-deterministic on repeat.
frames = {}
for setup in ['lens-diagram', 'refraction', 'tir', 'mirror', 'dispersion']:
    for p in [0, 0.25, 0.5, 0.75, 1]:
        name = f'{setup}-{int(p * 100):03}'
        frames[name] = render(name, setup=setup, progress=p)
    pngs = [(WORK / f'{setup}-{p:03}.png').read_bytes() for p in [0, 25, 50, 75, 100]]
    assert len(set(pngs)) == 5, setup
    render(f'{setup}-repeat', setup=setup, progress=0.5)
    assert pngs[2] == (WORK / f'{setup}-repeat.png').read_bytes(), setup

# Parameter variations that must change the picture.
frames['lens-diverging'] = render('lens-diverging', setup='lens-diagram', progress=1, lens_type='diverging')
assert frames['lens-diagram-100'].tobytes() != frames['lens-diverging'].tobytes()
frames['lens-magnifier'] = render('lens-magnifier', setup='lens-diagram', progress=1, object_distance=12, object_height=10, focal_length=20)
assert frames['lens-diagram-100'].tobytes() != frames['lens-magnifier'].tobytes()
frames['lens-at-focus'] = render('lens-at-focus', setup='lens-diagram', object_distance=20)
frames['mirror-concave-virtual'] = render('mirror-concave-virtual', setup='mirror', object_distance=10, object_height=6)
frames['mirror-convex'] = render('mirror-convex', setup='mirror', progress=1, curvature=-30)
assert frames['mirror-100'].tobytes() != frames['mirror-convex'].tobytes()
frames['refraction-steep'] = render('refraction-steep', setup='refraction', progress=1, angle_deg=70)
assert frames['refraction-100'].tobytes() != frames['refraction-steep'].tobytes()
frames['refraction-no-labels'] = render('refraction-no-labels', setup='refraction', progress=1, show_labels=False)
assert frames['refraction-100'].tobytes() != frames['refraction-no-labels'].tobytes()
frames['refraction-no-normal'] = render('refraction-no-normal', setup='refraction', progress=1, show_normal=False)
assert frames['refraction-100'].tobytes() != frames['refraction-no-normal'].tobytes()
frames['tir-past-critical'] = render('tir-past-critical', setup='tir', progress=1, angle_deg=60, index=1.5)
assert frames['tir-100'].tobytes() != frames['tir-past-critical'].tobytes()
frames['dispersion-single'] = render('dispersion-single', setup='dispersion', progress=1, spectrum=False)
assert frames['dispersion-100'].tobytes() != frames['dispersion-single'].tobytes()
frames['dispersion-few'] = render('dispersion-few', setup='dispersion', progress=1, ray_count=5)
assert frames['dispersion-100'].tobytes() != frames['dispersion-few'].tobytes()
render('dispersion-high-count', setup='dispersion', progress=1, ray_count=64)
render('dispersion-internal-bounce', setup='dispersion', progress=1, angle_deg=5, index=1.75, ray_count=64)
render('dispersion-corner', setup='dispersion', angle_deg=0)
frames['dispersion-flint'] = render('dispersion-flint', setup='dispersion', progress=1, index=1.9)
assert frames['dispersion-100'].tobytes() != frames['dispersion-flint'].tobytes()
render('tir-tir-impossible', setup='tir', progress=1, index=1.0, index_1=1.4)  # no TIR, still draws
render('accent', setup='lens-diagram', progress=1, accent=[120, 255, 120, 255])

# Host sampling of a keyframed progress must reproduce the static midpoint.
clip['animations'] = [{'target': {'Param': 'progress'}, 'keys': {
    '0': {'value': {'Number': 0.0}, 'interpolation': 'Linear'},
    '705600000': {'value': {'Number': 1.0}, 'interpolation': 'Hold'}}}]
assert frames['dispersion-050'].tobytes() == render('keyframed', frame=15, setup='dispersion', progress=0).tobytes()
clip['animations'] = []

# A portrait-ish canvas keeps the layout sane (min-edge scaling).
project['width'], project['height'] = 720, 1280
render('portrait', setup='lens-diagram', progress=1)
project['width'], project['height'] = 1280, 720
if '--screenshots' in sys.argv:
    out = ROOT / 'screenshots'
    out.mkdir(exist_ok=True)
    names = {
        'lens-diagram': 'lens-converging',
        'refraction': 'refraction',
        'tir': 'tir',
        'mirror': 'mirror-concave',
        'dispersion': 'dispersion',
    }
    for setup, label in names.items():
        for p in [0, 25, 50, 75, 100]:
            frames[f'{setup}-{p:03}'].save(out / f'{label}-{p:03}.png', optimize=True)
    for name in ['lens-diverging', 'lens-magnifier', 'mirror-convex', 'mirror-concave-virtual']:
        frames[name].save(out / f'{name}.png', optimize=True)
print('All released-host checks passed')
