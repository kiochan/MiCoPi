"""Verify non-destructive export sizing across JSONL, MCP, desktop and replay.

This check uses independent RGBA8 references, actual files, isolated profiles and
only test-owned GUI processes. Synthetic pen fixtures are not hardware evidence.
"""
import argparse
import base64
import json
import hashlib
from pathlib import Path
import shutil
import subprocess
import time

from png_checks import density, pixels, rgba_fixture

ROOT = Path(__file__).resolve().parents[1]
WIDTH, HEIGHT = 96, 64
RAW_DENSITY = (3937, 7874, 1)


def action(name, **args):
    return dict(action=name, args=args)


def request(number, name, arguments):
    return dict(jsonrpc='2.0', id=number, method='tools/call', params=dict(name=name, arguments=arguments))


def content(reply, allow_failure=False):
    assert 'error' not in reply, reply
    result = reply['result']
    assert allow_failure or not result.get('isError'), result
    return result['structuredContent']['result']


def commands(source, project, exported):
    return [
        action('document.open', path=str(source)),
        action('brush.set', color='#994477', radius=3, stabilizer=.35,
               pressure_size=True, pressure_opacity=True),
        action('canvas.stroke', points=[dict(x=8, y=48, pressure=.2, pen=True, time=0),
                                       dict(x=42, y=15, pressure=1., pen=True, time=40),
                                       dict(x=88, y=50, pressure=.4, pen=True, time=80)]),
        action('selection.shape', kind='rectangle', x=0, y=0, width=1, height=1),
        action('canvas.state'),
        action('canvas.snapshot'),
        action('canvas.export.plan', width=192, method='nearest'),
        action('canvas.export.preview', width=192, method='nearest'),
        action('canvas.snapshot', width=192, method='nearest'),
        action('canvas.export', path=str(exported), width=192, method='nearest'),
        action('canvas.state'),
        action('canvas.export.preview', width=50.8, height=25.4, unit='mm',
               lock_aspect=False, resolution_ppi=100, method='bilinear'),
        action('canvas.export.preview', scale=50),
        action('canvas.export.preview', width=96, height=64, lock_aspect=False),
        action('replay.verify'),
        action('document.save', path=str(project)),
        action('document.open', path=str(project)),
        action('canvas.snapshot'),
        action('canvas.state'),
    ]


def nearest_twice(source):
    width, height, rgba = pixels(source)
    result = bytearray()
    for y in range(height * 2):
        for x in range(width * 2):
            start = ((y // 2) * width + x // 2) * 4
            result.extend(rgba[start:start + 4])
    return width * 2, height * 2, bytes(result)


def validate(values, exported, desktop=False):
    before, after = values[4], values[10]
    original = base64.b64decode(values[5]['data'])
    preview = base64.b64decode(values[7]['data'])
    assert values[6]['width'] == 192 and values[6]['height'] == 128
    assert values[6]['source_width'] == WIDTH and values[6]['source_height'] == HEIGHT
    assert values[6]['method'] == 'nearest' and not values[6]['identity']
    assert values[6]['estimated_output_bytes'] == 192 * 128 * 4
    assert pixels(preview) == nearest_twice(original), 'Native export must sample the full merged frame'
    assert preview == base64.b64decode(values[8]['data']) == exported.read_bytes()
    assert values[9]['sha256'] == hashlib.sha256(exported.read_bytes()).hexdigest()
    assert density(original) == density(preview) == RAW_DENSITY
    if desktop:
        # Local fit-view/window reporting can settle between attached requests.
        # Headless runs below retain the stricter exact revision/view assertion.
        semantic = lambda state: {key: value for key, value in state.items() if key not in ['revision', 'view', 'ui']}
        assert semantic(after) == semantic(before)
        assert after['revision'] >= before['revision'] + 1
    else:
        changed = dict(before)
        changed['revision'] += 1
        assert after == changed, 'Export must retain source file, pixels, selection and document metadata'
    physical = base64.b64decode(values[11]['data'])
    assert pixels(physical)[:2] == (200, 100)
    assert density(physical) == (3937, 3937, 1)
    assert values[11]['document_info']['resolution_ppi'] == 100
    reduced = base64.b64decode(values[12]['data'])
    assert pixels(reduced)[:2] == (48, 32) and density(reduced) == RAW_DENSITY
    assert base64.b64decode(values[13]['data']) == original
    assert values[14]['valid']
    assert base64.b64decode(values[17]['data']) == original
    assert values[18]['width'] == WIDTH and values[18]['height'] == HEIGHT
    assert values[18]['document_info'] == before['document_info']
    return original, preview


def main():
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument('--cli', default='dist/micopi-cli.exe')
    parser.add_argument('--gui', default='dist/MiCoPi.exe')
    parser.add_argument('--skip-gui', action='store_true')
    args = parser.parse_args()
    cli, gui = (ROOT / args.cli).resolve(), (ROOT / args.gui).resolve()
    output = ROOT / 'target/test-output/export-smoke' / str(time.time_ns())
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
    source = output / 'source.data'
    source.write_bytes(rgba_fixture(WIDTH, HEIGHT, rgba, RAW_DENSITY))
    exported = output / 'batch.png'
    batch = commands(source, output / 'batch.mcp', exported)
    script = output / 'export.jsonl'
    script.write_text('\n'.join(json.dumps(c) for c in batch) + '\n', encoding='utf-8')
    run = subprocess.run([str(cli), 'run', str(script), *flags], capture_output=True,
                         text=True, encoding='utf-8', timeout=60)
    assert run.returncode == 0, run.stderr + run.stdout[-3000:]
    records = [json.loads(line) for line in run.stdout.splitlines()]
    assert len(records) == len(batch) and all(r['ok'] for r in records), records[-3:]
    original, expected = validate([r['data']['result'] for r in records], exported)

    requests = [dict(jsonrpc='2.0', id=1, method='initialize', params=dict(protocolVersion='2025-11-25')),
                dict(jsonrpc='2.0', method='notifications/initialized'),
                dict(jsonrpc='2.0', id=2, method='tools/list', params={})]
    requests += [request(10 + i, c['action'], c['args'])
                 for i, c in enumerate(commands(source, output / 'mcp.mcp', output / 'mcp.png'))]
    protected = output / 'mcp.mcp'
    archive = output / 'export.mcreplay'
    requests += [request(100, 'canvas.export', dict(path=str(protected), width=192)),
                 request(101, 'canvas.state', {}),
                 request(102, 'pipeline.run', dict(atomic=True, steps=[dict(id='export', action='canvas.export', args=dict(path=str(output / 'unsafe.png')))])),
                 request(103, 'replay.verify', {}), request(104, 'replay.export', dict(path=str(archive))),
                 request(105, 'replay.events', dict(start=0, limit=128))]
    run = subprocess.run([str(cli), 'mcp', *flags], input='\n'.join(json.dumps(r) for r in requests) + '\n',
                         capture_output=True, text=True, encoding='utf-8', timeout=60)
    assert run.returncode == 0, run.stderr + run.stdout[-3000:]
    replies = {r['id']: r for r in map(json.loads, run.stdout.splitlines()) if 'id' in r}
    tools = replies[2]['result']['tools']
    assert all(any(t['name'] == name for t in tools)
               for name in ['canvas.export.plan', 'canvas.export.preview', 'canvas.export'])
    assert validate([content(replies[10 + i]) for i in range(len(batch))], output / 'mcp.png') == (original, expected)
    assert replies[100]['result']['isError']
    assert protected.read_bytes().startswith(b'MICOPI03')
    assert content(replies[101])['width'] == WIDTH
    assert replies[102]['result']['isError'] and not (output / 'unsafe.png').exists()
    assert content(replies[103])['valid']
    artifacts = [event['source'].get('context', {}).get('export') for event in content(replies[105])['events']]
    assert any(item and item['path'] == str(output / 'mcp.png') and item['sha256'] == hashlib.sha256(expected).hexdigest() for item in artifacts)
    # Recorded outcomes retain the source frame; playback never repeats file effects.
    (output / 'mcp.png').unlink()
    minimal = output / 'minimal-plugins'
    minimal.mkdir()
    for name in ['core', 'replay']:
        shutil.copytree(ROOT / 'plugins' / name, minimal / name)
    replay_requests = requests[:2] + [request(3, 'replay.load', dict(path=str(archive))),
                                       request(4, 'replay.verify', dict(source='loaded')),
                                       request(5, 'replay.snapshot', dict(source='loaded')),
                                       request(6, 'replay.events', dict(source='loaded', start=0, limit=128))]
    run = subprocess.run([str(cli), 'mcp', '--plugins', str(minimal), '--config-dir', str(profile)],
                         input='\n'.join(json.dumps(r) for r in replay_requests) + '\n', capture_output=True,
                         text=True, encoding='utf-8', timeout=60)
    assert run.returncode == 0, run.stderr + run.stdout[-2000:]
    playback = {r['id']: r for r in map(json.loads, run.stdout.splitlines()) if 'id' in r}
    assert content(playback[4])['valid']
    assert base64.b64decode(content(playback[5])['data']) == original
    assert not (output / 'mcp.png').exists()
    restored = [event['source'].get('context', {}).get('export') for event in content(playback[6])['events']]
    assert any(item and item['sha256'] == hashlib.sha256(expected).hexdigest() for item in restored)
    print('JSONL/MCP: native export pixels, physical units, density, unchanged source, file guards and DLL-independent replay passed.')
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
        assert allow_failure or run.returncode == 0, run.stderr + run.stdout[-2000:]
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
        values = [call(c['action'], c['args'])
                  for c in commands(source, output / 'gui.mcp', output / 'gui.png')]
        assert validate(values, output / 'gui.png', desktop=True) == (original, expected)
        for language in ['en', 'zh-CN', 'ja', 'ko']:
            call('settings.update', dict(path=str(settings), ui=dict(language=language)))
            assert json.loads(settings.read_text(encoding='utf-8-sig'))['custom']['keep'] == 42
            before = call('canvas.info', {})
            png = base64.b64decode(call('canvas.snapshot', {})['data'])
            call('ui.dispatch', dict(id='command', value='Export'))
            time.sleep(.3)
            capture(f'{language}-export.png')
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
    print('GUI export parity and four-language options/cancel captures passed.')
    print(output)


if __name__ == '__main__':
    main()
