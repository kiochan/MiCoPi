"""Verify 26 blend modes through JSONL, MCP and the live desktop Session.

Fixed independent encoded-RGB references check exact pixels, half-opacity layer,
graph and isolated-group gates. Also check batch Undo, inert workflow drafts,
projects, DLL-independent outcome replay and four localized Layers captures.
An optional older compositor checks rejection without changing the live document.
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
MODES = ['normal', 'multiply', 'screen', 'overlay', 'darken', 'lighten', 'add', 'difference',
         'color_burn', 'linear_burn', 'darker_color', 'color_dodge', 'lighter_color',
         'soft_light', 'hard_light', 'vivid_light', 'linear_light', 'pin_light', 'hard_mix',
         'exclusion', 'subtract', 'divide', 'hue', 'saturation', 'color', 'luminosity']
# Backdrop #804020 and source #4080C0, alpha 255. Original eight modes retain
# their historical integer stages; appended modes use normalized equations.
OPAQUE = [0x4080c0, 0x202018, 0xa0a0c8, 0x414030, 0x404020, 0x8080c0, 0xc0c0e0,
          0x4040a0, 0, 0, 0x804020, 0xab8182, 0x4080c0, 0x60403c, 0x404191,
          0x024041, 0x0141a1, 0x804081, 0, 0x8080b0, 0x400000, 0xff802b,
          0x295989, 0x903b10, 0x1c5c9c, 0xa46444]


def rgba(mode, amount=128):
    # Opaque backdrop: independent weighted path after blend-channel rounding.
    channels = [((255-amount)*((0x804020 >> shift) & 255)
                 + amount*((OPAQUE[mode] >> shift) & 255)+127)//255 for shift in [16, 8, 0]]
    return bytes(channels+[255])


def image_graph(mode, linked=False, amount=128/255):
    return dict(enabled=True, linked=linked, nodes=[
        dict(id=0, position=[0, 0], kind='layer', index=0),
        dict(id=1, position=[0, 200], kind='layer', index=1),
        dict(id=2, position=[300, 0], kind='blend', mode=mode, opacity=amount),
        dict(id=3, position=[600, 0], kind='output'),
    ], edges=[dict(**{'from': 0}, to=2, input=0), dict(**{'from': 1}, to=2, input=1),
              dict(**{'from': 2}, to=3, input=0)])


def commands(output, prefix):
    rows, positions = [], {}

    def add(key, operation, **args):
        positions[key] = len(rows)
        rows.append(dict(action=operation, args=args))

    add('new', 'canvas.new', width=16, height=12)
    add('paper', 'layer.fill_color', color='#804020')
    add('paper-name', 'layer.rename', name='Paper 紙')
    add('add', 'layer.add')
    add('ink', 'layer.fill_color', color='#4080C0')
    add('ink-name', 'layer.rename', name='Ink 墨')
    for index, mode in enumerate(MODES):
        add(f'layer-{index}', 'layer.set', layer_id=2, blend_mode=mode, opacity=128/255)
        add(f'layer-png-{index}', 'canvas.snapshot')
    add('batch', 'layer.batch', action='layer.set', layer_ids=[2, 1], args=dict(blend_mode='color', opacity=1))
    add('batch-png', 'canvas.snapshot')
    add('batch-undo', 'history.undo')
    add('batch-undo-png', 'canvas.snapshot')
    draft = dict(version=1, nodes=[dict(id=1, type='action.layer.set', position=[0, 0],
                                      parameters=dict(layer_id=2, blend_mode='soft_light', opacity=128/255))], edges=[])
    add('workflow-set', 'workflow.set', graph=draft)
    add('workflow-dry', 'workflow.run', atomic=True, dry_run=True)
    add('workflow-inert', 'canvas.snapshot')
    add('workflow-run', 'workflow.run', atomic=True)
    add('workflow-png', 'canvas.snapshot')
    add('workflow-undo', 'history.undo')
    add('source-reset', 'layer.set', layer_id=2, blend_mode='normal', opacity=1)
    for index, mode in enumerate(MODES):
        add(f'graph-{index}', 'graph.set', graph=image_graph(mode))
        add(f'graph-png-{index}', 'canvas.snapshot')
    add('linked-reset', 'graph.set', graph=image_graph('normal', linked=True, amount=1))
    add('group', 'group.create', members=[dict(kind='layer', id=2)], name='Character ink 彩')
    for index, mode in enumerate(MODES):
        add(f'group-{index}', 'group.set', group_id=1, blend_mode=mode, opacity=128/255)
        add(f'group-png-{index}', 'canvas.snapshot')
    disabled = image_graph('normal', linked=True, amount=1)
    disabled['enabled'] = False
    add('filters-disabled', 'graph.set', graph=disabled)
    add('disabled-png', 'canvas.snapshot')
    add('final-query', 'layer.query')
    add('final-groups', 'group.query')
    add('final-graph', 'graph.read')
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

    def matches(key, index, amount=128):
        w, h, data = pixels(image(key))
        assert (w, h) == (16, 12), key
        assert data == rgba(index, amount)*(w*h), f'{key}: expected {rgba(index, amount).hex()}, got {data[:4].hex()}'

    for index, mode in enumerate(MODES):
        for scope in ['layer', 'graph', 'group']:
            matches(f'{scope}-png-{index}', index)
        assert at(f'layer-{index}')['layers'][1]['blend_mode'] == mode
    matches('batch-png', 24, 255)
    matches('batch-undo-png', 25)
    assert at('workflow-dry')['ok'] and at('workflow-run')['ok']
    matches('workflow-inert', 25)
    matches('workflow-png', 13)
    matches('disabled-png', 25)
    matches('reopened-png', 25)
    assert at('final-query')['layers'] == at('reopened-query')['layers']
    assert at('final-groups')['groups'] == at('reopened-groups')['groups']
    assert at('final-groups')['groups'][0]['blend_mode'] == 'luminosity'
    assert at('final-graph')['mode'] == 'grouped' and not at('final-graph')['graph']['enabled']
    assert at('verify')['valid']
    return at('final-query')['layers'], at('final-groups')['groups'], image('reopened-png')


def request(number, name, args):
    return dict(jsonrpc='2.0', id=number, method='tools/call', params=dict(name=name, arguments=args))


def content(reply):
    assert 'error' not in reply, reply
    assert not reply['result'].get('isError'), reply
    return reply['result']['structuredContent']['result']


def run_mcp(cli, flags, requests):
    run = subprocess.run([str(cli), 'mcp', *flags], input='\n'.join(json.dumps(r) for r in requests)+'\n',
                         capture_output=True, text=True, encoding='utf-8', timeout=180)
    assert run.returncode == 0, run.stderr+run.stdout[-3000:]
    return {row['id']: row for row in map(json.loads, run.stdout.splitlines()) if 'id' in row}


def legacy_check(cli, flags, initialized, output, dll):
    plugins = output / 'legacy-plugins'
    shutil.copytree(ROOT / 'plugins', plugins)
    shutil.copy2(dll, plugins / 'compositing/micopi_compositing.dll')
    legacy_flags = ['--plugins', str(plugins), *flags[2:]]
    replies = run_mcp(cli, legacy_flags, initialized + [
        request(3, 'canvas.new', dict(width=2, height=1)),
        request(4, 'layer.fill_color', dict(color='#804020')),
        request(5, 'canvas.state', {}), request(6, 'canvas.snapshot', {}),
        request(7, 'layer.set', dict(blend_mode='hue', visible=False)),
        request(8, 'canvas.state', {}), request(9, 'canvas.snapshot', {}),
        request(10, 'document.open', dict(path=str(output/'batch.mcp'))),
        request(11, 'canvas.state', {}), request(12, 'canvas.snapshot', {}),
        request(13, 'replay.verify', {}),
    ])
    assert replies[7]['result']['structuredContent']['error']['code'] == 'blend_mode_unavailable'
    assert replies[10]['result'].get('isError'), 'Old compositor must reject a project containing a new group mode'
    assert content(replies[5]) == content(replies[8]) == content(replies[11])
    assert content(replies[6])['data'] == content(replies[9])['data'] == content(replies[12])['data']
    assert content(replies[13])['valid']
    print('Older compositor: hidden new mode and extended-mode project rejected before replacing pixels/state.')


def main():
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument('--cli', default='dist/micopi-cli.exe')
    parser.add_argument('--gui', default='dist/MiCoPi.exe')
    parser.add_argument('--skip-gui', action='store_true')
    parser.add_argument('--legacy-compositor', type=Path, help='Optional previous 16/24-byte compositor DLL fixture')
    args = parser.parse_args()
    cli, gui = (ROOT/args.cli).resolve(), (ROOT/args.gui).resolve()
    output = ROOT/'target/test-output/blend-modes-smoke'/str(time.time_ns())
    output.mkdir(parents=True)
    plugins, profile = output/'plugins', output/'profile'
    shutil.copytree(ROOT/'plugins', plugins)
    profile.mkdir()
    (profile/'locales').mkdir()
    for language in ['zh-CN', 'ja', 'ko']:
        shutil.copy2(ROOT/'locales'/f'{language}.json', profile/'locales'/f'{language}.json')
    settings = profile/'settings.json'
    settings.write_text(json.dumps(dict(version=1, custom=dict(keep=42))), encoding='utf-8')
    flags = ['--plugins', str(plugins), '--config-dir', str(profile)]
    rows, positions = commands(output, 'batch')
    script = output/'blend-modes.jsonl'
    script.write_text('\n'.join(json.dumps(row) for row in rows)+'\n', encoding='utf-8')
    run = subprocess.run([str(cli), 'run', str(script), *flags], capture_output=True,
                         text=True, encoding='utf-8', timeout=180)
    assert run.returncode == 0, run.stderr+run.stdout[-3000:]
    records = [json.loads(line) for line in run.stdout.splitlines()]
    assert len(records) == len(rows) and all(row['ok'] for row in records), records[-3:]
    expected = validate([row['data']['result'] for row in records], positions)
    initialized = [dict(jsonrpc='2.0', id=1, method='initialize', params=dict(protocolVersion='2025-11-25')),
                   dict(jsonrpc='2.0', method='notifications/initialized')]
    mcp_rows, mcp_positions = commands(output, 'mcp')
    archive = output/'blend-modes.mcreplay'
    requests = initialized+[dict(jsonrpc='2.0', id=2, method='tools/list', params={})]
    requests += [request(10+i, row['action'], row['args']) for i, row in enumerate(mcp_rows)]
    requests += [request(900, 'graph.catalog', {}), request(901, 'node.catalog', {}),
                 request(902, 'replay.export', dict(path=str(archive)))]
    replies = run_mcp(cli, flags, requests)
    assert validate([content(replies[10+i]) for i in range(len(mcp_rows))], mcp_positions) == expected
    tools = {tool['name']: tool for tool in replies[2]['result']['tools']}
    for action in ['layer.set', 'group.set']:
        assert tools[action]['inputSchema']['properties']['blend_mode']['enum'] == MODES
    assert content(replies[900])['blend_modes'] == MODES
    catalog = json.dumps(content(replies[901]))
    assert all(mode in catalog for mode in MODES) and 'compositing.blend' in catalog and 'action.layer.batch' in catalog
    minimal = output/'minimal-plugins'
    minimal.mkdir()
    for name in ['core', 'replay']:
        shutil.copytree(ROOT/'plugins'/name, minimal/name)
    playback = run_mcp(cli, ['--plugins', str(minimal), '--config-dir', str(profile)], initialized+[
        request(3, 'replay.load', dict(path=str(archive))),
        request(4, 'replay.verify', dict(source='loaded')),
        request(5, 'replay.snapshot', dict(source='loaded')),
    ])
    assert content(playback[4])['valid']
    assert base64.b64decode(content(playback[5])['data']) == expected[2]
    if args.legacy_compositor:
        legacy_check(cli, flags, initialized, output, args.legacy_compositor.resolve())
    print(f'JSONL/MCP: {len(rows)} shared actions; all 26 layer, graph and group modes, batch Undo, workflow, project and DLL-independent replay passed.')
    if args.skip_gui:
        print(output)
        return
    startup = subprocess.STARTUPINFO()
    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = 0
    process = subprocess.Popen([str(gui), *flags], startupinfo=startup)
    endpoint = profile/'session.json'

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
        raise AssertionError(f'GUI revision did not settle: {previous}')

    try:
        deadline = time.monotonic()+20
        while not endpoint.exists():
            assert process.poll() is None and time.monotonic() < deadline, 'GUI startup failed'
            time.sleep(.05)
        gui_rows, gui_positions = commands(output, 'gui')
        assert validate([call(row['action'], row['args']) for row in gui_rows], gui_positions) == expected
        call('canvas.new', dict(width=320, height=220, background='#637489'))
        call('layer.rename', dict(name='Paper'))
        call('layer.add', {})
        call('layer.rename', dict(name='Ink 墨'))
        call('brush.set', dict(radius=16, color='#F9D8E6', stabilizer=.35, pressure_size=True, pressure_opacity=True))
        call('canvas.stroke', dict(points=[dict(x=25, y=165, pressure=.2, pen=True, time=0),
                                           dict(x=140, y=45, pressure=1., pen=True, time=60),
                                           dict(x=290, y=150, pressure=.4, pen=True, time=120)]))
        call('layer.set', dict(blend_mode='soft_light'))
        call('window.command', dict(panel='layers', operation='show'))
        for language in ['en', 'zh-CN', 'ja', 'ko']:
            call('settings.update', dict(path=str(settings), ui=dict(language=language)))
            assert json.loads(settings.read_text(encoding='utf-8-sig'))['custom']['keep'] == 42
            settled_revision()
            query = call('layer.query', {})
            graph = call('graph.read', {})
            png = call('canvas.snapshot', {})['data']
            assert next(row for row in query['layers'] if row['active'])['blend_mode'] == 'soft_light'
            path = output/f'{language}-blend-modes.png'
            call('ui.capture', dict(path=str(path)))
            deadline = time.monotonic()+10
            while not path.exists():
                assert time.monotonic() < deadline, 'Capture timeout'
                time.sleep(.05)
            current = call('layer.query', {})
            assert {k: v for k, v in current.items() if k != 'revision'} == {
                k: v for k, v in query.items() if k != 'revision'}
            assert call('graph.read', {})['graph'] == graph['graph']
            assert call('canvas.snapshot', {})['data'] == png
        assert call('replay.verify', {})['valid']
        call('ui.dispatch', dict(id='close', value=True))
        assert process.wait(timeout=10) == 0
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=10)
    print('GUI exact-pixel parity and four-language Layers captures with Soft Light selected passed.')
    print(output)


if __name__ == '__main__':
    main()
