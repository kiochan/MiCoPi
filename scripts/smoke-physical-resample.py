"""Check physical Image Size plans and outcomes across JSONL, MCP and desktop.

Uses small non-square-density fixtures and an independent nearest reference.
Recording playback runs with no painting DLL. GUI captures exercise opening and
cancelling the real Image Size dialog in all four supported languages.
"""
import argparse
import base64
import json
from pathlib import Path
import shutil
import subprocess
import time

from png_checks import density, pixels, rgba_fixture

ROOT = Path(__file__).resolve().parents[1]
SIZE = dict(width=192, unit='mm', method='nearest')
GRAPH = dict(version=1, nodes=[
    dict(id=1, type='action.canvas.image_size.plan', position=[0, 0], parameters=SIZE),
    dict(id=2, type='action.canvas.image_size', position=[400, 0],
         parameters=dict(lock_aspect=False, method='nearest')),
], edges=[])
# The wire key is "from", which is a Python keyword.
GRAPH['edges'] = [dict(**{'from': 1}, output=axis, to=2, input=axis) for axis in ['width', 'height']]


def action(name, **args):
    return dict(action=name, args=args)


def request(number, name, arguments):
    return dict(jsonrpc='2.0', id=number, method='tools/call', params=dict(name=name, arguments=arguments))


def content(reply):
    assert 'error' not in reply, reply
    result = reply['result']
    assert not result.get('isError'), result
    return result['structuredContent']['result']


def commands(source, output, prefix):
    return [
        action('document.open', path=str(source)),
        action('brush.set', radius=3, color='#445588', stabilizer=.35,
               pressure_size=True, pressure_opacity=True),
        action('canvas.stroke', points=[dict(x=8, y=50, pressure=.2, pen=True, time=0),
                                       dict(x=42, y=14, pressure=1., pen=True, time=40),
                                       dict(x=88, y=46, pressure=.4, pen=True, time=80)]),
        action('layer.duplicate'),
        action('layer.set', opacity=.4),
        action('selection.shape', kind='rectangle', x=8, y=6, width=60, height=40),
        action('selection.deselect'),
        action('selection.shape', kind='ellipse', x=16, y=10, width=52, height=38),
        action('workflow.set', graph=GRAPH),
        action('workflow.run', atomic=True, dry_run=True),
        action('canvas.info'),
        action('canvas.snapshot'),
        action('canvas.state'),
        action('canvas.image_size.plan', **SIZE),
        action('canvas.state'),
        action('canvas.image_size', **SIZE),
        action('canvas.info'),
        action('canvas.snapshot'),
        action('history.undo'),
        action('canvas.snapshot'),
        action('workflow.run', atomic=True),
        action('canvas.snapshot'),
        action('history.undo'),
        action('canvas.snapshot'),
        action('history.redo'),
        action('canvas.snapshot'),
        action('document.save', path=str(output / f'{prefix}.mcp')),
        action('document.open', path=str(output / f'{prefix}.mcp')),
        action('canvas.info'),
        action('selection.read'),
        action('replay.verify'),
        action('canvas.snapshot'),
    ]


def nearest_twice(png):
    width, height, rgba = pixels(png)
    result = bytearray()
    for y in range(height * 2):
        for x in range(width * 2):
            at = ((y // 2) * width + x // 2) * 4
            result.extend(rgba[at:at + 4])
    return width * 2, height * 2, bytes(result)


def validate(values, desktop=False):
    assert values[9]['ok'], 'Editing or validating a workflow must not execute it'
    assert values[10]['width'] == 96 and values[10]['height'] == 64
    before, after = values[12], values[14]
    if desktop:
        semantic = lambda state: {k: v for k, v in state.items() if k not in ['ui', 'view', 'revision']}
        assert semantic(before) == semantic(after)
    else:
        assert before == after, 'Planning changed the source state'
    plan = values[13]
    assert (plan['source_width'], plan['source_height'], plan['width'], plan['height']) == (96, 64, 192, 128), plan
    assert plan['method'] == 'nearest' and not plan['identity'] and not plan['drawing'], plan
    assert plan['layer_count'] == 2 and plan['mask_count'] == 2, plan
    assert plan['estimated_layer_bytes'] == 192 * 128 * 8
    assert plan['estimated_mask_bytes'] == 192 * 128 * 2
    assert plan['estimated_composite_bytes'] == 192 * 128 * 4
    assert plan['estimated_staged_bytes'] == 192 * 128 * 10
    for info in [values[16], values[28], plan['document_info']]:
        assert info['width'] == 192 and info['height'] == 128, info
        assert info['resolution_source'] == 'png' and info['physical_size_known'], info
        assert info['png_density'] == dict(xppu=1000, yppu=2000, unit='meter'), info
        assert abs(info['resolution_ppi_x'] - 25.4) < 1e-9
        assert abs(info['resolution_ppi_y'] - 50.8) < 1e-9
        assert abs(info['width_mm'] - 192) < 1e-9
        assert abs(info['height_mm'] - 64) < 1e-9
    original = base64.b64decode(values[11]['data'])
    resized = base64.b64decode(values[17]['data'])
    assert pixels(original)[:2] == (96, 64)
    assert pixels(resized) == nearest_twice(original), 'Nearest output differs from independent whole-image reference'
    assert density(original) == density(resized) == (1000, 2000, 1)
    assert base64.b64decode(values[19]['data']) == original
    assert values[20]['ok']
    assert base64.b64decode(values[21]['data']) == resized
    assert base64.b64decode(values[23]['data']) == original
    assert base64.b64decode(values[25]['data']) == resized
    assert values[30]['valid']
    assert base64.b64decode(values[31]['data']) == resized
    return original, resized


def run_mcp(cli, flags, requests):
    run = subprocess.run([str(cli), 'mcp', *flags],
                         input='\n'.join(json.dumps(r) for r in requests) + '\n', capture_output=True,
                         text=True, encoding='utf-8', timeout=90)
    assert run.returncode == 0, run.stderr + run.stdout[-3000:]
    return {r['id']: r for r in map(json.loads, run.stdout.splitlines()) if 'id' in r}


def main():
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument('--cli', default='dist/micopi-cli.exe')
    parser.add_argument('--gui', default='dist/MiCoPi.exe')
    parser.add_argument('--skip-gui', action='store_true')
    args = parser.parse_args()
    cli, gui = (ROOT / args.cli).resolve(), (ROOT / args.gui).resolve()
    output = ROOT / 'target/test-output/physical-resample-smoke' / str(time.time_ns())
    output.mkdir(parents=True)
    source = output / 'unequal-density.png'
    rgba = bytes(channel for y in range(64) for x in range(96)
                 for channel in [(x * 17 + y * 3) & 255, (x * 5 + y * 11) & 255, (x + y * 7) & 255, 255])
    source.write_bytes(rgba_fixture(96, 64, rgba, (1000, 2000, 1)))
    unknown = output / 'unknown-density.png'
    unknown.write_bytes(rgba_fixture(96, 64, rgba, (4, 3, 0)))
    plugins, profile = output / 'plugins', output / 'profile'
    shutil.copytree(ROOT / 'plugins', plugins)
    profile.mkdir()
    locales = profile / 'locales'
    locales.mkdir()
    for language in ['zh-CN', 'ja', 'ko']:
        shutil.copy2(ROOT / 'locales' / f'{language}.json', locales / f'{language}.json')
    settings = profile / 'settings.json'
    settings.write_text(json.dumps(dict(version=1, custom=dict(keep=42))), encoding='utf-8')
    flags = ['--plugins', str(plugins), '--config-dir', str(profile)]
    rows = commands(source, output, 'batch')
    script = output / 'physical-resample.jsonl'
    script.write_text('\n'.join(json.dumps(row) for row in rows) + '\n', encoding='utf-8')
    run = subprocess.run([str(cli), 'run', str(script), *flags], capture_output=True,
                         text=True, encoding='utf-8', timeout=90)
    assert run.returncode == 0, run.stderr + run.stdout[-3000:]
    records = [json.loads(line) for line in run.stdout.splitlines()]
    assert len(records) == len(rows) and all(r['ok'] for r in records), records[-3:]
    expected = validate([r['data']['result'] for r in records])
    initialized = [dict(jsonrpc='2.0', id=1, method='initialize', params=dict(protocolVersion='2025-11-25')),
                   dict(jsonrpc='2.0', method='notifications/initialized')]
    requests = initialized + [dict(jsonrpc='2.0', id=2, method='tools/list', params={})]
    mcp_rows = commands(source, output, 'mcp')
    requests += [request(10 + i, row['action'], row['args']) for i, row in enumerate(mcp_rows)]
    archive = output / 'physical-resample.mcreplay'
    requests += [request(900, 'node.catalog', {}), request(901, 'replay.export', dict(path=str(archive))),
                 request(902, 'canvas.image_size', dict(width=2.5)), request(903, 'canvas.snapshot', {}),
                 request(904, 'document.open', dict(path=str(unknown))),
                 request(905, 'canvas.info', {}), request(906, 'canvas.image_size.plan', SIZE),
                 request(907, 'canvas.image_size', SIZE), request(908, 'canvas.info', {}),
                 request(909, 'canvas.image_size.plan', dict(width=192, method='nearest')),
                 request(910, 'canvas.resolution', dict(resolution_ppi=100)),
                 request(911, 'canvas.image_size.plan', dict(width=25.4, unit='mm'))]
    replies = run_mcp(cli, flags, requests)
    for name in ['canvas.image_size', 'canvas.image_size.plan']:
        tool = next(t for t in replies[2]['result']['tools'] if t['name'] == name)
        properties = tool['inputSchema']['properties']
        assert properties['width']['type'] == 'number'
        assert set(properties['unit']['enum']) == {'px', 'mm', 'in'}
        assert not {'resolution_ppi', 'scale_percent'} & properties.keys()
    assert validate([content(replies[10 + i]) for i in range(len(mcp_rows))]) == expected
    assert 'action.canvas.image_size.plan' in json.dumps(content(replies[900]))
    assert replies[902]['result']['isError']
    assert base64.b64decode(content(replies[903])['data']) == expected[1]
    assert replies[906]['result']['isError'] and replies[907]['result']['isError']
    assert content(replies[905]) == content(replies[908])
    assert not content(replies[909])['document_info']['physical_size_known']
    assert (content(replies[911])['width'], content(replies[911])['height']) == (100, 67)
    minimal = output / 'minimal-plugins'
    minimal.mkdir()
    for name in ['core', 'replay']:
        shutil.copytree(ROOT / 'plugins' / name, minimal / name)
    playback = run_mcp(cli, ['--plugins', str(minimal), '--config-dir', str(profile)], initialized + [
        request(3, 'replay.load', dict(path=str(archive))),
        request(4, 'replay.verify', dict(source='loaded')),
        request(5, 'replay.snapshot', dict(source='loaded')),
    ])
    assert content(playback[4])['valid']
    assert base64.b64decode(content(playback[5])['data']) == expected[1]
    print('JSONL/MCP: physical sizing, reference pixels, mask estimates, workflow, density, projects and DLL-independent replay passed.')
    if args.skip_gui:
        print(output)
        return
    startup = subprocess.STARTUPINFO()
    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = 0
    process = subprocess.Popen([str(gui), *flags], startupinfo=startup)
    endpoint = profile / 'session.json'

    def call(name, arguments):
        run = subprocess.run([str(cli), 'call', name, json.dumps(arguments), '--attach', str(endpoint)],
                             capture_output=True, text=True, encoding='utf-8', timeout=30)
        assert run.returncode == 0, run.stderr + run.stdout[-2000:]
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
        gui_rows = commands(source, output, 'gui')
        assert validate([call(row['action'], row['args']) for row in gui_rows], desktop=True) == expected
        for language in ['en', 'zh-CN', 'ja', 'ko']:
            call('settings.update', dict(path=str(settings), ui=dict(language=language)))
            assert json.loads(settings.read_text(encoding='utf-8-sig'))['custom']['keep'] == 42
            before = call('canvas.info', {})
            png = base64.b64decode(call('canvas.snapshot', {})['data'])
            call('ui.dispatch', dict(id='command', value=dict(UiCommand=10173)))
            time.sleep(.3)
            capture(f'{language}-physical-image-size.png')
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
    print('GUI physical-sizing parity and four-language Image Size captures/cancel passed.')
    print(output)


if __name__ == '__main__':
    main()
