#!/usr/bin/env python3
"""Render Physics Play in a released AniMathIO sandbox and check the resulting pixels."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import zipfile

from PIL import Image

ROOT = Path(__file__).resolve().parent
WORK = ROOT / 'target' / 'smoke'
PLUGIN_ID = 'io.github.animathio.physics'
BACKGROUND = (17, 20, 28)
AMBER = (240, 150, 40)
SCENES = ('projectile', 'pendulum', 'spring', 'collision', 'stack')
TICKS_PER_SECOND = 705_600_000


def check_pixels(image, name, accent):
    colors = dict((rgb, count) for count, rgb in image.getcolors(image.width * image.height))
    # Host render can return success for a checkerboard with error text. Require
    # the actual project background and a solid patch of the plugin's body color.
    assert colors.get(BACKGROUND, 0) > image.width * image.height // 2, (
        f'{name}: expected the project background around the scene'
    )
    if accent is not None:
        assert colors.get(accent, 0) > 30, f'{name}: no physics body colored {accent}'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('cli', type=Path)
    parser.add_argument('plugin_dir', type=Path, help='directory containing physics.amplugin')
    parser.add_argument('--screenshots', action='store_true')
    args = parser.parse_args()
    cli = args.cli.resolve()
    plugins = args.plugin_dir.resolve()
    if not any(plugins.glob('*.amplugin')):
        parser.error(f'no .amplugin in {plugins}')

    WORK.mkdir(parents=True, exist_ok=True)
    env = dict(
        os.environ,
        ANIMATHIO_PLUGIN_DIR=str(plugins),
        ANIMATHIO_PLUGIN_SETTINGS=str(WORK / 'community-plugins.json'),
        ANIMATHIO_USER_PLUGIN_DIR=str(WORK / 'user-plugins'),
        XDG_CONFIG_HOME=str(WORK / 'config'),
        XDG_CACHE_HOME=str(WORK / 'cache'),
        XDG_DATA_HOME=str(WORK / 'data'),
        ANIMATHIO_AUDIO='none',
    )

    def run(*cmd):
        result = subprocess.run([cli, *map(str, cmd)], env=env, text=True,
                                capture_output=True, timeout=30)
        if result.returncode or 'warning:' in result.stderr.lower():
            raise RuntimeError(f'{cmd}\n{result.stdout}{result.stderr}')

    sample = WORK / 'sample.amio'
    sample.unlink(missing_ok=True)
    run('new-sample', '-o', sample)
    with zipfile.ZipFile(sample) as bundle:
        project = json.loads(bundle.read('project.json'))
    project.update(assets={}, width=1280, height=720, background=[*BACKGROUND, 255])
    project['tracks'] = project['tracks'][:1]
    clip = project['tracks'][0]['clips']['1']
    clip.update(animations=[], effects=[])
    clip['range']['end'] = 30 * TICKS_PER_SECOND

    def render(name, *, frame=0, expected_accent=AMBER, **params):
        clip['kind'] = {'Plugin': {'plugin_id': PLUGIN_ID, 'params': params}}
        path = WORK / f'{name}.amio'
        with zipfile.ZipFile(path, 'w', zipfile.ZIP_DEFLATED) as bundle:
            bundle.writestr('project.json', json.dumps(project))
        png = WORK / f'{name}.png'
        png.unlink(missing_ok=True)
        run('render', path, '--frame', frame, '-o', png)
        with Image.open(png) as result:
            image = result.convert('RGB')
        check_pixels(image, name, expected_accent)
        print('PASS', name)
        return image.tobytes()

    frames = {}
    screenshots = []
    for scene in SCENES:
        for progress in (0, 25, 50, 75, 100):
            name = f'{scene}-{progress:03}'
            frames[name] = render(name, scene=scene, progress=progress / 100)
            if progress in (0, 50, 100):
                screenshots.append(name)
        assert len({frames[f'{scene}-{p:03}'] for p in (0, 25, 50, 75, 100)}) == 5, scene

    midpoint = dict(scene='projectile', progress=0.5)
    for key in ('show_vectors', 'show_trail', 'show_ground'):
        assert frames['projectile-050'] != render(key, **midpoint, **{key: False})
    assert frames['projectile-050'] != render('moon', **midpoint, gravity=1.62)
    green = (61, 199, 120)
    render('custom-accent', **midpoint, accent=[*green, 255], expected_accent=green)
    assert frames['pendulum-050'] != render('long-pendulum', scene='pendulum', progress=0.5, length_m=4)
    assert frames['spring-050'] != render('fast-spring', scene='spring', progress=0.5, gravity=20)

    isolated = dict(scene='collision', bodies=2, duration_s=1, progress=0.85,
                    gravity=0, friction=0, speed=5)
    elastic = render('collision-elastic', **isolated, restitution=1)
    inelastic = render('collision-inelastic', **isolated, restitution=0)
    assert elastic != inelastic
    assert elastic != render('double-mass', **isolated, restitution=1, mass=2)
    render('stack-chain', scene='stack', progress=0.1, bodies=8)
    screenshots += ['collision-elastic', 'collision-inelastic', 'stack-chain']

    # Force maximum-size worlds to run inside the released host's 50M fuel,
    # 64 MiB and epoch limits. Exercise busy contacts as well as default material.
    for scene in SCENES:
        render(f'max-{scene}', scene=scene, duration_s=30, progress=1, bodies=12)
    for scene in ('collision', 'stack'):
        for gravity, restitution, friction in ((0, 1, 0), (30, 1, 0), (30, 0, 1)):
            name = f'{scene}-stress-{gravity}-{restitution}-{friction}'
            params = dict(scene=scene, duration_s=30, progress=1, bodies=12, speed=40,
                          gravity=gravity, restitution=restitution, friction=friction)
            original = render(name, **params)
            assert original == render(f'{name}-repeat', **params)
    # Largest projectile extent has subpixel bodies. The project background
    # still distinguishes the vector render from a sandbox placeholder.
    render('wide-trajectory', scene='projectile', duration_s=30, progress=1,
           speed=40, bodies=12, gravity=0, expected_accent=None)
    render('no-gravity', scene='pendulum', gravity=0)
    render('flat-angle', scene='projectile', angle_deg=0, bodies=1, progress=0.5)
    render('single-body', scene='collision', bodies=1)

    clip['animations'] = [{'target': {'Param': 'progress'}, 'keys': {
        '0': {'value': {'Number': 0.0}, 'interpolation': 'Linear'},
        str(TICKS_PER_SECOND): {'value': {'Number': 1.0}, 'interpolation': 'Hold'},
    }}]
    assert frames['stack-050'] == render('keyframed', frame=15, scene='stack', progress=0)
    clip['animations'] = []
    assert frames['projectile-100'] == render('defaults')

    if args.screenshots:
        out = ROOT / 'screenshots'
        out.mkdir(exist_ok=True)
        for name in screenshots:
            with Image.open(WORK / f'{name}.png') as image:
                image.save(out / f'{name}.png', optimize=True)
    print('All released-host checks passed')


if __name__ == '__main__':
    main()
