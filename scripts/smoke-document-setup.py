"""Document setup parity, metadata history and isolated four-language dialog captures.

Density-aware image import/export is outside this check: resolution is project
metadata and must leave PNG pixels unchanged. Only test-created processes close.
"""
import argparse
import base64
import json
from pathlib import Path
import shutil
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]


def action(name, **args):
    return dict(action=name, args=args)


def request(number, name, arguments):
    return dict(jsonrpc='2.0', id=number, method='tools/call',
                params=dict(name=name, arguments=arguments))


def content(reply, allow_error=False):
    assert 'error' not in reply, reply
    result = reply['result']
    assert allow_error or not result.get('isError'), result
    value = result.get('structuredContent')
    if value is None:
        value = json.loads(result['content'][0]['text'])
    return value['result']


def physical(info, ppi):
    assert info['width'] == 320 and info['height'] == 240, info
    assert info['resolution_ppi'] == ppi, info
    assert info['initial_background'] == '#F8EFE0', info
    assert abs(info['width_mm'] - 320 * 25.4 / ppi) < 1e-9, info
    assert abs(info['height_mm'] - 240 * 25.4 / ppi) < 1e-9, info


def commands(project):
    return [
        action('canvas.new', width=320, height=240, resolution_ppi=320, background='#f8efe0'),
        action('canvas.info'),
        action('brush.set', color='#334477', radius=7, stabilizer=.35,
               pressure_size=True, pressure_opacity=True),
        action('canvas.stroke', points=[
            dict(x=30, y=150, pressure=.2, pen=True, time=0),
            dict(x=140, y=50, pressure=1., pen=True, time=40),
            dict(x=290, y=155, pressure=.4, pen=True, time=80)]),
        action('canvas.snapshot'),
        action('canvas.resolution', resolution_ppi=640),
        action('canvas.info'),
        action('history.undo'),
        action('canvas.info'),
        action('history.redo'),
        action('canvas.info'),
        action('document.save', path=str(project)),
        action('document.open', path=str(project)),
        action('canvas.info'),
        action('replay.verify'),
        action('canvas.snapshot'),
    ]


def validate(values):
    physical(values[1], 320)
    physical(values[6], 640)
    physical(values[8], 320)
    physical(values[10], 640)
    physical(values[13], 640)
    assert values[14]['valid'], values[14]
    before = base64.b64decode(values[4]['data'])
    after = base64.b64decode(values[15]['data'])
    assert before == after, 'Resolution metadata must not resample pixels'
    return after


def main():
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument('--cli', default='dist/micopi-cli.exe')
    parser.add_argument('--gui', default='dist/MiCoPi.exe')
    parser.add_argument('--skip-gui', action='store_true')
    args = parser.parse_args()
    cli, gui = (ROOT / args.cli).resolve(), (ROOT / args.gui).resolve()
    output = ROOT / 'target/test-output/document-setup-smoke' / str(time.time_ns())
    output.mkdir(parents=True)
    plugins = output / 'plugins'
    shutil.copytree(ROOT / 'plugins', plugins)
    profile = output / 'profile'
    profile.mkdir()
    locale_dir = profile / 'locales'
    locale_dir.mkdir()
    for language in ['zh-CN', 'ja', 'ko']:
        shutil.copy2(ROOT / 'locales' / f'{language}.json', locale_dir / f'{language}.json')
    settings = profile / 'settings.json'
    settings.write_text(json.dumps(dict(version=1, custom=dict(keep=42))), encoding='utf-8')
    flags = ['--plugins', str(plugins), '--config-dir', str(profile)]
    batch = commands(output / 'batch.mcp')
    script = output / 'setup.jsonl'
    script.write_text('\n'.join(json.dumps(c) for c in batch) + '\n', encoding='utf-8')
    p = subprocess.run([str(cli), 'run', str(script), *flags], capture_output=True,
                       text=True, encoding='utf-8', timeout=40)
    assert p.returncode == 0, p.stderr + p.stdout
    records = [json.loads(line) for line in p.stdout.splitlines()]
    assert len(records) == len(batch) and all(r['ok'] for r in records), records
    expected = validate([r['data']['result'] for r in records])
    (output / 'batch.png').write_bytes(expected)

    rollback = dict(atomic=True, steps=[
        dict(id='resolution', action='canvas.resolution', args=dict(resolution_ppi=150)),
        dict(id='fail', action='layer.select', args=dict(layer_id=99999))])
    workflow = dict(version=1, nodes=[
        dict(id=1, type='state.core.document_info', position=[0, 0], parameters={}),
        dict(id=2, type='action.canvas.resolution', position=[420, 0], parameters={})],
        edges=[dict(**{'from': 1}, output='resolution_ppi', to=2, input='resolution_ppi')])
    requests = [dict(jsonrpc='2.0', id=1, method='initialize',
                     params=dict(protocolVersion='2025-11-25')),
                dict(jsonrpc='2.0', method='notifications/initialized'),
                dict(jsonrpc='2.0', id=2, method='tools/list', params={})]
    requests += [request(i, c['action'], c['args'])
                 for i, c in enumerate(commands(output / 'mcp.mcp'), 10)]
    requests += [
        request(100, 'pipeline.run', rollback),
        request(101, 'canvas.info', {}),
        request(102, 'workflow.set', dict(graph=workflow)),
        request(103, 'canvas.info', {}),
        request(104, 'workflow.run', dict(dry_run=True, atomic=True)),
        request(105, 'workflow.run', dict(atomic=True)),
        request(106, 'canvas.info', {}),
        request(107, 'replay.verify', {}),
        dict(jsonrpc='2.0', id=108, method='resources/read',
             params=dict(uri='micopi://state/core.document_info')),
        request(109, 'canvas.snapshot', {}),
    ]
    p = subprocess.run([str(cli), 'mcp', *flags],
                       input='\n'.join(json.dumps(r) for r in requests) + '\n',
                       capture_output=True, text=True, encoding='utf-8', timeout=40)
    assert p.returncode == 0, p.stderr + p.stdout
    replies = {r['id']: r for r in map(json.loads, p.stdout.splitlines()) if 'id' in r}
    tools = replies[2]['result']['tools']
    assert all(any(t['name'] == name for t in tools)
               for name in ['canvas.info', 'canvas.resolution', 'canvas.new'])
    for name, required in [('canvas.new', ['width', 'height']), ('canvas.resolution', ['resolution_ppi'])]:
        tool = next(t for t in tools if t['name'] == name)
        assert all(k in tool['inputSchema']['required'] for k in required), tool
    values = []
    for number in range(10, 26):
        if number in [14, 25]:
            block = replies[number]['result']['content'][0]
            assert block['type'] == 'image' and block['mimeType'] == 'image/png', block
            values.append(dict(data=block['data']))
        else:
            values.append(content(replies[number]))
    assert validate(values) == expected
    failed = content(replies[100], allow_error=True)
    assert failed['rolled_back'] and not failed['ok'], failed
    for number in [101, 103, 106]:
        physical(content(replies[number]), 640)
    assert content(replies[104])['ok'] and content(replies[105])['ok']
    assert content(replies[107])['valid']
    resource = json.loads(replies[108]['result']['contents'][0]['text'])
    physical(resource, 640)
    assert base64.b64decode(replies[109]['result']['content'][0]['data']) == expected
    (output / 'mcp.png').write_bytes(expected)
    print('JSONL/MCP: setup, metadata-only undo/redo, atomic rollback, state nodes, projects and replay passed.')
    if args.skip_gui:
        print(output)
        return

    startup = subprocess.STARTUPINFO()
    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = 0
    process = subprocess.Popen([str(gui), *flags], startupinfo=startup)
    endpoint = profile / 'session.json'

    def call(name, arguments, allow_failure=False):
        p = subprocess.run([str(cli), 'call', name, json.dumps(arguments), '--attach', str(endpoint)],
                           capture_output=True, text=True, encoding='utf-8', timeout=30)
        assert allow_failure or p.returncode == 0, p.stderr + p.stdout
        return json.loads(p.stdout)['result']

    def capture(name):
        path = output / name
        call('ui.capture', dict(path=str(path)))
        deadline = time.monotonic() + 10
        while not path.exists():
            assert time.monotonic() < deadline, 'Capture timed out'
            time.sleep(.05)

    try:
        deadline = time.monotonic() + 20
        while not endpoint.exists():
            assert process.poll() is None and time.monotonic() < deadline, 'GUI startup failed'
            time.sleep(.05)
        values = [call(c['action'], c['args']) for c in commands(output / 'gui.mcp')]
        assert validate(values) == expected
        result = call('pipeline.run', rollback, allow_failure=True)
        assert result['rolled_back'] and not result['ok'], result
        physical(call('canvas.info', {}), 640)
        call('workflow.set', dict(graph=workflow))
        assert call('workflow.run', dict(dry_run=True, atomic=True))['ok']
        assert call('workflow.run', dict(atomic=True))['ok']
        assert call('replay.verify', {})['valid']
        assert base64.b64decode(call('canvas.snapshot', {})['data']) == expected
        (output / 'gui.png').write_bytes(expected)
        for language in ['en', 'zh-CN', 'ja', 'ko']:
            call('settings.update', dict(path=str(settings), ui=dict(language=language)))
            stored = json.loads(settings.read_text(encoding='utf-8-sig'))
            assert stored['custom']['keep'] == 42 and stored['ui']['language'] == language
            assert stored['new_canvas']['default_preset'] == 'illustration'
            call('canvas.new', dict(width=320, height=240, resolution_ppi=320, background='#F8EFE0'))
            pixels = base64.b64decode(call('canvas.snapshot', {})['data'])
            call('ui.dispatch', dict(id='command', value='New'))
            time.sleep(.3)
            info = call('canvas.info', {})
            capture(f'{language}-new-canvas.png')
            assert call('canvas.info', {}) == info
            assert base64.b64decode(call('canvas.snapshot', {})['data']) == pixels
            call('ui.dispatch', dict(id='command', value='Cancel'))
            time.sleep(.1)
            assert call('canvas.info', {}) == info
            assert base64.b64decode(call('canvas.snapshot', {})['data']) == pixels
        call('ui.dispatch', dict(id='close', value=True))
        assert process.wait(timeout=10) == 0
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=10)
    print('GUI parity and four-language New Canvas captures passed; draft cancellation preserves pixels/info.')
    print(output)


if __name__ == '__main__':
    main()
