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
ENV = dict(os.environ, ANIMATHIO_PLUGIN_DIR=str(Path(sys.argv[2]).resolve()),
           ANIMATHIO_AUDIO='none', PIPEWIRE_REMOTE='animathio-silent-no-such-remote',
           PULSE_SERVER='unix:/nonexistent/animathio-silent', SDL_AUDIODRIVER='dummy')

def run(*args):
    result = subprocess.run([CLI, *map(str, args)], env=ENV, text=True, capture_output=True)
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
CODE = '''fn fibonacci(n: u32) -> u64 {
    match n {
        0 => 0,
        1 => 1,
        _ => fibonacci(n - 1) + fibonacci(n - 2),
    }
}'''
TARGET = '''fn fibonacci(n: u32) -> u64 {
    let (mut a, mut b) = (0, 1);
    for _ in 0..n {
        (a, b) = (b, a + b);
    }
    a
}'''

def render(name, frame=0, **params):
    clip['kind'] = {'Plugin': {'plugin_id': 'org.animathio.community.code-snippets',
                             'params': dict(dict(code=CODE, code_to=TARGET, font_size=4.0), **params)}}
    file = WORK / (name + '.amio')
    with zipfile.ZipFile(file, 'w', zipfile.ZIP_DEFLATED) as z:
        z.writestr('project.json', json.dumps(project))
    png = WORK / (name + '.png')
    png.unlink(missing_ok=True)
    run('render', file, '--frame', frame, '-o', png)
    image = Image.open(io.BytesIO(png.read_bytes())).convert('RGB')
    # A placeholder or blank canvas must not pass as a successful render.
    colors = dict((rgb, count) for count, rgb in image.getcolors(image.width * image.height))
    if params.get('theme', 'ocean') == 'ocean' and params.get('background', True):
        assert colors.get((43, 48, 59), 0) > 1000, name
    else:
        assert any(max(rgb) - min(rgb) > 30 for rgb in colors), name
    print('PASS', name)
    return image

frames = {}
for mode in ['reveal', 'morph']:
    for p in [0, 0.25, 0.5, 0.75, 1]:
        name = f'{mode}-{int(p * 100):03}'
        frames[name] = render(name, mode=mode, progress=p)
    assert frames[f'{mode}-000'].tobytes() != frames[f'{mode}-050'].tobytes()
    assert frames[f'{mode}-050'].tobytes() != frames[f'{mode}-100'].tobytes()
assert frames['reveal-100'].tobytes() == frames['morph-000'].tobytes()
assert frames['morph-050'].tobytes() == render('repeat', mode='morph', progress=0.5).tobytes()
samples = {
    'python': 'def greet(name):\n    # a comment\n    return f"Hello {name}"',
    'javascript': 'const greet = (name) => "Hello " + name;',
    'typescript': 'interface User { name: string; }\nconst user: User = { name: "Ada" };',
    'c': 'int main(void) { /* comment */ return 42; }',
    'cpp': 'template<typename T> T square(T x) { return x * x; }',
    'c++': 'const auto answer = 42;',
    'json': '{"name": "Ada", "active": true, "count": 42}',
    'latex': r"\frac{a}{b} + \sqrt{2} % comment",
    'tex': r"\begin{equation} e^{i\pi} + 1 = 0 \end{equation}",
    'bash': '#!/bin/bash\nfor name in Ada Bob; do\n    echo "$name"\ndone',
}
for language, code in samples.items():
    render('language-' + language.replace('+', 'plus'), language=language, code=code, progress=1)
render('paper-lines', theme='paper', highlight_lines='2,4-6', reveal_by='line', progress=0.5)
render('night-transparent', theme='night', background=False, line_numbers=False)
# Verify the target endpoint against a static rendering with the same box dimensions.
original = CODE
CODE, TARGET = TARGET, CODE
assert frames['morph-100'].tobytes() == render('target-static', mode='morph', progress=0).tobytes()
CODE, TARGET = original, CODE
# Host sampling of a typed progress animation must reproduce the static midpoint.
clip['animations'] = [{'target': {'Param': 'progress'}, 'keys': {
    '0': {'value': {'Number': 0.0}, 'interpolation': 'Linear'},
    '705600000': {'value': {'Number': 1.0}, 'interpolation': 'Hold'}}}]
assert frames['morph-050'].tobytes() == render('keyframed', frame=15, mode='morph', progress=0).tobytes()
clip['animations'] = []
render('bounded-lcs', code=('x;' * 16 + '\n') * 8, code_to=('y;' * 16 + '\n') * 8, mode='morph', progress=0.5)
render('long-comment', code=('// ' + 'a' * 60 + '\n') * 16, progress=1)
if '--screenshots' in sys.argv:
    out = ROOT / 'screenshots'
    out.mkdir(exist_ok=True)
    for mode in ['reveal','morph']:
        for p in [0,25,50,75,100]:
            name = f'{mode}-{p:03}'
            frames[name].save(out / f'{name}.png', optimize=True)
print('All released-host checks passed')
