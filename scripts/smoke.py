#!/usr/bin/env python3
"""Render every source, then verify invert changes a frame and is self-inverse."""
import io
from PIL import Image
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import tomllib
import zipfile

cli = str(Path(sys.argv[1]).resolve())
plugins = Path(sys.argv[2]).resolve()
env = dict(os.environ, ANIMATHIO_PLUGIN_DIR=str(plugins), ANIMATHIO_AUDIO='none',
           PIPEWIRE_REMOTE='animathio-silent-no-such-remote',
           PULSE_SERVER='unix:/nonexistent/animathio-silent', SDL_AUDIODRIVER='dummy')

def run(*args):
    result = subprocess.run([cli, *map(str, args)], env=env, capture_output=True, text=True)
    if result.returncode or any(word in result.stderr.lower() for word in
                                ['failed', 'unresolved', 'not found', 'placeholder']):
        raise RuntimeError(result.stdout + result.stderr)

with tempfile.TemporaryDirectory(dir=plugins.parent) as temporary:
    work = Path(temporary)
    run('new-sample', '-o', work / 'sample.amio')
    with zipfile.ZipFile(work / 'sample.amio') as bundle:
        project = json.loads(bundle.read('project.json'))
    project['assets'] = {}
    project['tracks'] = project['tracks'][:1]
    clip = project['tracks'][0]['clips']['1']
    clip['animations'] = []
    sources = []
    for package in sorted(plugins.glob('*.amplugin')):
        with zipfile.ZipFile(package) as bundle:
            manifest = tomllib.loads(bundle.read('plugin.toml').decode())
        if manifest['kinds'] == ['clip-source']:
            sources.append(manifest['id'])
    assert sources, 'No source plugins to test'

    def render(plugin, radius=96, effects=()):
        clip['kind'] = {'Plugin': {'plugin_id': plugin, 'params': {'radius': radius}}}
        clip['effects'] = [{'id': i+1, 'plugin': 'org.example.invert',
                            'effect': 'invert', 'params': {}} for i in range(len(effects))]
        path = work / 'frame.amio'
        with zipfile.ZipFile(path, 'w') as bundle:
            bundle.writestr('project.json', json.dumps(project))
        output = work / 'frame.png'
        output.unlink(missing_ok=True)
        run('render', path, '--frame', '0', '-o', output)
        return Image.open(io.BytesIO(output.read_bytes())).convert('RGBA')

    for plugin in sources:
        baseline = render(plugin)
        assert baseline.tobytes() != render(plugin, radius=48).tobytes(), 'Radius did not affect rendering'
        assert baseline.tobytes() != render(plugin, effects=(1,)).tobytes(), 'Invert did not affect rendering'
        restored = render(plugin, effects=(1, 2))
        center = (baseline.width // 2, baseline.height // 2)
        assert baseline.getpixel(center) == restored.getpixel(center), 'Double inversion changed center'
        print(f'PASS {plugin}: radius, invert, double invert')
