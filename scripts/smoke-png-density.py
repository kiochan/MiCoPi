"""Verify PNG density interchange across JSONL, MCP, projects, replay and desktop.

Fixtures include independent axes, unspecified units and zero axes. Physical-size
claims stay absent when the imported data cannot define them. No hardware pen
compatibility claim follows from the synthetic pressure/stabilization stroke.
"""
import argparse
import base64
import json
from pathlib import Path
import shutil
import subprocess
import time

from png_checks import SIGNATURE, chunk, chunks, density, pixels, rgba_fixture

ROOT = Path(__file__).resolve().parents[1]
WIDTH, HEIGHT = 96, 64
CASES = [('unequal', (3937, 7874, 1)), ('unspecified', (4, 3, 0)), ('zero-axis', (0, 7874, 1))]


def action(name, **args):
    return dict(action=name, args=args)


def request(number, name, arguments):
    return dict(jsonrpc='2.0', id=number, method='tools/call', params=dict(name=name, arguments=arguments))


def content(reply, allow_failure=False):
    assert 'error' not in reply, reply
    result = reply['result']
    assert allow_failure or not result.get('isError'), result
    return result['structuredContent']['result']


def info_matches(info, raw):
    assert info['width'] == WIDTH and info['height'] == HEIGHT, info
    assert info['png_density'] == dict(xppu=raw[0], yppu=raw[1], unit='meter' if raw[2] else 'unspecified'), info
    assert info['resolution_source'] == 'png', info
    known = raw[2] == 1 and raw[0] > 0 and raw[1] > 0
    assert info['physical_size_known'] == known, info
    if known:
        assert abs(info['width_mm'] - WIDTH * 1000 / raw[0]) < 1e-9, info
        assert abs(info['height_mm'] - HEIGHT * 1000 / raw[1]) < 1e-9, info
        assert abs(info['resolution_ppi_x'] - raw[0] * .0254) < 1e-9, info
        assert abs(info['resolution_ppi_y'] - raw[1] * .0254) < 1e-9, info
    else:
        assert all(key not in info for key in ['width_mm', 'height_mm', 'resolution_ppi_x', 'resolution_ppi_y']), info


def commands(source, project, exported):
    return [
        action('document.open', path=str(source)),
        action('canvas.info'),
        action('brush.set', color='#A44E77', radius=3, stabilizer=.35,
               pressure_size=True, pressure_opacity=True),
        action('canvas.stroke', points=[dict(x=8, y=48, pressure=.2, pen=True, time=0),
                                       dict(x=42, y=15, pressure=1., pen=True, time=40),
                                       dict(x=88, y=50, pressure=.4, pen=True, time=80)]),
        action('canvas.snapshot'),
        action('canvas.resolution', resolution_ppi=600),
        action('canvas.info'),
        action('canvas.snapshot'),
        action('history.undo'),
        action('canvas.info'),
        action('canvas.snapshot'),
        action('history.redo'),
        action('canvas.info'),
        action('history.undo'),
        action('document.save', path=str(project)),
        action('document.open', path=str(project)),
        action('canvas.info'),
        action('replay.verify'),
        action('canvas.export', path=str(exported)),
        action('canvas.snapshot'),
    ]


def validate(values, raw, exported):
    for index in [1, 9, 16]:
        info_matches(values[index], raw)
    for index in [6, 12]:
        changed = values[index]
        assert changed['resolution_ppi'] == 600 and changed['resolution_source'] == 'document', changed
        assert changed.get('png_density') is None and changed['physical_size_known'], changed
    original = base64.b64decode(values[4]['data'])
    changed = base64.b64decode(values[7]['data'])
    undone = base64.b64decode(values[10]['data'])
    reopened = base64.b64decode(values[19]['data'])
    assert density(original) == raw and density(undone) == raw and density(reopened) == raw
    assert density(changed) == (23622, 23622, 1), 'Expected rounded 600 PPI metric density'
    assert pixels(original) == pixels(changed), 'Resolution must not resample pixels'
    assert original == undone == reopened == exported.read_bytes()
    assert values[17]['valid'], values[17]
    return reopened


def registered_command(action_id):
    # Plugin commands have stable identities shared by command search and bindings.
    value = 2166136261
    for byte in ('plugin.' + action_id).encode('ascii'):
        value = ((value ^ byte) * 16777619) & 0xffffffff
    return value | 0x80000000


def main():
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument('--cli', default='dist/micopi-cli.exe')
    parser.add_argument('--gui', default='dist/MiCoPi.exe')
    parser.add_argument('--skip-gui', action='store_true')
    args = parser.parse_args()
    cli, gui = (ROOT / args.cli).resolve(), (ROOT / args.gui).resolve()
    output = ROOT / 'target/test-output/png-density-smoke' / str(time.time_ns())
    output.mkdir(parents=True)
    plugins, profile = output / 'plugins', output / 'profile'
    shutil.copytree(ROOT / 'plugins', plugins)
    profile.mkdir()
    locale_dir = profile / 'locales'
    locale_dir.mkdir()
    for language in ['zh-CN', 'ja', 'ko']:
        shutil.copy2(ROOT / 'locales' / f'{language}.json', locale_dir / f'{language}.json')
    settings = profile / 'settings.json'
    settings.write_text(json.dumps(dict(version=1, custom=dict(keep=42))), encoding='utf-8')
    flags = ['--plugins', str(plugins), '--config-dir', str(profile)]
    rgba = bytes(channel for y in range(HEIGHT) for x in range(WIDTH)
                 for channel in [(x * 3) & 255, (y * 5) & 255, (x + y) & 255, 255])
    sources = {}
    for name, raw in CASES:
        data = rgba_fixture(WIDTH, HEIGHT, rgba, raw)
        assert pixels(data) == (WIDTH, HEIGHT, rgba)
        source = output / f'{name}.data'
        source.write_bytes(data)
        sources[name] = source
    # A valid CRC does not make a misplaced pHYs valid; reject after IDAT.
    invalid = output / 'late-density.png'
    base = rgba_fixture(WIDTH, HEIGHT, rgba)
    items = chunks(base)
    invalid.write_bytes(SIGNATURE + b''.join(chunk(kind, payload) for kind, payload in items[:-1])
                        + chunk(b'pHYs', b'\0\0\x0f\x61\0\0\x1e\xc2\1') + chunk(*items[-1]))
    batch, locations = [], {}
    for name, raw in CASES:
        start = len(batch)
        exported = output / f'batch-{name}.png'
        batch.extend(commands(sources[name], output / f'batch-{name}.mcp', exported))
        locations[name] = (start, exported)
    script = output / 'density.jsonl'
    script.write_text('\n'.join(json.dumps(c) for c in batch) + '\n', encoding='utf-8')
    run = subprocess.run([str(cli), 'run', str(script), *flags], capture_output=True, text=True, encoding='utf-8', timeout=60)
    assert run.returncode == 0, run.stderr + run.stdout[-2000:]
    records = [json.loads(line) for line in run.stdout.splitlines()]
    assert len(records) == len(batch) and all(r['ok'] for r in records), records[-3:]
    values = [r['data']['result'] for r in records]
    expected = {}
    for name, raw in CASES:
        start, exported = locations[name]
        expected[name] = validate(values[start:start + 20], raw, exported)

    rollback = dict(atomic=True, steps=[dict(id='resolution', action='canvas.resolution', args=dict(resolution_ppi=300)),
                                       dict(id='fail', action='layer.select', args=dict(layer_id=99999))])
    requests = [dict(jsonrpc='2.0', id=1, method='initialize', params=dict(protocolVersion='2025-11-25')),
                dict(jsonrpc='2.0', method='notifications/initialized')]
    blocks = {}
    for name, raw in CASES:
        rows = commands(sources[name], output / f'mcp-{name}.mcp', output / f'mcp-{name}.png')
        start = len(requests) + 10
        blocks[name] = (start, output / f'mcp-{name}.png')
        requests.extend(request(start + i, c['action'], c['args']) for i, c in enumerate(rows))
    archive = output / 'density.mcreplay'
    requests.extend([request(900, 'document.open', dict(path=str(invalid))), request(901, 'canvas.info', {}),
                     request(902, 'canvas.snapshot', {}), request(903, 'pipeline.run', rollback),
                     request(904, 'canvas.info', {}), request(905, 'canvas.snapshot', {}),
                     request(906, 'replay.verify', {}), request(907, 'replay.export', dict(path=str(archive)))])
    run = subprocess.run([str(cli), 'mcp', *flags], input='\n'.join(json.dumps(r) for r in requests) + '\n',
                         capture_output=True, text=True, encoding='utf-8', timeout=60)
    assert run.returncode == 0, run.stderr + run.stdout[-2000:]
    replies = {r['id']: r for r in map(json.loads, run.stdout.splitlines()) if 'id' in r}
    for name, raw in CASES:
        start, exported = blocks[name]
        assert validate([content(replies[start + i]) for i in range(20)], raw, exported) == expected[name]
    assert replies[900]['result']['isError'], 'Late density must reject before document replacement'
    for number in [901, 904]:
        info_matches(content(replies[number]), CASES[-1][1])
    for number in [902, 905]:
        assert base64.b64decode(content(replies[number])['data']) == expected[CASES[-1][0]]
    failed = content(replies[903], allow_failure=True)
    assert failed['rolled_back'] and not failed['ok'], failed
    assert content(replies[906])['valid']
    minimal = output / 'minimal-plugins'
    minimal.mkdir()
    for name in ['core', 'replay']:
        shutil.copytree(ROOT / 'plugins' / name, minimal / name)
    replay_requests = requests[:2] + [request(2, 'replay.load', dict(path=str(archive))),
                                       request(3, 'replay.verify', dict(source='loaded')),
                                       request(4, 'replay.snapshot', dict(source='loaded'))]
    run = subprocess.run([str(cli), 'mcp', '--plugins', str(minimal), '--config-dir', str(profile)],
                         input='\n'.join(json.dumps(r) for r in replay_requests) + '\n', capture_output=True,
                         text=True, encoding='utf-8', timeout=60)
    assert run.returncode == 0, run.stderr + run.stdout[-2000:]
    playback = {r['id']: r for r in map(json.loads, run.stdout.splitlines()) if 'id' in r}
    assert content(playback[3])['valid']
    assert base64.b64decode(content(playback[4])['data']) == expected[CASES[-1][0]]
    print('JSONL/MCP: raw density, unchanged pixels, undo/redo, projects, rollback, invalid import and DLL-independent replay passed.')
    if args.skip_gui:
        print(output)
        return
    startup = subprocess.STARTUPINFO()
    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = 0
    process = subprocess.Popen([str(gui), *flags], startupinfo=startup)
    endpoint = profile / 'session.json'

    def call(name, arguments, allow_failure=False):
        run = subprocess.run([str(cli), 'call', name, json.dumps(arguments), '--attach', str(endpoint)],
                             capture_output=True, text=True, encoding='utf-8', timeout=30)
        assert allow_failure or run.returncode == 0, run.stderr + run.stdout
        return json.loads(run.stdout)['result']

    def capture(name):
        path = output / name
        call('ui.capture', dict(path=str(path)))
        deadline = time.monotonic() + 10
        while not path.exists():
            assert time.monotonic() < deadline, 'Capture timeout'
            time.sleep(.05)

    try:
        deadline = time.monotonic() + 20
        while not endpoint.exists():
            assert process.poll() is None and time.monotonic() < deadline, 'GUI startup failed'
            time.sleep(.05)
        for name, raw in CASES:
            exported = output / f'gui-{name}.png'
            rows = commands(sources[name], output / f'gui-{name}.mcp', exported)
            values = [call(c['action'], c['args']) for c in rows]
            assert validate(values, raw, exported) == expected[name]
        result = call('pipeline.run', rollback, allow_failure=True)
        assert result['rolled_back'] and not result['ok'], result
        info_matches(call('canvas.info', {}), CASES[-1][1])
        for language in ['en', 'zh-CN', 'ja', 'ko']:
            call('settings.update', dict(path=str(settings), ui=dict(language=language)))
            assert json.loads(settings.read_text(encoding='utf-8-sig'))['custom']['keep'] == 42
            call('document.open', dict(path=str(sources['unequal'])))
            before = call('canvas.info', {})
            png = base64.b64decode(call('canvas.snapshot', {})['data'])
            call('ui.dispatch', dict(id='command', value=dict(UiCommand=registered_command('canvas.resolution'))))
            time.sleep(.3)
            capture(f'{language}-png-resolution.png')
            assert call('canvas.info', {}) == before
            assert base64.b64decode(call('canvas.snapshot', {})['data']) == png
            call('ui.dispatch', dict(id='command', value='Cancel'))
            time.sleep(.1)
            assert call('canvas.info', {}) == before
            assert base64.b64decode(call('canvas.snapshot', {})['data']) == png
        assert call('replay.verify', {})['valid']
        call('ui.dispatch', dict(id='close', value=True))
        assert process.wait(timeout=10) == 0
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=10)
    print('GUI density parity and four-language resolution-form captures/cancel passed.')
    print(output)


if __name__ == '__main__':
    main()
