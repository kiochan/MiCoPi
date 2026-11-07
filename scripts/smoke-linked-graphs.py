"""Exercise linked layer composition through JSONL, MCP and an isolated desktop profile."""
import argparse
import base64
import json
from pathlib import Path
import shutil
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument('--cli', default='dist/micopi-cli.exe')
    parser.add_argument('--gui', default='dist/MiCoPi.exe')
    args = parser.parse_args()
    cli, gui = (ROOT / args.cli).resolve(), (ROOT / args.gui).resolve()
    output = ROOT / 'target/test-output/linked-graph-smoke' / str(time.time_ns())
    output.mkdir(parents=True)
    plugins, profile = output / 'plugins', output / 'profile'
    shutil.copytree(ROOT / 'plugins', plugins)
    profile.mkdir()
    flags = ['--plugins', str(plugins), '--config-dir', str(profile)]
    identity_curve = dict(interpolation="linear", points=[dict(input=0, output=0), dict(input=255, output=255)])
    curves = dict(master=dict(interpolation="smooth", points=[dict(input=0, output=0), dict(input=96, output=120), dict(input=255, output=255)]),
                  red=identity_curve, green=identity_curve, blue=dict(interpolation="linear", points=[dict(input=0, output=12), dict(input=255, output=240)]))
    graph = dict(enabled=True, linked=True, nodes=[
        dict(id=0, kind='layer', index=0, position=[0, 0]),
        dict(id=1, kind='filter', filter='invert', amount=1, position=[260, 0]),
        dict(id=2, kind='layer', index=1, position=[0, 280]),
        dict(id=3, kind='blend', mode='screen', opacity=1, position=[520, 280]),
        dict(id=4, kind='output', position=[780, 280]),
        dict(id=5, kind='levels', node_type='compositing.levels', position=[400, 0],
             parameters=dict(input_black=16, input_white=240, gamma=1.5,
                             output_black=8, output_white=248)),
        dict(id=6, kind="curves", node_type="compositing.curves", curves=curves, position=[500, 0]),
    ], edges=[dict(**{'from': 0}, to=1, input=0), dict(**{'from': 1}, to=5, input=0),
              dict(**{'from': 5}, to=6, input=0), dict(**{'from': 6}, to=3, input=0),
              dict(**{'from': 2}, to=3, input=1), dict(**{'from': 3}, to=4, input=0)])
    commands = [
        dict(action='canvas.new', args=dict(width=256, height=192)),
        dict(action='layer.rename', args=dict(name='Base color')),
        dict(action='layer.gradient', args=dict(x0=0, y0=0, x1=256, y1=192,
             start_color='#dc8f5b', end_color='#264875')),
        dict(action='layer.add', args={}),
        dict(action='layer.rename', args=dict(name='Light')),
        dict(action='layer.gradient', args=dict(x0=60, y0=60, x1=210, y1=160,
             start_color='#c8f5ff', end_color='#00000000', kind='radial')),
        dict(action='graph.set', args=dict(graph=graph)),
        dict(action='layer.duplicate', args=dict(layer_id=1)),
        dict(action='layer.move', args=dict(layer_id=1, to=2)),
        dict(action='layer.set', args=dict(layer_id=1, opacity=.7, blend_mode='multiply')),
        dict(action='layer.delete', args=dict(layer_id=3)),
        dict(action='history.undo', args={}),
        dict(action='history.redo', args={}),
        dict(action='canvas.resize', args=dict(width=280, height=210, anchor='center')),
        dict(action='canvas.crop', args=dict(x=8, y=6, width=260, height=196)),
        dict(action='history.undo', args={}),
        dict(action='history.redo', args={}),
        dict(action='graph.read', args={}),
        dict(action='replay.verify', args={}),
    ]
    script = output / 'linked.jsonl'
    script.write_text('\n'.join(json.dumps(c) for c in commands + [
        dict(action='canvas.export', args=dict(path=str(output/'batch.png')))])+'\n', encoding='utf-8')
    p = subprocess.run([str(cli), 'run', str(script), *flags], capture_output=True,
                       text=True, encoding='utf-8', timeout=40)
    assert p.returncode == 0, p.stderr+p.stdout
    records = [json.loads(line) for line in p.stdout.splitlines()]
    assert all(r['ok'] for r in records), records
    assert records[-3]['data']['result']['mode'] == 'linked'
    assert any(n['kind'] == 'levels' for n in records[-3]['data']['result']['graph']['nodes'])
    assert any(n['kind'] == 'curves' for n in records[-3]['data']['result']['graph']['nodes'])
    assert records[-2]['data']['result']['valid']
    expected = (output/'batch.png').read_bytes()
    requests = [dict(jsonrpc='2.0', id=1, method='initialize', params=dict(protocolVersion='2025-11-25')),
                dict(jsonrpc='2.0', method='notifications/initialized')]
    requests += [dict(jsonrpc='2.0', id=i, method='tools/call',
                     params=dict(name=c['action'], arguments=c['args'])) for i, c in enumerate(commands, 2)]
    requests.append(dict(jsonrpc='2.0', id=100, method='tools/call',
                         params=dict(name='canvas.snapshot', arguments={})))
    p = subprocess.run([str(cli), 'mcp', *flags], input='\n'.join(json.dumps(r) for r in requests)+'\n',
                       capture_output=True, text=True, encoding='utf-8', timeout=40)
    assert p.returncode == 0, p.stderr
    replies = [json.loads(line) for line in p.stdout.splitlines()]
    assert all('error' not in r and not r.get('result', {}).get('isError') for r in replies), replies
    assert base64.b64decode(next(r for r in replies if r['id'] == 100)['result']['content'][0]['data']) == expected
    startup = subprocess.STARTUPINFO()
    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = 0
    process = subprocess.Popen([str(gui), *flags], startupinfo=startup)
    endpoint = profile/'session.json'

    def call(action, arguments):
        p = subprocess.run([str(cli), 'call', action, json.dumps(arguments), '--attach', str(endpoint)],
                           capture_output=True, text=True, encoding='utf-8', timeout=30)
        assert p.returncode == 0, p.stderr+p.stdout
        return json.loads(p.stdout)['result']

    try:
        deadline = time.monotonic()+20
        while not endpoint.exists():
            assert process.poll() is None and time.monotonic() < deadline, 'GUI startup failed'
            time.sleep(.05)
        for command in commands:
            result = call(command['action'], command['args'])
            assert result.get('ok', True), result
        assert base64.b64decode(call('canvas.snapshot', {})['data']) == expected
        applied = call('graph.read', {})
        assert applied['mode'] == 'linked' and len(applied['layers']) == 2
        call('graph.open', {})
        time.sleep(.4)
        # An idle node editor must follow a layer insertion, without requiring Load Current.
        call('layer.add', {})
        assert len(call('graph.read', {})['layers']) == 3
        call('history.undo', {})
        project = output/'linked.mcp'
        call('document.save', dict(path=str(project)))
        call('document.open', dict(path=str(project)))
        assert call('graph.read', {})['graph'] == applied['graph']
        assert base64.b64decode(call('canvas.snapshot', {})['data']) == expected
        assert call('replay.verify', {})['valid']
        capture = output/'gui-linked-stack.png'
        call('ui.dispatch', dict(id='screenshot', value=str(capture)))
        deadline = time.monotonic()+10
        while not capture.exists():
            assert time.monotonic() < deadline, 'Screenshot timed out'
            time.sleep(.05)
        call('ui.dispatch', dict(id='close', value=True))
        assert process.wait(timeout=10) == 0
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=10)
    print('Linked stacks: identical JSONL/MCP/GUI PNGs; filter/identity/undo/project/replay checks passed.')
    print(output)


if __name__ == '__main__':
    main()
