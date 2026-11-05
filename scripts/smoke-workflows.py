"""Plugin node graph parity across JSONL, MCP and an isolated GUI session."""
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
    output = ROOT / 'target/test-output/workflow-smoke' / str(time.time_ns())
    output.mkdir(parents=True)
    plugins, profile = output / 'plugins', output / 'profile'
    shutil.copytree(ROOT / 'plugins', plugins)
    profile.mkdir()
    flags = ['--plugins', str(plugins), '--config-dir', str(profile)]
    graph = dict(version=1, nodes=[
        dict(id=1, type='state.document.active', position=[0, 0]),
        dict(id=2, type='action.canvas.new', position=[420, 0], parameters={}),
        dict(id=3, type='action.native.checkerboard', position=[840, 0], parameters=dict(cell=16)),
        dict(id=4, type='action.brush.set', position=[1260, 0], parameters=dict(color='#0077ff', radius=9, stabilizer=.35, pressure_size=True)),
        dict(id=5, type='action.canvas.stroke', position=[1680, 0], parameters=dict(points=[
            dict(x=24, y=130, pen=True, pressure=.2, time=10),
            dict(x=90, y=80, pen=True, pressure=1., time=40),
            dict(x=220, y=130, pen=True, pressure=.4, time=80)])),
        dict(id=6, type='action.canvas.transform', position=[2100,0], parameters=dict(target='layer',skew_x=18,skew_y=-9,scale_x=.85,scale_y=.85)),
    ], edges=[dict(**{'from':1}, output='width', to=2, input='width'),
              dict(**{'from':1}, output='height', to=2, input='height')])
    graph['edges'] += [dict(**{'from':i}, output='$exec', to=i+1, input='$exec') for i in range(2, 6)]
    for node in graph['nodes']:
        node.setdefault('parameters', {})
    commands = [dict(action='canvas.new', args=dict(width=256, height=192)),
                dict(action='workflow.set', args=dict(graph=graph)),
                dict(action='workflow.validate', args={}),
                dict(action='workflow.run', args=dict(dry_run=True)),
                dict(action='workflow.run', args={})]
    script = output / 'workflow.jsonl'
    script.write_text('\n'.join(json.dumps(c) for c in commands + [dict(action='canvas.export', args=dict(path=str(output/'batch.png')))])+'\n', encoding='utf-8')
    p = subprocess.run([str(cli), 'run', str(script), *flags], capture_output=True, text=True, encoding='utf-8', timeout=30)
    assert p.returncode == 0, p.stderr+p.stdout
    records = [json.loads(line) for line in p.stdout.splitlines()]
    assert all(r['ok'] for r in records), records
    assert records[4]['data']['result']['completed_nodes'] == [1, 2, 3, 4, 5, 6]
    expected = (output/'batch.png').read_bytes()
    requests = [dict(jsonrpc='2.0', id=1, method='initialize', params=dict(protocolVersion='2025-11-25')),
                dict(jsonrpc='2.0', method='notifications/initialized')]
    requests += [dict(jsonrpc='2.0', id=i, method='tools/call', params=dict(name=c['action'], arguments=c['args'])) for i,c in enumerate(commands, 2)]
    requests.append(dict(jsonrpc='2.0', id=100, method='tools/call', params=dict(name='canvas.snapshot', arguments={})))
    p = subprocess.run([str(cli), 'mcp', *flags], input='\n'.join(json.dumps(r) for r in requests)+'\n', capture_output=True, text=True, encoding='utf-8', timeout=30)
    assert p.returncode == 0, p.stderr
    replies = [json.loads(line) for line in p.stdout.splitlines()]
    assert all('error' not in r and not r.get('result',{}).get('isError') for r in replies), replies
    shot = next(r for r in replies if r['id']==100)['result']['content'][0]
    assert base64.b64decode(shot['data']) == expected
    (output/'mcp.png').write_bytes(expected)
    startup = subprocess.STARTUPINFO()
    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = 0
    process = subprocess.Popen([str(gui), *flags], startupinfo=startup)
    endpoint = profile/'session.json'

    def call(action, arguments):
        p = subprocess.run([str(cli), 'call', action, json.dumps(arguments), '--attach', str(endpoint)], capture_output=True, text=True, encoding='utf-8', timeout=30)
        assert p.returncode == 0, p.stderr+p.stdout
        return json.loads(p.stdout)['result']

    try:
        deadline = time.monotonic()+20
        while not endpoint.exists():
            assert process.poll() is None and time.monotonic()<deadline, 'GUI startup failed'
            time.sleep(.05)
        catalog = call('node.catalog', {})['nodes']
        assert any(n['id']=='native_paint.statistics' and n['owner']=='native_paint' for n in catalog)
        for command in commands:
            result = call(command['action'], command['args'])
            assert result.get('ok', True), result
        assert base64.b64decode(call('canvas.snapshot', {})['data']) == expected
        assert call('workflow.read', {})['graph'] == graph
        call('document.save', dict(path=str(output/'workflow.mcp')))
        call('document.open', dict(path=str(output/'workflow.mcp')))
        assert call('workflow.read', {})['graph'] == graph
        preview = dict(version=1, nodes=[
            dict(id=1,type='native_paint.statistics',position=[0,0],parameters={}),
            dict(id=2,type='action.native.checkerboard',position=[420,0],parameters={})],
            edges=[dict(**{'from':1}, output='calls',to=2,input='cell')])
        call('workflow.set', dict(graph=preview))
        call('workflow.patch', dict(operations=[dict(op='move_node',id=2,position=[460,0])]))
        assert call('workflow.read', {})['graph']['nodes'][1]['position'] == [460,0]
        call('workflow.open', {})
        time.sleep(.5)
        capture = output/'gui-workflow.png'
        call('ui.dispatch', dict(id='screenshot',value=str(capture)))
        deadline = time.monotonic()+10
        while not capture.exists():
            assert time.monotonic()<deadline, 'Screenshot timed out'
            time.sleep(.05)
        assert base64.b64decode(call('canvas.snapshot', {})['data']) == expected, 'Editing nodes must not execute them'
        call('ui.dispatch', dict(id='close',value=True))
        assert process.wait(timeout=10)==0
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=10)
    print('JSONL, MCP, GUI: plugin nodes produce identical PNG bytes; dry run, patching and project roundtrip passed.')
    print(output)


if __name__ == '__main__':
    main()
