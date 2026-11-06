"""Check Pass Through groups across JSONL, MCP, GUI, projects and verified replay."""
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
BACKDROP, SOURCE = 0x7f804020, 0xbf4080c0


def half_up(value):
    return (2*value.numerator+value.denominator)//(2*value.denominator)


def over(backdrop, source, blend):
    ab, aa = Fraction(backdrop >> 24, 255), Fraction(source >> 24, 255)
    alpha = half_up((aa+ab*(1-aa))*255)
    if alpha == 0:
        return 0
    output = alpha << 24
    for shift in [16, 8, 0]:
        b, a, mixed = [Fraction((p >> shift) & 255, 255) for p in [backdrop, source, blend]]
        covered = (ab*(1-aa)*b+aa*(1-ab)*a+aa*ab*mixed)*255/Fraction(alpha, 255)+Fraction(127, 255)
        output |= (covered.numerator//covered.denominator) << shift
    return output


def mix(before, after, amount):
    ab, aa = before >> 24, after >> 24
    weight = ab*(255-amount)+aa*amount
    alpha = (weight+127)//255
    if alpha == 0:
        return 0
    output = alpha << 24
    for shift in [16, 8, 0]:
        numerator = ((before >> shift) & 255)*ab*(255-amount)+((after >> shift) & 255)*aa*amount
        output |= ((2*numerator+weight)//(2*weight)) << shift
    return output


FLAT = over(BACKDROP, SOURCE, 0xff202018)
HALF = mix(BACKDROP, FLAT, 137)
ISOLATED = over(BACKDROP, SOURCE, SOURCE)
ISOLATED_SCREEN = over(BACKDROP, (SOURCE & 0xffffff) | (((SOURCE >> 24)*137//255) << 24), 0xffa0a0c8)


def graph(enabled=False, has_mix=False, connected=True):
    nodes = [dict(id=0, position=[0, 0], kind='layer', index=0),
             dict(id=1, position=[0, 200], kind='layer', index=1),
             dict(id=2, position=[300, 0], kind='mix' if has_mix else 'blend', opacity=1),
             dict(id=3, position=[600, 0], kind='output')]
    if not has_mix:
        nodes[2]['mode'] = 'multiply'
    return dict(enabled=enabled, linked=not has_mix, nodes=nodes,
                edges=[dict(**{'from': 0}, to=2, input=0), dict(**{'from': 1}, to=2, input=1),
                       dict(**{'from': 2 if connected else 0}, to=3, input=0)])


def commands(output, prefix):
    rows, positions = [], {}

    def add(key, operation, **args):
        positions[key] = len(rows)
        rows.append(dict(action=operation, args=args))

    add('new', 'canvas.new', width=16, height=12)
    add('paper', 'layer.fill_color', color='#804020')
    add('paper-alpha', 'layer.set', opacity=127/255)
    add('ink-add', 'layer.add')
    add('ink', 'layer.fill_color', color='#4080C0')
    add('ink-alpha-mode', 'layer.set', opacity=191/255, blend_mode='multiply')
    add('flat', 'canvas.snapshot')
    add('create', 'group.create', members=[dict(kind='layer', id=2)], name='Character ink 彩')
    add('isolated', 'canvas.snapshot')
    add('pass', 'group.set', group_id=1, compositing='pass_through', blend_mode='screen')
    add('full', 'canvas.snapshot')
    add('pass-query', 'group.query')
    add('half', 'group.set', group_id=1, opacity=137/255)
    add('half-png', 'canvas.snapshot')
    add('half-undo', 'history.undo')
    add('half-undo-png', 'canvas.snapshot')
    add('half-redo', 'history.redo')
    add('half-redo-png', 'canvas.snapshot')
    draft = dict(version=1, nodes=[dict(id=1, type='action.group.set', position=[0, 0],
                                      parameters=dict(group_id=1, opacity=89/255, compositing='pass_through'))], edges=[])
    add('draft', 'workflow.set', graph=draft)
    add('dry', 'workflow.run', atomic=True, dry_run=True)
    add('inert', 'canvas.snapshot')
    add('workflow', 'workflow.run', atomic=True)
    add('workflow-png', 'canvas.snapshot')
    add('workflow-undo', 'history.undo')
    add('disable-effects', 'graph.set', graph=graph())
    add('disabled-png', 'canvas.snapshot')
    add('empty', 'group.create', members=[], name='Empty notes')
    add('empty-pass', 'group.set', group_id=2, compositing='pass_through', opacity=.5)
    add('empty-png', 'canvas.snapshot')
    add('hide', 'group.set', group_id=1, visible=False)
    add('hidden-png', 'canvas.snapshot')
    add('show', 'group.set', group_id=1, visible=True)
    add('isolate-again', 'group.set', group_id=1, compositing='isolated')
    add('isolated-screen', 'canvas.snapshot')
    add('pass-again', 'group.set', group_id=1, compositing='pass_through')
    add('before-visible-groups', 'group.query')
    add('visible', 'layer.merge_visible')
    add('visible-png', 'canvas.snapshot')
    add('visible-undo', 'history.undo')
    add('restored-groups', 'group.query')
    add('visible-restored', 'canvas.snapshot')
    add('flatten', 'layer.flatten')
    add('flatten-png', 'canvas.snapshot')
    add('flatten-undo', 'history.undo')
    add('final-query', 'layer.query')
    add('final-groups', 'group.query')
    add('final-graph', 'graph.read')
    add('final-png', 'canvas.snapshot')
    add('save', 'document.save', path=str(output/f'{prefix}.mcp'))
    add('open', 'document.open', path=str(output/f'{prefix}.mcp'))
    add('reopened-groups', 'group.query')
    add('reopened-png', 'canvas.snapshot')
    add('verify', 'replay.verify')
    return rows, positions


def validate(values, positions):
    at = lambda key: values[positions[key]]
    images = {}
    for key, expected in [('flat', FLAT), ('isolated', ISOLATED), ('full', FLAT),
                          ('half-png', HALF), ('half-undo-png', FLAT), ('half-redo-png', HALF),
                          ('inert', HALF), ('workflow-png', mix(BACKDROP, FLAT, 89)),
                          ('disabled-png', HALF), ('empty-png', HALF), ('hidden-png', BACKDROP),
                          ('isolated-screen', ISOLATED_SCREEN), ('visible-png', HALF),
                          ('visible-restored', HALF), ('flatten-png', HALF),
                          ('final-png', HALF), ('reopened-png', HALF)]:
        png = base64.b64decode(at(key)['data'])
        width, height, rgba = pixels(png)
        channels = bytes([(expected >> shift) & 255 for shift in [16, 8, 0, 24]])
        assert (width, height) == (16, 12) and rgba == channels*(width*height), (key, channels.hex(), rgba[:4].hex())
        images[key] = png
    assert FLAT != ISOLATED and HALF != ISOLATED_SCREEN
    assert at('dry')['ok'] and at('workflow')['ok']
    info = next(g for g in at('pass-query')['groups'] if g['id'] == 1)
    assert info['compositing'] == 'pass_through' and info['blend_mode'] == 'screen'
    assert not info['mergeable'] and info['merge_error']
    assert at('before-visible-groups')['groups'] == at('restored-groups')['groups']
    final = at('final-groups')
    assert final['version'] == 2 and all(g['compositing'] == 'pass_through' for g in final['groups'])
    assert final['groups'] == at('reopened-groups')['groups']
    actual = at('final-graph')
    assert actual['renderer_mix'] and actual['mode'] == 'grouped' and not actual['graph']['enabled']
    assert any(n['kind'] == 'mix' for n in actual['effective_graph']['nodes'])
    assert at('verify')['valid']
    return at('final-query')['layers'], final['groups'], images['final-png']


def request(number, name, args):
    return dict(jsonrpc='2.0', id=number, method='tools/call', params=dict(name=name, arguments=args))


def content(reply):
    assert 'error' not in reply and not reply['result'].get('isError'), reply
    return reply['result']['structuredContent']['result']


def run_mcp(cli, flags, requests):
    run = subprocess.run([str(cli), 'mcp', *flags], input='\n'.join(json.dumps(r) for r in requests)+'\n',
                         capture_output=True, text=True, encoding='utf-8', timeout=120)
    assert run.returncode == 0, run.stderr+run.stdout[-3000:]
    return {r['id']: r for r in map(json.loads, run.stdout.splitlines()) if 'id' in r}


def legacy_check(cli, flags, initialized, output, dll):
    plugins = output/'legacy-plugins'
    shutil.copytree(ROOT/'plugins', plugins)
    shutil.copy2(dll, plugins/'compositing/micopi_compositing.dll')
    replies = run_mcp(cli, ['--plugins', str(plugins), *flags[2:]], initialized+[
        request(3, 'canvas.new', dict(width=2, height=1)), request(4, 'layer.add', {}),
        request(5, 'group.create', dict(members=[dict(kind='layer', id=2)], name='Old renderer')),
        request(6, 'group.set', dict(group_id=1, visible=False)),
        request(7, 'canvas.state', {}), request(8, 'canvas.snapshot', {}),
        request(9, 'graph.read', {}), request(10, 'group.set', dict(group_id=1, compositing='pass_through')),
        request(11, 'document.open', dict(path=str(output/'batch.mcp'))),
        request(12, 'canvas.state', {}), request(13, 'canvas.snapshot', {}), request(14, 'replay.verify', {}),
    ])
    assert not content(replies[9])['renderer_mix']
    assert replies[10]['result'].get('isError') and replies[11]['result'].get('isError')
    assert content(replies[7]) == content(replies[12])
    assert content(replies[8])['data'] == content(replies[13])['data'] and content(replies[14])['valid']
    print('Previous compositor: hidden Pass Through and project rejected without replacing state/pixels.')


def main():
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument('--cli', default='dist/micopi-cli.exe')
    parser.add_argument('--gui', default='dist/MiCoPi.exe')
    parser.add_argument('--skip-gui', action='store_true')
    parser.add_argument('--legacy-compositor', type=Path, help='Optional previous 32-byte compositor without Mix capability')
    args = parser.parse_args()
    cli, gui = (ROOT/args.cli).resolve(), (ROOT/args.gui).resolve()
    output = ROOT/'target/test-output/pass-through-smoke'/str(time.time_ns())
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
    script = output/'pass-through.jsonl'
    script.write_text('\n'.join(json.dumps(r) for r in rows)+'\n', encoding='utf-8')
    run = subprocess.run([str(cli), 'run', str(script), *flags], capture_output=True,
                         text=True, encoding='utf-8', timeout=120)
    assert run.returncode == 0, run.stderr+run.stdout[-3000:]
    records = [json.loads(line) for line in run.stdout.splitlines()]
    assert len(records) == len(rows) and all(r['ok'] for r in records), records[-3:]
    expected = validate([r['data']['result'] for r in records], positions)
    initialized = [dict(jsonrpc='2.0', id=1, method='initialize', params=dict(protocolVersion='2025-11-25')),
                   dict(jsonrpc='2.0', method='notifications/initialized')]
    mcp_rows, mcp_positions = commands(output, 'mcp')
    archive = output/'pass-through.mcreplay'
    requests = initialized+[dict(jsonrpc='2.0', id=2, method='tools/list', params={})]
    requests += [request(20+i, r['action'], r['args']) for i, r in enumerate(mcp_rows)]
    requests += [request(900, 'graph.catalog', {}), request(901, 'node.catalog', {}),
                 request(902, 'replay.export', dict(path=str(archive))), request(903, 'group.merge', dict(group_id=1)),
                 request(904, 'group.query', {}), request(905, 'canvas.snapshot', {})]
    replies = run_mcp(cli, flags, requests)
    assert validate([content(replies[20+i]) for i in range(len(mcp_rows))], mcp_positions) == expected
    tools = {t['name']: t for t in replies[2]['result']['tools']}
    assert tools['group.set']['inputSchema']['properties']['compositing']['enum'] == ['isolated', 'pass_through']
    assert content(replies[900])['renderer_mix'] and 'compositing.mix' in json.dumps(content(replies[901]))
    assert replies[903]['result'].get('isError')
    assert content(replies[904])['groups'] == expected[1]
    assert base64.b64decode(content(replies[905])['data']) == expected[2]
    minimal = output/'minimal-plugins'
    minimal.mkdir()
    for name in ['core', 'replay']:
        shutil.copytree(ROOT/'plugins'/name, minimal/name)
    playback = run_mcp(cli, ['--plugins', str(minimal), '--config-dir', str(profile)], initialized+[
        request(3, 'replay.load', dict(path=str(archive))), request(4, 'replay.verify', dict(source='loaded')),
        request(5, 'replay.snapshot', dict(source='loaded')),
    ])
    assert content(playback[4])['valid'] and base64.b64decode(content(playback[5])['data']) == expected[2]
    if args.legacy_compositor:
        legacy_check(cli, flags, initialized, output, args.legacy_compositor.resolve())
    print(f'JSONL/MCP: {len(rows)} shared actions; independent partial-alpha references, policy/Undo, workflow, full-output baking, project and DLL-independent replay passed.')
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

    def settle(quiet=.75, timeout=8):
        deadline = time.monotonic()+timeout
        previous, changed = None, time.monotonic()
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
        call('layer.rename', dict(name='Ink 墨'))
        call('brush.set', dict(radius=16, color='#F9D8E6', stabilizer=.35, pressure_size=True, pressure_opacity=True))
        call('canvas.stroke', dict(points=[dict(x=25, y=165, pressure=.2, pen=True, time=0),
                                           dict(x=140, y=45, pressure=1., pen=True, time=60),
                                           dict(x=290, y=150, pressure=.4, pen=True, time=120)]))
        call('layer.set', dict(blend_mode='screen'))
        call('group.create', dict(members=[dict(kind='layer', id=2)], name='Character ink'))
        call('group.set', dict(group_id=1, compositing='pass_through', opacity=179/255))
        call('window.command', dict(panel='layers', operation='show'))
        call('group.inspect', dict(group_id=1))
        for language in ['en', 'zh-CN', 'ja', 'ko']:
            call('settings.update', dict(path=str(settings), ui=dict(language=language)))
            assert json.loads(settings.read_text(encoding='utf-8-sig'))['custom']['keep'] == 42
            settle()
            query, actual = call('group.query', {}), call('graph.read', {})
            png = call('canvas.snapshot', {})['data']
            assert query['groups'][0]['compositing'] == 'pass_through'
            path = output/f'{language}-pass-through.png'
            call('ui.capture', dict(path=str(path)))
            deadline = time.monotonic()+10
            while not path.exists():
                assert time.monotonic() < deadline, 'Capture timeout'
                time.sleep(.05)
            current = call('group.query', {})
            assert {k: v for k, v in current.items() if k != 'revision'} == {
                k: v for k, v in query.items() if k != 'revision'}
            assert call('graph.read', {})['effective_graph'] == actual['effective_graph']
            assert call('canvas.snapshot', {})['data'] == png
        assert call('replay.verify', {})['valid']
        call('ui.dispatch', dict(id='close', value=True))
        assert process.wait(timeout=10) == 0
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=10)
    print('GUI exact-pixel parity and four-language Pass Through inspector captures passed.')
    print(output)


if __name__ == '__main__':
    main()
