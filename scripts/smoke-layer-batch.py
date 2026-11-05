"""Check stable multi-selection and explicit layer batches on all transports.

Uses real painting plugins and isolated profiles. Verifies one-step batch undo,
stable structural targets, query-to-node execution, projects and recorded-state
playback without painting DLLs. GUI captures show four supported languages.
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
    dict(id=1, type='action.layer.query', position=[0, 0], parameters=dict(selected=True)),
    dict(id=2, type='action.layer.batch', position=[400, 0],
         parameters=dict(action='layer.set', args=dict(color_label='violet'))),
], edges=[dict(**{'from': 1}, output='selected_layer_ids', to=2, input='layer_ids')])


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

    def batch(key, operation, targets, **args):
        add(key, 'layer.batch', action=f'layer.{operation}', layer_ids=targets, args=args)
    add('new', 'canvas.new', width=80, height=60, background='#F8EFE0')
    add('brush', 'brush.set', radius=3, color='#445588', stabilizer=.35,
        pressure_size=True, pressure_opacity=True)
    for layer in range(4):
        if layer:
            add(f'add-{layer}', 'layer.add')
        add(f'name-{layer}', 'layer.rename', name=f'Ink {layer+1} 墨')
        add(f'stroke-{layer}', 'canvas.stroke', points=[
            dict(x=6+layer*3, y=48-layer*5, pressure=.2, pen=True, time=0),
            dict(x=30+layer*4, y=10+layer*3, pressure=1., pen=True, time=40),
            dict(x=70-layer*2, y=40+layer*3, pressure=.4, pen=True, time=80),
        ])
    add('selection', 'layer.selection.set', layer_ids=[3, 1], active_layer_id=3)
    add('selected', 'layer.query', selected=True)
    add('unselected', 'layer.query', selected=False)
    add('workflow-set', 'workflow.set', graph=GRAPH)
    add('workflow-before', 'canvas.snapshot')
    add('workflow-dry', 'workflow.run', atomic=True, dry_run=True)
    add('workflow-inert', 'layer.query')
    add('workflow-run', 'workflow.run', atomic=True)
    add('workflow-after', 'layer.query')
    add('context', 'context.read', resources=['core.layers'])
    add('before-flip', 'canvas.snapshot')
    batch('flip', 'flip_horizontal', [3, 1])
    add('after-flip', 'canvas.snapshot')
    add('flip-undo', 'history.undo')
    add('flip-restored', 'canvas.snapshot')
    add('flip-redo', 'history.redo')
    add('flip-repeated', 'canvas.snapshot')
    batch('duplicate', 'duplicate', [3, 1])
    add('duplicate-query', 'layer.query')
    add('duplicate-undo', 'history.undo')
    add('duplicate-restored', 'layer.query')
    batch('move', 'move', [3, 1], to=1)
    add('move-query', 'layer.query')
    add('move-undo', 'history.undo')
    batch('delete', 'delete', [3, 1])
    add('delete-query', 'layer.query')
    add('delete-undo', 'history.undo')
    add('pipeline', 'pipeline.run', atomic=True, steps=[
        dict(id='find', action='layer.query', args=dict(selected=True)),
        dict(id='label', action='layer.batch', args=dict(action='layer.set',
             layer_ids={'$ref': 'find#/result/selected_layer_ids'}, args=dict(color_label='red'))),
    ])
    add('final-query', 'layer.query')
    add('final-snapshot', 'canvas.snapshot')
    add('save', 'document.save', path=str(output / f'{prefix}.mcp'))
    add('open', 'document.open', path=str(output / f'{prefix}.mcp'))
    add('reopened-query', 'layer.query')
    add('reopened-snapshot', 'canvas.snapshot')
    add('verify', 'replay.verify')
    return rows, positions


def validate(values, positions):
    at = lambda key: values[positions[key]]
    row_ids = lambda value: [row['id'] for row in value['layers']]
    selected = at('selected')
    assert selected['selected_layer_ids'] == [1, 3] and selected['active_layer_id'] == 3
    assert row_ids(selected) == [1, 3] and selected['matched'] == 2
    assert row_ids(at('unselected')) == [2, 4]
    assert all(row['color_label'] == 'none' for row in at('workflow-inert')['layers'])
    assert at('workflow-dry')['ok'] and at('workflow-run')['ok']
    assert [row['color_label'] for row in at('workflow-after')['layers']] == ['violet', 'none', 'violet', 'none']
    public = at('context')['resources']['core.layers']
    assert public['ok'] and public['value']['selected_layer_ids'] == [1, 3]
    before, flipped = (base64.b64decode(at(key)['data']) for key in ['before-flip', 'after-flip'])
    assert pixels(before)[:2] == (80, 60) and before != flipped
    assert base64.b64decode(at('flip-restored')['data']) == before
    assert base64.b64decode(at('flip-repeated')['data']) == flipped
    duplicate = at('duplicate-query')
    assert row_ids(duplicate) == [1, 5, 2, 3, 6, 4], duplicate
    assert duplicate['selected_layer_ids'] == [5, 6]
    assert duplicate['active_layer_id'] in [5, 6]
    assert row_ids(at('duplicate-restored')) == [1, 2, 3, 4]
    assert at('duplicate-restored')['selected_layer_ids'] == [1, 3]
    assert at('duplicate-restored')['active_layer_id'] == 3
    assert row_ids(at('move-query')) == [2, 1, 3, 4]
    assert at('move-query')['selected_layer_ids'] == [1, 3]
    assert at('move-query')['active_layer_id'] == 3
    assert row_ids(at('delete-query')) == [2, 4]
    assert len(at('delete-query')['selected_layer_ids']) == 1
    assert at('pipeline')['ok']
    final = at('final-query')
    assert row_ids(final) == [1, 2, 3, 4]
    assert final['selected_layer_ids'] == [1, 3] and final['active_layer_id'] == 3
    assert [row['selected'] for row in final['layers']] == [True, False, True, False]
    assert [row['color_label'] for row in final['layers']] == ['red', 'none', 'red', 'none']
    assert at('reopened-query')['layers'] == final['layers']
    assert at('reopened-query')['selected_layer_ids'] == [1, 3]
    png = base64.b64decode(at('final-snapshot')['data'])
    assert png == flipped == base64.b64decode(at('reopened-snapshot')['data'])
    assert at('verify')['valid']
    return final['layers'], png


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
    output = ROOT / 'target/test-output/layer-batch-smoke' / str(time.time_ns())
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
    script = output / 'layer-batch.jsonl'
    script.write_text('\n'.join(json.dumps(row) for row in rows)+'\n', encoding='utf-8')
    run = subprocess.run([str(cli), 'run', str(script), *flags], capture_output=True,
                         text=True, encoding='utf-8', timeout=90)
    assert run.returncode == 0, run.stderr+run.stdout[-3000:]
    records = [json.loads(line) for line in run.stdout.splitlines()]
    assert len(records) == len(rows) and all(r['ok'] for r in records), records[-3:]
    expected = validate([r['data']['result'] for r in records], positions)
    initialized = [dict(jsonrpc='2.0', id=1, method='initialize', params=dict(protocolVersion='2025-11-25')),
                   dict(jsonrpc='2.0', method='notifications/initialized')]
    mcp_rows, mcp_positions = commands(output, 'mcp')
    requests = initialized + [dict(jsonrpc='2.0', id=2, method='tools/list', params={})]
    requests += [request(10+i, row['action'], row['args']) for i, row in enumerate(mcp_rows)]
    archive = output / 'layer-batch.mcreplay'
    requests += [request(900, 'node.catalog', {}), request(901, 'replay.export', dict(path=str(archive))),
                 request(902, 'layer.selection.set', dict(layer_ids=[1, 1])),
                 request(903, 'layer.batch', dict(action='layer.delete', layer_ids=[1, 2, 3, 4])),
                 request(904, 'layer.set', dict(layer_id=3, pixel_locked=True)),
                 request(905, 'layer.batch', dict(action='layer.flip_horizontal', layer_ids=[1, 3])),
                 request(906, 'canvas.snapshot', {}), request(907, 'layer.query', dict(selected=True))]
    replies = run_mcp(cli, flags, requests)
    tools = {tool['name']: tool for tool in replies[2]['result']['tools']}
    assert {'layer.selection.set', 'layer.batch', 'layer.query'} <= tools.keys()
    assert tools['layer.query']['inputSchema']['properties']['selected']['type'] == 'boolean'
    assert validate([content(replies[10+i]) for i in range(len(mcp_rows))], mcp_positions) == expected
    catalog = json.dumps(content(replies[900]))
    assert 'action.layer.selection.set' in catalog and 'action.layer.batch' in catalog
    assert all(replies[i]['result']['isError'] for i in [902, 903, 905])
    assert base64.b64decode(content(replies[906])['data']) == expected[1]
    assert content(replies[907])['selected_layer_ids'] == [1, 3]
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
    print('JSONL/MCP: stable multi-selection, batches, one-step undo, nodes, project and DLL-independent replay passed.')
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
            before = call('layer.query', {})
            png = base64.b64decode(call('canvas.snapshot', {})['data'])
            time.sleep(.3)
            capture(f'{language}-layer-multi-selection.png')
            after = call('layer.query', {})
            assert after['layers'] == before['layers'] and after['selected_layer_ids'] == [1, 3]
            assert base64.b64decode(call('canvas.snapshot', {})['data']) == png
        assert call('replay.verify', {})['valid']
        call('ui.dispatch', dict(id='close', value=True))
        assert process.wait(timeout=10) == 0
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=10)
    print('GUI batch parity and four-language multi-selection captures passed.')
    print(output)


if __name__ == '__main__':
    main()
