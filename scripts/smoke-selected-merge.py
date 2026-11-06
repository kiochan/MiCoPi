"""Verify explicit selected-only merging across JSONL, MCP, GUI and replay."""
import argparse
import base64
from fractions import Fraction
import json
from pathlib import Path
import shutil
import subprocess
import time

from png_checks import pixels

ROOT = Path(__file__).resolve().parents[1]


def rounded(value):
    return (2*value.numerator+value.denominator)//(2*value.denominator)


def over(before, after, mode='normal'):
    ab, aa = Fraction(before >> 24, 255), Fraction(after >> 24, 255)
    alpha = rounded((aa+ab*(1-aa))*255)
    if not alpha:
        return 0
    out = alpha << 24
    for shift in [16, 8, 0]:
        b, a = Fraction((before >> shift) & 255, 255), Fraction((after >> shift) & 255, 255)
        blended = a if mode == 'normal' else 1-(1-a)*(1-b)
        blend_byte = rounded(blended*255)
        value = (ab*(1-aa)*b+aa*(1-ab)*a+aa*ab*Fraction(blend_byte, 255))*255/Fraction(alpha, 255)+Fraction(127, 255)
        out |= (value.numerator//value.denominator) << shift
    return out


def mix(before, after, amount):
    ab, aa = before >> 24, after >> 24
    weight = ab*(255-amount)+aa*amount
    alpha = (weight+127)//255
    if not alpha:
        return 0
    out = alpha << 24
    for shift in [16, 8, 0]:
        n = ((before >> shift) & 255)*ab*(255-amount)+((after >> shift) & 255)*aa*amount
        out |= ((2*n+weight)//(2*weight)) << shift
    return out


def attenuate(pixel, amount):
    alpha = (pixel >> 24)*amount//255
    return (pixel & 0xffffff) | (alpha << 24) if alpha else 0


PROJECTED = over(mix(0, 0x80e04020, 128), attenuate(0x903399cc, 179), 'screen')
CROSS_FINAL = over(over(over(mix(0, 0xff00ff00, 128), attenuate(0xffffff00, 179), 'screen'), PROJECTED), 0x40885522)


def commands(output, prefix):
    rows, positions = [], {}

    def add(key, operation, **args):
        positions[key] = len(rows)
        rows.append(dict(action=operation, args=args))

    add('new', 'canvas.new', width=16, height=12)
    add('red', 'layer.fill_color', color='#FF0000')
    add('unselected-add', 'layer.add')
    add('green', 'layer.fill_color', color='#00FF00')
    add('protect-green', 'layer.set', pixel_locked=True)
    add('blue-add', 'layer.add')
    add('right-mask', 'selection.shape', kind='rectangle', x=8, y=0, width=8, height=12)
    add('right-blue', 'layer.fill_color', color='#0000FF')
    add('deselect-right', 'selection.deselect')
    add('blue-name', 'layer.rename', name='Highlight 青')
    add('blue-flags', 'layer.set', color_label='violet', position_locked=True)
    add('left-mask', 'selection.shape', kind='rectangle', x=0, y=0, width=4, height=12)
    add('multi-select', 'layer.selection.set', layer_ids=[1, 3], active_layer_id=1)
    add('before', 'canvas.snapshot')
    add('plan', 'layer.merge_selected.plan', layer_ids=[3, 1])
    draft = dict(version=1, nodes=[
        dict(id=1, type='action.layer.merge_selected', position=[0, 0], parameters=dict(layer_ids=[3, 1])),
        dict(id=2, type='action.layer.rename', position=[400, 0], parameters=dict(name='Merged selected 彩')),
    ], edges=[dict(**{'from': 1}, output='layer_id', to=2, input='layer_id')])
    add('draft', 'workflow.set', graph=draft)
    add('dry', 'workflow.run', atomic=True, dry_run=True)
    add('inert', 'canvas.snapshot')
    add('merge', 'layer.merge_selected', layer_ids=[3, 1])
    add('merged-query', 'layer.query')
    add('merged', 'canvas.snapshot')
    add('undo', 'history.undo')
    add('undo-query', 'layer.query')
    add('undone', 'canvas.snapshot')
    add('redo', 'history.redo')
    add('redone', 'canvas.snapshot')
    add('restore', 'history.undo')
    add('contextual', 'layer.merge')
    add('contextual-png', 'canvas.snapshot')
    add('contextual-undo', 'history.undo')
    add('workflow', 'workflow.run', atomic=True)
    add('workflow-query', 'layer.query')
    add('workflow-png', 'canvas.snapshot')
    add('cross-new', 'canvas.new', width=16, height=12)
    for number, color, alpha in [(1, '#E04020', 128), (2, '#00FF00', 255), (3, '#3399CC', 144),
                                  (4, '#FFFF00', 255), (5, '#885522', 64)]:
        if number > 1:
            add(f'cross-add-{number}', 'layer.add')
        add(f'cross-fill-{number}', 'layer.fill_color', color=color)
        add(f'cross-opacity-{number}', 'layer.set', opacity=alpha/255)
    add('left-group', 'group.create', members=[dict(kind='layer', id=1), dict(kind='layer', id=2)], name='Left pass')
    add('left-pass', 'group.set', group_id=1, compositing='pass_through', opacity=128/255)
    add('right-group', 'group.create', members=[dict(kind='layer', id=3), dict(kind='layer', id=4)], name='Right isolated')
    add('right-screen', 'group.set', group_id=2, blend_mode='screen', opacity=179/255)
    add('cross-plan', 'layer.merge_selected.plan', layer_ids=[3, 1])
    add('cross-merge', 'layer.merge_selected', layer_ids=[3, 1])
    add('cross-query', 'layer.query')
    add('cross-groups', 'group.query')
    add('cross-png', 'canvas.snapshot')
    add('hide-survivors', 'layer.batch', **{'action': 'layer.set', 'layer_ids': [2, 4, 5], 'args': dict(visible=False)})
    add('projected-png', 'canvas.snapshot')
    add('show-survivors', 'history.undo')
    add('final-png', 'canvas.snapshot')
    add('save', 'document.save', path=str(output/f'{prefix}.mcp'))
    add('open', 'document.open', path=str(output/f'{prefix}.mcp'))
    add('reopened-query', 'layer.query')
    add('reopened-groups', 'group.query')
    add('reopened-png', 'canvas.snapshot')
    add('verify', 'replay.verify')
    return rows, positions


def validate(values, positions):
    at = lambda key: values[positions[key]]
    image = lambda key: base64.b64decode(at(key)['data'])

    def expected_rows(left, right):
        rgba = lambda p: bytes((p >> s) & 255 for s in [16, 8, 0, 24])
        return (rgba(left)*8+rgba(right)*8)*12

    before, after = expected_rows(0xff00ff00, 0xff0000ff), expected_rows(0xffff0000, 0xff0000ff)
    for key in ['before', 'inert', 'undone']:
        w, h, data = pixels(image(key))
        assert (w, h) == (16, 12) and data == before, key
    for key in ['merged', 'redone', 'contextual-png', 'workflow-png']:
        assert pixels(image(key)) == (16, 12, after), key
    assert before != after, 'Selected-only merging deliberately changes the intervening layer order'
    plan = at('plan')
    assert plan['layer_ids'] == [1, 3] and plan['common_parent_id'] == 0 and plan['parent_index'] == 1
    assert plan['layer_id'] == 3 and plan['interleaved_unselected_ids'] == [2] and plan['changes_stacking']
    assert plan['consumed_hidden_ids'] == [] and plan['backdrop_policy'] == 'selected_only_transparent'
    assert plan['output_buffer_bytes'] == 16*12*4 and at('merge')['layer_id'] == 3
    rows = at('merged-query')['layers']
    assert [r['id'] for r in rows] == [2, 3] and rows[0]['pixel_locked']
    assert rows[1]['name'] == 'Highlight 青' and rows[1]['color_label'] == 'violet' and rows[1]['position_locked']
    assert rows[1]['blend_mode'] == 'normal' and rows[1]['opacity'] == 1
    assert at('merged-query')['selected_layer_ids'] == [3]
    assert at('undo-query')['selected_layer_ids'] == [1, 3] and at('undo-query')['active_layer_id'] == 1
    assert at('contextual')['layer_id'] == 3 and at('dry')['ok'] and at('workflow')['ok']
    assert next(r for r in at('workflow-query')['layers'] if r['id'] == 3)['name'] == 'Merged selected 彩'
    plan = at('cross-plan')
    assert plan['layer_ids'] == [1, 3] and plan['common_parent_id'] == 0 and plan['parent_index'] == 2
    assert plan['interleaved_unselected_ids'] == [2, 4] and plan['changes_stacking']
    assert [r['id'] for r in at('cross-query')['layers']] == [2, 4, 3, 5]
    groups = at('cross-groups')
    assert len(groups['groups']) == 2 and {g['id']: g['layer_ids'] for g in groups['groups']} == {1: [2], 2: [4]}
    assert groups['roots'] == [dict(kind='group', id=1), dict(kind='group', id=2), dict(kind='layer', id=3), dict(kind='layer', id=5)]
    for key, expected in [('cross-png', CROSS_FINAL), ('projected-png', PROJECTED), ('final-png', CROSS_FINAL), ('reopened-png', CROSS_FINAL)]:
        expected = bytes((expected >> shift) & 255 for shift in [16, 8, 0, 24])*(16*12)
        assert pixels(image(key)) == (16, 12, expected), key
    assert at('cross-query')['layers'] == at('reopened-query')['layers']
    assert at('cross-groups')['groups'] == at('reopened-groups')['groups'] and at('verify')['valid']
    return at('cross-query')['layers'], at('cross-groups')['groups'], image('final-png')


def request(number, name, args):
    return dict(jsonrpc='2.0', id=number, method='tools/call', params=dict(name=name, arguments=args))


def content(reply):
    assert 'error' not in reply and not reply['result'].get('isError'), reply
    return reply['result']['structuredContent']['result']


def run_mcp(cli, flags, requests):
    result = subprocess.run([str(cli), 'mcp', *flags], input='\n'.join(json.dumps(r) for r in requests)+'\n',
                            capture_output=True, text=True, encoding='utf-8', timeout=180)
    assert result.returncode == 0, result.stderr+result.stdout[-3000:]
    return {r['id']: r for r in map(json.loads, result.stdout.splitlines()) if 'id' in r}


def main():
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument('--cli', default='dist/micopi-cli.exe')
    parser.add_argument('--gui', default='dist/MiCoPi.exe')
    parser.add_argument('--skip-gui', action='store_true')
    args = parser.parse_args()
    cli, gui = (ROOT/args.cli).resolve(), (ROOT/args.gui).resolve()
    output = ROOT/'target/test-output/selected-merge-smoke'/str(time.time_ns())
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
    script = output/'selected-merge.jsonl'
    script.write_text('\n'.join(json.dumps(r) for r in rows)+'\n', encoding='utf-8')
    result = subprocess.run([str(cli), 'run', str(script), *flags], capture_output=True,
                            text=True, encoding='utf-8', timeout=180)
    assert result.returncode == 0, result.stderr+result.stdout[-3000:]
    records = [json.loads(line) for line in result.stdout.splitlines()]
    assert len(records) == len(rows) and all(r['ok'] for r in records), [r for r in records if not r['ok']]
    expected = validate([r['data']['result'] for r in records], positions)
    initialized = [dict(jsonrpc='2.0', id=1, method='initialize', params=dict(protocolVersion='2025-11-25')),
                   dict(jsonrpc='2.0', method='notifications/initialized')]
    mcp_rows, mcp_positions = commands(output, 'mcp')
    archive = output/'selected-merge.mcreplay'
    requests = initialized+[dict(jsonrpc='2.0', id=2, method='tools/list', params={})]
    requests += [request(20+i, row['action'], row['args']) for i, row in enumerate(mcp_rows)]
    requests += [request(900, 'node.catalog', {}), request(901, 'replay.export', dict(path=str(archive))),
                 request(902, 'layer.merge_selected.open', dict(layer_ids=[2, 3]))]
    replies = run_mcp(cli, flags, requests)
    assert validate([content(replies[20+i]) for i in range(len(mcp_rows))], mcp_positions) == expected
    tools = {t['name']: t for t in replies[2]['result']['tools']}
    assert 'layer.merge_selected.plan' in tools and 'layer.merge' in tools
    assert 'action.layer.merge_selected' in json.dumps(content(replies[900]))
    assert replies[902]['result']['structuredContent']['error']['code'] == 'ui_unavailable'
    minimal = output/'minimal-plugins'
    minimal.mkdir()
    for name in ['core', 'replay']:
        shutil.copytree(ROOT/'plugins'/name, minimal/name)
    replayed = run_mcp(cli, ['--plugins', str(minimal), '--config-dir', str(profile)], initialized+[
        request(3, 'replay.load', dict(path=str(archive))), request(4, 'replay.verify', dict(source='loaded')),
        request(5, 'replay.snapshot', dict(source='loaded')),
    ])
    assert content(replayed[4])['valid'] and base64.b64decode(content(replayed[5])['data']) == expected[2]
    print(f'JSONL/MCP: {len(rows)} shared actions; selected-only pixels, plans, cross-parent policies, typed workflow result, Undo, project and DLL-independent replay passed.')
    if args.skip_gui:
        print(output)
        return
    startup = subprocess.STARTUPINFO()
    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = 0
    process = subprocess.Popen([str(gui), *flags], startupinfo=startup)
    endpoint = profile/'session.json'

    def call(name, arguments):
        result = subprocess.run([str(cli), 'call', name, json.dumps(arguments), '--attach', str(endpoint)],
                                capture_output=True, text=True, encoding='utf-8', timeout=30)
        assert result.returncode == 0, result.stderr+result.stdout[-2000:]
        return json.loads(result.stdout)['result']

    def settle(quiet=.75, timeout=8):
        previous, changed, deadline = None, time.monotonic(), time.monotonic()+timeout
        while time.monotonic() < deadline:
            revision = call('canvas.state', {})['revision']
            if revision != previous:
                previous, changed = revision, time.monotonic()
            elif time.monotonic()-changed >= quiet:
                return revision
            time.sleep(.1)
        raise AssertionError('GUI revision did not settle')

    try:
        deadline = time.monotonic()+20
        while not endpoint.exists():
            assert process.poll() is None and time.monotonic() < deadline, 'GUI startup failed'
            time.sleep(.05)
        gui_rows, gui_positions = commands(output, 'gui')
        assert validate([call(r['action'], r['args']) for r in gui_rows], gui_positions) == expected
        call('canvas.new', dict(width=320, height=220, background='#637489'))
        call('layer.rename', dict(name='Paper'))
        call('layer.add', {})
        call('layer.rename', dict(name='Guide'))
        call('brush.set', dict(radius=18, color='#8699B5', opacity=.5, stabilizer=.35))
        call('canvas.stroke', dict(points=[dict(x=20, y=130, pressure=.5, pen=True, time=0), dict(x=295, y=90, pressure=1., pen=True, time=80)]))
        call('layer.add', {})
        call('layer.rename', dict(name='Ink 墨'))
        call('brush.set', dict(radius=16, color='#F9D8E6', opacity=1., stabilizer=.35, pressure_size=True, pressure_opacity=True))
        call('canvas.stroke', dict(points=[dict(x=25, y=165, pressure=.2, pen=True, time=0), dict(x=140, y=45, pressure=1., pen=True, time=60), dict(x=290, y=150, pressure=.4, pen=True, time=120)]))
        call('layer.selection.set', dict(layer_ids=[1, 3], active_layer_id=3))
        call('window.command', dict(panel='layers', operation='show'))
        for language in ['en', 'zh-CN', 'ja', 'ko']:
            call('settings.update', dict(path=str(settings), ui=dict(language=language)))
            assert json.loads(settings.read_text(encoding='utf-8-sig'))['custom']['keep'] == 42
            settle()
            query, actual, png = call('layer.query', {}), call('graph.read', {}), call('canvas.snapshot', {})['data']
            call('layer.merge_selected.open', dict(layer_ids=[3, 1]))
            settle(quiet=.3)
            path = output/f'{language}-selected-merge.png'
            call('ui.capture', dict(path=str(path)))
            deadline = time.monotonic()+10
            while not path.exists():
                assert time.monotonic() < deadline, 'Capture timeout'
                time.sleep(.05)
            call('ui.dispatch', dict(id='command', value='Cancel'))
            current = call('layer.query', {})
            assert {k: v for k, v in current.items() if k != 'revision'} == {k: v for k, v in query.items() if k != 'revision'}
            assert call('graph.read', {})['graph'] == actual['graph'] and call('canvas.snapshot', {})['data'] == png
        assert call('replay.verify', {})['valid']
        call('ui.dispatch', dict(id='close', value=True))
        assert process.wait(timeout=10) == 0
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=10)
    print('GUI parity and four-language optional merge-review open/cancel captures passed.')
    print(output)


if __name__ == '__main__':
    main()
