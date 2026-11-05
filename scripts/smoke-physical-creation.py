"""Check physical-unit creation plans and outcomes across JSONL, MCP and desktop.

A4 is planned only; small actual canvases verify pixels, density, native projects,
recorded outcomes and four-language dialog cancellation in isolated profiles.
"""
import argparse
import base64
import json
from pathlib import Path
import shutil
import subprocess
import time

from png_checks import density, pixels

ROOT = Path(__file__).resolve().parents[1]
CASES = [
    ('metric', dict(width=25.4, height=12.7, unit='mm', resolution_ppi=100), 100, 50, 100, True),
    ('inches', dict(width=1, height=.5, unit='in', resolution_ppi=100.0), 100, 50, 100, True),
    ('default-physical', dict(width=1, height=.5, unit='in'), 72, 36, 72, True),
    ('legacy', dict(width=100.0, height=50.0), 100, 50, 72, False),
]
A4 = dict(width=210, height=297, unit='mm', resolution_ppi=300.0, background='#FFFFFF')


def action(name, **args):
    return dict(action=name, args=args)


def request(number, name, arguments):
    return dict(jsonrpc='2.0', id=number, method='tools/call', params=dict(name=name, arguments=arguments))


def content(reply):
    assert 'error' not in reply, reply
    result = reply['result']
    assert not result.get('isError'), result
    return result['structuredContent']['result']


def commands(output, prefix):
    rows = [action('canvas.state'), action('canvas.new.plan', **A4), action('canvas.state')]
    positions = {}
    for name, args, width, height, _, _ in CASES:
        args = dict(args, background='#F8EFE0')
        positions[name] = len(rows)
        rows += [
            action('canvas.new.plan', **args),
            action('canvas.new', **args),
            action('canvas.info'),
            action('brush.set', radius=2, color='#445588', stabilizer=.35,
                   pressure_size=True, pressure_opacity=True),
            action('canvas.stroke', points=[dict(x=width*.1, y=height*.75, pressure=.2, pen=True, time=0),
                                           dict(x=width*.45, y=height*.25, pressure=1., pen=True, time=40),
                                           dict(x=width*.9, y=height*.7, pressure=.4, pen=True, time=80)]),
            action('canvas.snapshot'),
            action('document.save', path=str(output / f'{prefix}-{name}.mcp')),
            action('document.open', path=str(output / f'{prefix}-{name}.mcp')),
            action('canvas.info'),
            action('replay.verify'),
            action('canvas.snapshot'),
        ]
    return rows, positions


def a4(plan):
    assert plan['width'] == 2480 and plan['height'] == 3508, plan
    assert plan['estimated_initial_pixel_bytes'] == 2480 * 3508 * 8, plan
    assert plan['document_info']['resolution_ppi'] == 300 and not plan['drawing'], plan


def validate(values, positions, desktop=False):
    a4(values[1])
    if desktop:
        semantic = lambda state: {k: v for k, v in state.items() if k not in ['ui', 'view', 'revision']}
        assert semantic(values[0]) == semantic(values[2])
    else:
        assert values[0] == values[2], 'Planning must not create or modify a document'
    result = {}
    for name, _, width, height, ppi, explicit in CASES:
        rows = values[positions[name]:positions[name]+11]
        plan = rows[0]
        assert plan['width'] == width and plan['height'] == height
        assert plan['estimated_initial_pixel_bytes'] == width * height * 8 and not plan['drawing']
        for info in [plan['document_info'], rows[2], rows[8]]:
            assert info['width'] == width and info['height'] == height, info
            assert info['resolution_ppi'] == ppi and info['initial_background'] == '#F8EFE0', info
            assert info.get('resolution_explicit', False) == explicit, info
            assert info['physical_size_known'] and info['resolution_source'] == 'document', info
            assert abs(info['width_mm'] - width * 25.4 / ppi) < 1e-9, info
            assert abs(info['height_mm'] - height * 25.4 / ppi) < 1e-9, info
        before = base64.b64decode(rows[5]['data'])
        after = base64.b64decode(rows[10]['data'])
        assert before == after and rows[9]['valid'], 'Native project/replay must retain pixels and density'
        assert pixels(before)[:2] == (width, height)
        expected_density = ((ppi * 10000 + 127) // 254,) * 2 + (1,) if explicit else None
        assert density(before) == expected_density, (name, density(before), expected_density)
        result[name] = before
    assert result['metric'] == result['inches'], 'Equivalent physical dimensions must create the same drawing'
    return result


def main():
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument('--cli', default='dist/micopi-cli.exe')
    parser.add_argument('--gui', default='dist/MiCoPi.exe')
    parser.add_argument('--skip-gui', action='store_true')
    args = parser.parse_args()
    cli, gui = (ROOT / args.cli).resolve(), (ROOT / args.gui).resolve()
    output = ROOT / 'target/test-output/physical-creation-smoke' / str(time.time_ns())
    output.mkdir(parents=True)
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
    rows, positions = commands(output, 'batch')
    script = output / 'physical-creation.jsonl'
    script.write_text('\n'.join(json.dumps(row) for row in rows) + '\n', encoding='utf-8')
    run = subprocess.run([str(cli), 'run', str(script), *flags], capture_output=True, text=True, encoding='utf-8', timeout=60)
    assert run.returncode == 0, run.stderr + run.stdout[-3000:]
    records = [json.loads(line) for line in run.stdout.splitlines()]
    assert len(records) == len(rows) and all(r['ok'] for r in records), records[-3:]
    expected = validate([r['data']['result'] for r in records], positions)

    requests = [dict(jsonrpc='2.0', id=1, method='initialize', params=dict(protocolVersion='2025-11-25')),
                dict(jsonrpc='2.0', method='notifications/initialized'),
                dict(jsonrpc='2.0', id=2, method='tools/list', params={})]
    mcp_rows, mcp_positions = commands(output, 'mcp')
    requests += [request(10+i, c['action'], c['args']) for i, c in enumerate(mcp_rows)]
    archive = output / 'physical-creation.mcreplay'
    requests += [request(900, 'canvas.new', dict(width=None, height=1, unit='mm')),
                 request(901, 'canvas.info', {}), request(902, 'canvas.snapshot', {}),
                 request(903, 'canvas.new.plan', A4), request(904, 'replay.verify', {}),
                 request(905, 'replay.export', dict(path=str(archive)))]
    run = subprocess.run([str(cli), 'mcp', *flags], input='\n'.join(json.dumps(r) for r in requests)+'\n',
                         capture_output=True, text=True, encoding='utf-8', timeout=60)
    assert run.returncode == 0, run.stderr + run.stdout[-3000:]
    replies = {r['id']: r for r in map(json.loads, run.stdout.splitlines()) if 'id' in r}
    for name in ['canvas.new', 'canvas.new.plan']:
        tool = next(t for t in replies[2]['result']['tools'] if t['name'] == name)
        assert tool['inputSchema']['properties']['width']['type'] == 'number'
        assert set(tool['inputSchema']['properties']['unit']['enum']) == {'px', 'mm', 'in'}
        assert {'width', 'height'}.issubset(tool['inputSchema']['required'])
    assert validate([content(replies[10+i]) for i in range(len(mcp_rows))], mcp_positions) == expected
    assert replies[900]['result']['isError']
    assert content(replies[901])['width'] == 100 and content(replies[901])['height'] == 50
    assert base64.b64decode(content(replies[902])['data']) == expected['legacy']
    a4(content(replies[903]))
    assert content(replies[904])['valid']
    minimal = output / 'minimal-plugins'
    minimal.mkdir()
    for name in ['core', 'replay']:
        shutil.copytree(ROOT / 'plugins' / name, minimal / name)
    replay_requests = requests[:2] + [request(3, 'replay.load', dict(path=str(archive))),
                                       request(4, 'replay.verify', dict(source='loaded')),
                                       request(5, 'replay.snapshot', dict(source='loaded'))]
    run = subprocess.run([str(cli), 'mcp', '--plugins', str(minimal), '--config-dir', str(profile)],
                         input='\n'.join(json.dumps(r) for r in replay_requests)+'\n', capture_output=True,
                         text=True, encoding='utf-8', timeout=60)
    assert run.returncode == 0, run.stderr + run.stdout[-2000:]
    playback = {r['id']: r for r in map(json.loads, run.stdout.splitlines()) if 'id' in r}
    assert content(playback[4])['valid']
    assert base64.b64decode(content(playback[5])['data']) == expected['legacy']
    print('JSONL/MCP: physical plans/creation, equivalent units, density, projects and DLL-independent replay passed.')
    if args.skip_gui:
        print(output)
        return
    startup = subprocess.STARTUPINFO()
    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = 0
    process = subprocess.Popen([str(gui), *flags], startupinfo=startup)
    endpoint = profile / 'session.json'

    def call(name, arguments):
        run = subprocess.run([str(cli), 'call', name, json.dumps(arguments), '--attach', str(endpoint)], capture_output=True,
                             text=True, encoding='utf-8', timeout=30)
        assert run.returncode == 0, run.stderr + run.stdout[-2000:]
        return json.loads(run.stdout)['result']

    def capture(name):
        path = output / name
        call('ui.capture', dict(path=str(path)))
        deadline = time.monotonic()+10
        while not path.exists():
            assert time.monotonic() < deadline, 'Capture timeout'
            time.sleep(.05)
    try:
        deadline = time.monotonic()+20
        while not endpoint.exists():
            assert process.poll() is None and time.monotonic() < deadline, 'GUI startup failed'
            time.sleep(.05)
        gui_rows, gui_positions = commands(output, 'gui')
        values = [call(c['action'], c['args']) for c in gui_rows]
        assert validate(values, gui_positions, desktop=True) == expected
        for language in ['en', 'zh-CN', 'ja', 'ko']:
            call('settings.update', dict(path=str(settings), ui=dict(language=language)))
            assert json.loads(settings.read_text(encoding='utf-8-sig'))['custom']['keep'] == 42
            before = call('canvas.info', {})
            png = base64.b64decode(call('canvas.snapshot', {})['data'])
            call('ui.dispatch', dict(id='command', value='New'))
            time.sleep(.3)
            capture(f'{language}-physical-new-canvas.png')
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
    print('GUI physical-creation parity and four-language New Canvas captures/cancel passed.')
    print(output)


if __name__ == '__main__':
    main()
