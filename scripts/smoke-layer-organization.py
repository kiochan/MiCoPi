"""Check layer locks, labels and discovery across JSONL, MCP and desktop.

Uses isolated profiles, real drawing plugins and small documents. Playback is
verified without the painting DLL; captures show the real Layers panel in four
languages. Synthetic pressure samples do not establish tablet compatibility.
"""
import argparse
import base64
import json
from pathlib import Path
import shutil
import subprocess
import time

from png_checks import pixels

ROOT = Path(__file__).resolve().parents[1]
GRAPH = dict(version=1, nodes=[dict(id=1, type='action.layer.query', position=[0, 0],
                                  parameters=dict(query='墨', pixel_locked=True))], edges=[])


def request(number, name, arguments):
    return dict(jsonrpc='2.0', id=number, method='tools/call', params=dict(name=name, arguments=arguments))


def content(reply):
    assert 'error' not in reply, reply
    result = reply['result']
    assert not result.get('isError'), result
    return result['structuredContent']['result']


def commands(output, prefix):
    rows, positions = [], {}

    def add(key, operation, **args):
        positions[key] = len(rows)
        rows.append(dict(action=operation, args=args))
    add('new', 'canvas.new', width=96, height=64, background='#F8EFE0')
    add('brush', 'brush.set', radius=3, color='#445588', stabilizer=.35,
        pressure_size=True, pressure_opacity=True)
    add('ink', 'canvas.stroke', points=[dict(x=8, y=50, pressure=.2, pen=True, time=0),
                                      dict(x=42, y=14, pressure=1., pen=True, time=40),
                                      dict(x=88, y=46, pressure=.4, pen=True, time=80)])
    add('name-ink', 'layer.rename', name='Ink 墨')
    add('lock-ink', 'layer.set', pixel_locked=True, position_locked=True, color_label='violet')
    add('duplicate', 'layer.duplicate')
    add('duplicate-query', 'layer.query')
    add('name-paint', 'layer.rename', name='Paint 彩')
    add('position-only', 'layer.set', pixel_locked=False, color_label='blue', opacity=.5)
    add('paint', 'canvas.stroke', points=[dict(x=10, y=12, pressure=.3, pen=True, time=0),
                                        dict(x=82, y=44, pressure=.9, pen=True, time=45)])
    add('notes-add', 'layer.add')
    add('notes-name', 'layer.rename', name='Notes ノート')
    add('notes-label', 'layer.set', color_label='green', visible=False)
    add('default-add', 'layer.add')
    add('default-label', 'layer.set', color_label='gray', alpha_locked=True)
    add('workflow-set', 'workflow.set', graph=GRAPH)
    add('workflow-before', 'canvas.snapshot')
    add('workflow-dry', 'workflow.run', atomic=True, dry_run=True)
    add('workflow-run', 'workflow.run', atomic=True)
    add('all', 'layer.query')
    add('state-before', 'canvas.state')
    add('query-ink', 'layer.query', query='INK', color_label='violet', locked=True,
        pixel_locked=True, position_locked=True, alpha_locked=False, visible=True)
    add('query-paint', 'layer.query', query='彩', color_label='blue', position_locked=True, pixel_locked=False)
    add('query-default', 'layer.query', query='layer 04', alpha_locked=True)
    add('query-hidden', 'layer.query', visible=False, color_label='green')
    add('query-empty', 'layer.query', query='墨', color_label='blue')
    add('public-state', 'context.read', resources=['core.layers'])
    add('state-after', 'canvas.state')
    add('query-snapshot', 'canvas.snapshot')
    add('combined', 'layer.set', alpha_locked=False, pixel_locked=True, position_locked=False, color_label='orange')
    add('combined-query', 'layer.query', query='layer 04')
    add('undo', 'history.undo')
    add('undo-query', 'layer.query', query='layer 04')
    add('redo', 'history.redo')
    add('redo-query', 'layer.query', query='layer 04')
    add('pipeline', 'pipeline.run', atomic=True, steps=[
        dict(id='find', action='layer.query', args=dict(query='墨', pixel_locked=True)),
        dict(id='label', action='layer.set', args=dict(layer_id={'$ref': 'find#/result/layers/0/id'}, color_label='red')),
    ])
    add('read-select', 'layer.select', layer_id=1)
    add('copy', 'clipboard.copy')
    add('paste', 'clipboard.paste_in_place')
    add('paste-query', 'layer.query')
    add('paste-undo', 'history.undo')
    add('stamp', 'layer.stamp_visible')
    add('stamp-query', 'layer.query')
    add('stamp-undo', 'history.undo')
    add('final-query', 'layer.query')
    add('final-snapshot', 'canvas.snapshot')
    add('save', 'document.save', path=str(output / f'{prefix}.mcp'))
    add('open', 'document.open', path=str(output / f'{prefix}.mcp'))
    add('reopened-query', 'layer.query')
    add('reopened-snapshot', 'canvas.snapshot')
    add('verify', 'replay.verify')
    return rows, positions


def validate(values, positions, desktop=False):
    at = lambda name: values[positions[name]]
    rows = at('all')['layers']
    assert at('all')['total'] == at('all')['matched'] == 4
    assert [(r['id'], r['name'], r['color_label']) for r in rows] == [
        (1, 'Ink 墨', 'violet'), (2, 'Paint 彩', 'blue'), (3, 'Notes ノート', 'green'), (4, 'Layer 04', 'gray')]
    assert [(r['pixel_locked'], r['position_locked'], r['alpha_locked']) for r in rows] == [
        (True, True, False), (False, True, False), (False, False, False), (False, False, True)]
    assert [r['active'] for r in rows] == [False, False, False, True]
    duplicated = at('duplicate-query')['layers'][1]
    assert duplicated['pixel_locked'] and duplicated['position_locked'] and duplicated['color_label'] == 'violet'
    for key, expected in [('query-ink', [1]), ('query-paint', [2]), ('query-default', [4]),
                          ('query-hidden', [3]), ('query-empty', [])]:
        query = at(key)
        assert query['total'] == 4 and query['matched'] == len(expected)
        assert [r['id'] for r in query['layers']] == expected, query
    state_before, state_after = at('state-before'), at('state-after')
    if desktop:
        semantic = lambda state: {k: v for k, v in state.items() if k not in ['ui', 'view', 'revision']}
        assert semantic(state_before) == semantic(state_after)
    else:
        assert state_before == state_after
    assert at('public-state')['resources']['core.layers']['ok']
    assert at('public-state')['resources']['core.layers']['value']['layers'] == rows
    assert state_after['layers'] == rows
    assert at('workflow-dry')['ok'] and at('workflow-run')['ok']
    assert base64.b64decode(at('workflow-before')['data']) == base64.b64decode(at('query-snapshot')['data'])
    changed = at('combined-query')['layers'][0]
    assert changed['pixel_locked'] and not changed['position_locked'] and not changed['alpha_locked']
    assert changed['color_label'] == 'orange'
    assert at('undo-query')['layers'] == [rows[3]]
    assert at('redo-query')['layers'] == [changed]
    assert at('pipeline')['ok']
    for key in ['paste-query', 'stamp-query']:
        inserted = next(r for r in at(key)['layers'] if r['active'])
        assert inserted['color_label'] == 'none'
        assert not inserted['alpha_locked'] and not inserted['pixel_locked'] and not inserted['position_locked']
        assert at(key)['total'] == 5
    final = at('final-query')['layers']
    assert len(final) == 4 and final[0]['color_label'] == 'red'
    assert final[0]['pixel_locked'] and final[0]['position_locked'] and final[0]['active']
    assert at('reopened-query')['layers'] == final
    png = base64.b64decode(at('final-snapshot')['data'])
    assert png == base64.b64decode(at('reopened-snapshot')['data'])
    assert pixels(png)[:2] == (96, 64) and at('verify')['valid']
    return final, png


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
    output = ROOT / 'target/test-output/layer-organization-smoke' / str(time.time_ns())
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
    script = output / 'layer-organization.jsonl'
    script.write_text('\n'.join(json.dumps(row) for row in rows) + '\n', encoding='utf-8')
    run = subprocess.run([str(cli), 'run', str(script), *flags], capture_output=True,
                         text=True, encoding='utf-8', timeout=90)
    assert run.returncode == 0, run.stderr + run.stdout[-3000:]
    records = [json.loads(line) for line in run.stdout.splitlines()]
    assert len(records) == len(rows) and all(r['ok'] for r in records), records[-3:]
    expected = validate([r['data']['result'] for r in records], positions)
    initialized = [dict(jsonrpc='2.0', id=1, method='initialize', params=dict(protocolVersion='2025-11-25')),
                   dict(jsonrpc='2.0', method='notifications/initialized')]
    requests = initialized + [dict(jsonrpc='2.0', id=2, method='tools/list', params={})]
    mcp_rows, mcp_positions = commands(output, 'mcp')
    requests += [request(10+i, row['action'], row['args']) for i, row in enumerate(mcp_rows)]
    archive = output / 'layer-organization.mcreplay'
    requests += [request(900, 'node.catalog', {}), request(901, 'replay.export', dict(path=str(archive))),
                 request(902, 'native.invert', {}), request(903, 'layer.clear', {}),
                 request(904, 'canvas.move', dict(target='layer', dx=1, dy=0)),
                 request(905, 'layer.query', dict(query='bad\nquery')),
                 request(906, 'layer.set', dict(color_label='purple')),
                 request(907, 'canvas.snapshot', {}), request(908, 'layer.query', {})]
    replies = run_mcp(cli, flags, requests)
    tools = replies[2]['result']['tools']
    properties = next(t for t in tools if t['name'] == 'layer.set')['inputSchema']['properties']
    assert properties['pixel_locked']['type'] == properties['position_locked']['type'] == 'boolean'
    assert set(properties['color_label']['enum']) == {'none', 'red', 'orange', 'yellow', 'green', 'blue', 'violet', 'gray'}
    assert any(t['name'] == 'layer.query' for t in tools)
    assert validate([content(replies[10+i]) for i in range(len(mcp_rows))], mcp_positions) == expected
    catalog = json.dumps(content(replies[900]))
    assert 'action.layer.query' in catalog and 'state.core.layers' in catalog
    assert all(replies[i]['result']['isError'] for i in range(902, 907))
    assert base64.b64decode(content(replies[907])['data']) == expected[1]
    assert content(replies[908])['layers'] == expected[0]
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
    print('JSONL/MCP: labels/locks, query/state/node discovery, copies, history, project and DLL-independent replay passed.')
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
        gui_rows, gui_positions = commands(output, 'gui')
        assert validate([call(row['action'], row['args']) for row in gui_rows], gui_positions, desktop=True) == expected
        call('window.command', dict(panel='layers', operation='show'))
        for language in ['en', 'zh-CN', 'ja', 'ko']:
            call('settings.update', dict(path=str(settings), ui=dict(language=language)))
            assert json.loads(settings.read_text(encoding='utf-8-sig'))['custom']['keep'] == 42
            before = call('layer.query', {})['layers']
            png = base64.b64decode(call('canvas.snapshot', {})['data'])
            time.sleep(.3)
            capture(f'{language}-layer-organization.png')
            assert call('layer.query', {})['layers'] == before
            assert base64.b64decode(call('canvas.snapshot', {})['data']) == png
        assert call('replay.verify', {})['valid']
        call('ui.dispatch', dict(id='close', value=True))
        assert process.wait(timeout=10) == 0
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=10)
    print('GUI layer organization parity and four-language Layers panel captures passed.')
    print(output)


if __name__ == '__main__':
    main()
