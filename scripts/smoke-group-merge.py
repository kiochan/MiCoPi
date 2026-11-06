"""Check Group Merge and grouped Merge Visible across JSONL, MCP and GUI.

Exact opacity references, fresh-ID workflow binding, hidden locked content,
single-step undo, native projects and DLL-independent outcome replay are checked.
The real schema command form is opened/cancelled in four languages for captures.
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
GRAPH = dict(version=1, nodes=[
    dict(id=1, type='action.group.merge', position=[0, 0], parameters=dict(group_id=1)),
    dict(id=2, type='action.layer.rename', position=[400, 0], parameters=dict(name='Baked cel 彩')),
], edges=[dict(**{'from': 1}, output='layer_id', to=2, input='layer_id')])


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
    add('new', 'canvas.new', width=96, height=64)
    add('red', 'layer.fill_color', color='#FF0000')
    add('name-red', 'layer.rename', name='Red source 赤')
    add('add-blue', 'layer.add')
    add('blue', 'layer.fill_color', color='#0000FF')
    add('name-blue', 'layer.rename', name='Blue source 青')
    add('add-hidden', 'layer.add')
    add('green', 'layer.fill_color', color='#00FF00')
    add('name-hidden', 'layer.rename', name='Hidden source 隠し')
    add('hide-lock', 'layer.set', visible=False, pixel_locked=True, alpha_locked=True, color_label='red')
    add('group', 'group.create', members=[dict(kind='layer', id=1), dict(kind='layer', id=2)], name='Cel group 彩')
    add('half', 'group.set', group_id=1, opacity=.5)
    add('empty', 'group.create', members=[], name='Notes メモ')
    add('before', 'canvas.snapshot')
    add('workflow-set', 'workflow.set', graph=GRAPH)
    add('workflow-dry', 'workflow.run', atomic=True, dry_run=True)
    add('inert', 'layer.query')
    add('merge', 'group.merge', group_id=1)
    add('merged-query', 'layer.query')
    add('merged-groups', 'group.query')
    add('merged-png', 'canvas.snapshot')
    add('merge-undo', 'history.undo')
    add('undo-query', 'layer.query')
    add('undo-png', 'canvas.snapshot')
    add('workflow-run', 'workflow.run', atomic=True)
    add('workflow-query', 'layer.query')
    add('workflow-png', 'canvas.snapshot')
    add('visible', 'layer.merge_visible')
    add('visible-query', 'layer.query')
    add('visible-png', 'canvas.snapshot')
    add('visible-undo', 'history.undo')
    add('visible-restored', 'layer.query')
    add('visible-redo', 'history.redo')
    add('final-query', 'layer.query')
    add('final-groups', 'group.query')
    add('final-png', 'canvas.snapshot')
    add('save', 'document.save', path=str(output / f'{prefix}.mcp'))
    add('open', 'document.open', path=str(output / f'{prefix}.mcp'))
    add('reopened-query', 'layer.query')
    add('reopened-groups', 'group.query')
    add('reopened-png', 'canvas.snapshot')
    add('verify', 'replay.verify')
    return rows, positions


def validate(values, positions):
    at = lambda key: values[positions[key]]
    image = lambda key: base64.b64decode(at(key)['data'])
    before = image('before')
    width, height, rgba = pixels(before)
    assert (width, height) == (96, 64) and rgba == bytes([0, 0, 255, 128]) * (width * height)
    assert at('workflow-dry')['ok'] and len(at('inert')['layers']) == 3
    assert at('merge')['group_id'] == 1 and at('merge')['layer_id'] == 4
    merged = next(row for row in at('merged-query')['layers'] if row['active'])
    assert merged['id'] == 4 and merged['name'] == 'Cel group 彩' and merged['parent_id'] == 0
    assert abs(merged['opacity'] - 128/255) < 1e-6 and merged['blend_mode'] == 'normal'
    assert not merged['pixel_locked'] and not merged['alpha_locked'] and not merged['position_locked']
    assert merged['color_label'] == 'none' and at('merged-query')['selected_layer_ids'] == [4]
    assert len(at('merged-groups')['groups']) == 1 and at('merged-groups')['groups'][0]['id'] == 2
    assert image('merged-png') == before == image('undo-png')
    assert [row['id'] for row in at('undo-query')['layers']] == [1, 2, 3]
    assert at('workflow-run')['ok']
    baked = next(row for row in at('workflow-query')['layers'] if row['active'])
    assert baked['id'] == 5 and baked['name'] == 'Baked cel 彩'
    assert abs(baked['opacity'] - 128/255) < 1e-6
    assert image('workflow-png') == before == image('visible-png')
    visible = next(row for row in at('visible-query')['layers'] if row['active'])
    assert visible['id'] == baked['id'] and visible['name'] == baked['name']
    assert visible['parent_id'] == 0 and visible['opacity'] == 1 and visible['blend_mode'] == 'normal'
    restored = next(row for row in at('visible-restored')['layers'] if row['active'])
    assert abs(restored['opacity'] - 128/255) < 1e-6, 'A single Undo must restore the prior group bake gate'
    final = at('final-query')
    assert final['selected_layer_ids'] == [5] and final['active_layer_id'] == 5
    assert len(final['layers']) == 2
    hidden = next(row for row in final['layers'] if row['id'] == 3)
    original_hidden = next(row for row in at('inert')['layers'] if row['id'] == 3)
    assert {k: v for k, v in hidden.items() if k not in ['index', 'active', 'selected']} == {
        k: v for k, v in original_hidden.items() if k not in ['index', 'active', 'selected']}
    assert hidden['pixel_locked'] and hidden['alpha_locked'] and not hidden['effective_visible']
    assert at('reopened-query')['layers'] == final['layers']
    assert at('reopened-groups')['groups'] == at('final-groups')['groups']
    assert image('final-png') == before == image('reopened-png') and at('verify')['valid']
    return final['layers'], at('final-groups')['groups'], before


def run_mcp(cli, flags, requests):
    run = subprocess.run([str(cli), 'mcp', *flags],
                         input='\n'.join(json.dumps(row) for row in requests)+'\n', capture_output=True,
                         text=True, encoding='utf-8', timeout=90)
    assert run.returncode == 0, run.stderr+run.stdout[-3000:]
    return {reply['id']: reply for reply in map(json.loads, run.stdout.splitlines()) if 'id' in reply}


def plugin_command_id(name):
    value = 2166136261
    for byte in ('plugin.' + name).encode():
        value = ((value ^ byte) * 16777619) & 0xffffffff
    return value | 0x80000000


def main():
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument('--cli', default='dist/micopi-cli.exe')
    parser.add_argument('--gui', default='dist/MiCoPi.exe')
    parser.add_argument('--skip-gui', action='store_true')
    args = parser.parse_args()
    cli, gui = (ROOT / args.cli).resolve(), (ROOT / args.gui).resolve()
    output = ROOT / 'target/test-output/group-merge-smoke' / str(time.time_ns())
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
    script = output / 'group-merge.jsonl'
    script.write_text('\n'.join(json.dumps(row) for row in rows)+'\n', encoding='utf-8')
    run = subprocess.run([str(cli), 'run', str(script), *flags], capture_output=True,
                         text=True, encoding='utf-8', timeout=90)
    assert run.returncode == 0, run.stderr+run.stdout[-3000:]
    records = [json.loads(line) for line in run.stdout.splitlines()]
    assert len(records) == len(rows) and all(row['ok'] for row in records), records[-3:]
    expected = validate([row['data']['result'] for row in records], positions)
    initialized = [dict(jsonrpc='2.0', id=1, method='initialize', params=dict(protocolVersion='2025-11-25')),
                   dict(jsonrpc='2.0', method='notifications/initialized')]
    mcp_rows, mcp_positions = commands(output, 'mcp')
    requests = initialized + [dict(jsonrpc='2.0', id=2, method='tools/list', params={})]
    requests += [request(10+i, row['action'], row['args']) for i, row in enumerate(mcp_rows)]
    archive = output / 'group-merge.mcreplay'
    requests += [request(900, 'node.catalog', {}), request(901, 'replay.export', dict(path=str(archive))),
                 request(902, 'group.create', dict(members=[dict(kind='layer', id=5), dict(kind='layer', id=3)], name='Locked scope')),
                 request(903, 'canvas.state', {}), request(904, 'group.merge', dict(group_id=3)),
                 request(905, 'canvas.state', {}), request(906, 'canvas.snapshot', {}),
                 request(907, 'canvas.pointer', dict(phase='down', x=8, y=8, pen=True, tool_override='hand')),
                 request(908, 'group.merge', dict(group_id=3)),
                 request(909, 'canvas.pointer', dict(phase='up', x=8, y=8, pen=True, tool_override='hand')),
                 request(910, 'replay.verify', {})]
    replies = run_mcp(cli, flags, requests)
    assert any(tool['name'] == 'group.merge' for tool in replies[2]['result']['tools'])
    assert validate([content(replies[10+i]) for i in range(len(mcp_rows))], mcp_positions) == expected
    assert 'action.group.merge' in json.dumps(content(replies[900]))
    assert replies[904]['result']['isError']
    assert 'lock' in replies[904]['result']['structuredContent']['error']['message'].lower()
    assert content(replies[903]) == content(replies[905])
    assert base64.b64decode(content(replies[906])['data']) == expected[2]
    assert replies[908]['result']['structuredContent']['error']['code'] == 'stroke_active'
    assert content(replies[910])['valid']
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
    assert base64.b64decode(content(playback[5])['data']) == expected[2]
    print('JSONL/MCP: Group Merge, grouped Merge Visible, fresh-ID node binding, hidden locks, Undo, project and DLL-independent replay passed.')
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
        assert run.returncode == 0, run.stderr+run.stdout[-2000:]
        return json.loads(run.stdout)['result']

    def settled_revision(quiet=.75, timeout=8):
        deadline = time.monotonic()+timeout
        previous, changed = None, time.monotonic()
        while time.monotonic() < deadline:
            revision = call('canvas.state', {})['revision']
            if revision != previous:
                previous, changed = revision, time.monotonic()
            elif time.monotonic()-changed >= quiet:
                return revision
            time.sleep(.1)
        raise AssertionError(f'GUI revision did not settle before form capture: {previous}')

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
        assert validate([call(row['action'], row['args']) for row in gui_rows], gui_positions) == expected
        call('canvas.new', dict(width=96, height=64, background='#F8EFE0'))
        call('layer.rename', dict(name='Paper'))
        call('layer.add', {})
        call('layer.rename', dict(name='Ink 墨'))
        call('brush.set', dict(radius=3, color='#445588', stabilizer=.35,
                               pressure_size=True, pressure_opacity=True))
        call('canvas.stroke', dict(points=[
            dict(x=8, y=50, pressure=.2, pen=True, time=0),
            dict(x=42, y=14, pressure=1., pen=True, time=40),
            dict(x=88, y=46, pressure=.4, pen=True, time=80),
        ]))
        created = call('group.create', dict(members=[dict(kind='layer', id=2)], name='Character ink'))
        assert created['group_id'] == 1
        assert next(group for group in call('group.query', {})['groups'] if group['id'] == 1)['layer_ids'] == [2]
        call('window.command', dict(panel='layers', operation='show'))
        for language in ['en', 'zh-CN', 'ja', 'ko']:
            call('settings.update', dict(path=str(settings), ui=dict(language=language)))
            assert json.loads(settings.read_text(encoding='utf-8-sig'))['custom']['keep'] == 42
            revision = settled_revision()
            hierarchy = call('group.query', {})
            graph = call('graph.read', {})
            png = base64.b64decode(call('canvas.snapshot', {})['data'])
            assert hierarchy['revision'] == revision, 'GUI changed after the language/layout settling window'
            call('ui.dispatch', dict(id='command', value=dict(UiCommand=plugin_command_id('group.merge'))))
            form_revision = settled_revision(quiet=.3)
            capture(f'{language}-merge-group.png')
            assert call('canvas.state', {})['revision'] == form_revision, 'GUI changed during form capture'
            opened = call('group.query', {})
            assert {k: v for k, v in opened.items() if k != 'revision'} == {
                k: v for k, v in hierarchy.items() if k != 'revision'}
            assert call('graph.read', {})['effective_graph'] == graph['effective_graph']
            assert base64.b64decode(call('canvas.snapshot', {})['data']) == png
            call('ui.dispatch', dict(id='command', value='Cancel'))
            current = call('group.query', {})
            assert {k: v for k, v in current.items() if k != 'revision'} == {
                k: v for k, v in hierarchy.items() if k != 'revision'}
            assert call('graph.read', {})['effective_graph'] == graph['effective_graph']
            assert base64.b64decode(call('canvas.snapshot', {})['data']) == png
        assert call('replay.verify', {})['valid']
        call('ui.dispatch', dict(id='close', value=True))
        assert process.wait(timeout=10) == 0
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=10)
    print('GUI merge parity and four-language Merge Group form open/cancel captures passed.')
    print(output)


if __name__ == '__main__':
    main()
