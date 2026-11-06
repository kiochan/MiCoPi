"""Check isolated groups through JSONL, MCP, GUI and DLL-independent replay.

Small exact-color references distinguish group opacity from child opacity.
The real Layers panel is captured in four languages under an isolated profile.
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
    dict(id=1, type='action.group.create', position=[0, 0], parameters=dict(members=[], name='Workflow notes')),
    dict(id=2, type='action.group.set', position=[400, 0], parameters=dict(opacity=.75)),
], edges=[dict(**{'from': 1}, output='group_id', to=2, input='group_id')])


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
    add('name-red', 'layer.rename', name='Red cel 赤')
    add('add-blue', 'layer.add')
    add('blue', 'layer.fill_color', color='#0000FF')
    add('name-blue', 'layer.rename', name='Blue cel 青')
    add('select', 'layer.selection.set', layer_ids=[1, 2], active_layer_id=2)
    add('inner', 'group.create_selected', name='Colors 彩')
    add('half', 'group.set', group_id=1, opacity=.5)
    add('half-png', 'canvas.snapshot')
    add('outer', 'group.create', members=[dict(kind='group', id=1)], name='Drawing 作画')
    add('quarter', 'group.set', group_id=2, opacity=.5)
    add('quarter-png', 'canvas.snapshot')
    add('empty', 'group.create', members=[], name='Notes メモ')
    add('query-name', 'group.query', query='Colors')
    add('query-ancestor', 'group.query', query='作画')
    add('workflow-set', 'workflow.set', graph=GRAPH)
    add('workflow-before', 'group.query')
    add('workflow-dry', 'workflow.run', atomic=True, dry_run=True)
    add('workflow-inert', 'group.query')
    add('workflow-run', 'workflow.run', atomic=True)
    add('workflow-after', 'group.query')
    add('duplicate', 'group.duplicate', group_id=2)
    add('duplicate-query', 'group.query')
    add('duplicate-png', 'canvas.snapshot')
    add('duplicate-undo', 'history.undo')
    add('duplicate-restored', 'canvas.snapshot')
    add('reparent', 'group.move', items=[dict(kind='group', id=1)], parent_id=0, index=1)
    add('reparent-png', 'canvas.snapshot')
    add('reparent-undo', 'history.undo')
    add('ungroup', 'group.ungroup', group_id=2)
    add('ungroup-png', 'canvas.snapshot')
    add('ungroup-undo', 'history.undo')
    add('hide', 'group.set', group_id=2, visible=False)
    add('hidden-layers', 'layer.query')
    add('hidden-png', 'canvas.snapshot')
    add('show', 'group.set', group_id=2, visible=True)
    add('delete-empty', 'group.delete', group_id=3)
    add('delete-empty-undo', 'history.undo')
    add('brush', 'brush.set', radius=3, color='#F8EFE0', stabilizer=.35,
        pressure_size=True, pressure_opacity=True)
    add('stroke', 'canvas.stroke', points=[dict(x=8, y=50, pressure=.2, pen=True, time=0),
                                         dict(x=42, y=14, pressure=1., pen=True, time=40),
                                         dict(x=88, y=46, pressure=.4, pen=True, time=80)])
    add('final-query', 'group.query')
    add('final-layers', 'layer.query')
    add('final-png', 'canvas.snapshot')
    add('save', 'document.save', path=str(output / f'{prefix}.mcp'))
    add('open', 'document.open', path=str(output / f'{prefix}.mcp'))
    add('reopened-query', 'group.query')
    add('reopened-layers', 'layer.query')
    add('reopened-png', 'canvas.snapshot')
    add('verify', 'replay.verify')
    return rows, positions


def uniform(png, rgba):
    width, height, data = pixels(png)
    assert (width, height) == (96, 64)
    assert data == bytes(rgba) * (width * height), (rgba, data[:16])


def validate(values, positions):
    at = lambda key: values[positions[key]]
    png = lambda key: base64.b64decode(at(key)['data'])
    assert at('inner')['group_id'] == 1 and at('outer')['group_id'] == 2 and at('empty')['group_id'] == 3
    uniform(png('half-png'), [0, 0, 255, 128])
    uniform(png('quarter-png'), [0, 0, 255, 64])
    query = at('query-name')
    assert query['matching_group_ids'] == [1]
    assert any(row['kind'] == 'group' and row['id'] == 2 and row['context'] for row in query['rows'])
    assert len([row for row in query['rows'] if row['kind'] == 'layer']) == 2
    assert at('query-ancestor')['matching_group_ids'] == [2]
    assert at('workflow-dry')['ok'] and at('workflow-run')['ok']
    assert at('workflow-before')['groups'] == at('workflow-inert')['groups']
    assert len(at('workflow-after')['groups']) == 4
    duplicate = at('duplicate-query')
    assert duplicate['selected_layer_ids'] == [3, 4]
    assert duplicate['active_layer_id'] in [3, 4]
    assert len(duplicate['groups']) == 6
    uniform(png('duplicate-png'), [0, 0, 255, 112])
    assert png('duplicate-restored') == png('quarter-png')
    uniform(png('reparent-png'), [0, 0, 255, 128])
    uniform(png('ungroup-png'), [0, 0, 255, 128])
    uniform(png('hidden-png'), [0, 0, 0, 0])
    assert all(row['visible'] and not row['effective_visible'] for row in at('hidden-layers')['layers'])
    final, final_layers = at('final-query'), at('final-layers')
    assert len(final['groups']) == 4
    assert final['selected_layer_ids'] == [1, 2] and final['active_layer_id'] == 2
    assert [row['id'] for row in final_layers['layers']] == [1, 2]
    assert all(row['parent_id'] == 1 and row['effective_visible'] for row in final_layers['layers'])
    assert at('reopened-query')['groups'] == final['groups']
    assert at('reopened-query')['roots'] == final['roots']
    assert at('reopened-layers')['layers'] == final_layers['layers']
    result = png('final-png')
    assert result == png('reopened-png') and at('verify')['valid']
    _, _, rgba = pixels(result)
    assert set(rgba[3::4]) == {64}
    assert len({rgba[i:i+4] for i in range(0, len(rgba), 4)}) > 1, 'The pressure stroke did not reach the active raster'
    return final['groups'], final_layers['layers'], result


def run_mcp(cli, flags, requests):
    run = subprocess.run([str(cli), 'mcp', *flags],
                         input='\n'.join(json.dumps(r) for r in requests)+'\n', capture_output=True,
                         text=True, encoding='utf-8', timeout=90)
    assert run.returncode == 0, run.stderr+run.stdout[-3000:]
    return {r['id']: r for r in map(json.loads, run.stdout.splitlines()) if 'id' in r}


def main():
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument('--cli', default='dist/micopi-cli.exe')
    parser.add_argument('--gui', default='dist/MiCoPi.exe')
    parser.add_argument('--skip-gui', action='store_true')
    args = parser.parse_args()
    cli, gui = (ROOT / args.cli).resolve(), (ROOT / args.gui).resolve()
    output = ROOT / 'target/test-output/groups-smoke' / str(time.time_ns())
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
    script = output / 'groups.jsonl'
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
    archive = output / 'groups.mcreplay'
    requests += [request(900, 'node.catalog', {}), request(901, 'replay.export', dict(path=str(archive))),
                 request(902, 'group.move', dict(items=[dict(kind='group', id=2)], parent_id=1, index=0)),
                 request(903, 'group.delete', dict(group_id=2)),
                 request(904, 'group.select_layers', dict(group_id=3)),
                 request(905, 'canvas.snapshot', {}), request(906, 'group.query', {})]
    replies = run_mcp(cli, flags, requests)
    tools = {tool['name']: tool for tool in replies[2]['result']['tools']}
    for name in ['group.query', 'group.create', 'group.create_selected', 'group.set', 'group.move',
                 'group.ungroup', 'group.ungroup_parent', 'group.delete', 'group.duplicate', 'group.select_layers']:
        assert name in tools
    assert validate([content(replies[10+i]) for i in range(len(mcp_rows))], mcp_positions) == expected
    catalog = json.dumps(content(replies[900]))
    assert 'compositing.transparent' in catalog and 'action.group.create' in catalog and 'action.group.query' in catalog
    assert all(replies[i]['result']['isError'] for i in [902, 903, 904])
    assert base64.b64decode(content(replies[905])['data']) == expected[2]
    assert content(replies[906])['groups'] == expected[0]
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
    print('JSONL/MCP: isolated/nested opacity references, hierarchy, copies, nodes, history, project and DLL-independent replay passed.')
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
        call('window.command', dict(panel='layers', operation='show'))
        for language in ['en', 'zh-CN', 'ja', 'ko']:
            call('settings.update', dict(path=str(settings), ui=dict(language=language)))
            assert json.loads(settings.read_text(encoding='utf-8-sig'))['custom']['keep'] == 42
            before = call('group.query', {})['groups']
            png = base64.b64decode(call('canvas.snapshot', {})['data'])
            time.sleep(.3)
            capture(f'{language}-layer-groups.png')
            assert call('group.query', {})['groups'] == before
            assert base64.b64decode(call('canvas.snapshot', {})['data']) == png
        call('settings.update', dict(path=str(settings), ui=dict(language='en')))
        before = call('group.query', {})
        graph = call('graph.read', {})
        png = base64.b64decode(call('canvas.snapshot', {})['data'])
        call('graph.open', {})
        time.sleep(.3)
        capture('en-group-composition.png')
        after = call('group.query', {})
        assert {k: v for k, v in after.items() if k != 'revision'} == {
            k: v for k, v in before.items() if k != 'revision'}
        current = call('graph.read', {})
        assert current['graph'] == graph['graph']
        assert current['effective_graph'] == graph['effective_graph']
        assert base64.b64decode(call('canvas.snapshot', {})['data']) == png
        assert call('replay.verify', {})['valid']
        call('ui.dispatch', dict(id='close', value=True))
        assert process.wait(timeout=10) == 0
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=10)
    print('GUI group parity, four-language hierarchy and English Actual Composition captures passed.')
    print(output)


if __name__ == '__main__':
    main()
