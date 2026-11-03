"""Verify plugin windows, command discovery and unchanged pixels in an isolated desktop."""
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
    output = ROOT/'target/test-output/window-smoke'/str(time.time_ns())
    output.mkdir(parents=True)
    plugins, profile = output/'plugins', output/'profile'
    shutil.copytree(ROOT/'plugins', plugins)
    profile.mkdir()
    flags = ['--plugins', str(plugins), '--config-dir', str(profile)]
    commands = [dict(action='canvas.new', args=dict(width=320, height=240)),
                dict(action='native.checkerboard', args=dict(cell=24)),
                dict(action='brush.set', args=dict(color='#5e78c8', radius=14, stabilizer=.35,
                     pressure_size=True, pressure_opacity=True)),
                dict(action='canvas.stroke', args=dict(points=[
                    dict(x=30, y=170, pen=True, pressure=.2, time=10),
                    dict(x=100, y=70, pen=True, pressure=.9, time=50),
                    dict(x=180, y=160, pen=True, pressure=.6, time=90),
                    dict(x=280, y=65, pen=True, pressure=1., time=130)]))]
    script = output/'windows.jsonl'
    script.write_text('\n'.join(json.dumps(c) for c in commands+[
        dict(action='window.canvas.float', args={}),
        dict(action='canvas.export', args=dict(path=str(output/'batch.png')))])+'\n', encoding='utf-8')
    p = subprocess.run([str(cli), 'run', str(script), *flags], capture_output=True,
                       text=True, encoding='utf-8', timeout=40)
    assert p.returncode == 0, p.stderr+p.stdout
    records = [json.loads(line) for line in p.stdout.splitlines()]
    assert all(r['ok'] for r in records), records
    assert records[-2]['data']['result']['applied'] is False
    assert records[-2]['data']['result']['queued'] is False
    expected = (output/'batch.png').read_bytes()
    startup = subprocess.STARTUPINFO()
    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = 0
    process = subprocess.Popen([str(gui), *flags], startupinfo=startup)
    endpoint = profile/'session.json'

    def call(action, arguments):
        p = subprocess.run([str(cli), 'call', action, json.dumps(arguments), '--attach', str(endpoint)],
                           capture_output=True, text=True, encoding='utf-8', timeout=35)
        assert p.returncode == 0, p.stderr+p.stdout
        return json.loads(p.stdout)['result']

    def window(panel, operation, **parameters):
        response = call('window.command', dict(panel=panel, operation=operation, **parameters))
        assert response['queued'] and not response['applied']
        deadline = time.monotonic()+10
        while True:
            state = call('window.list', {})
            if state['last_result']['status'] != 'queued':
                assert state['last_result']['status'] == 'applied', state
                return state
            assert time.monotonic() < deadline, 'Window command timed out'
            time.sleep(.05)

    def capture(name):
        path = output/name
        call('ui.dispatch', dict(id='screenshot', value=str(path)))
        deadline = time.monotonic()+10
        while not path.exists():
            assert time.monotonic() < deadline, 'Screenshot timed out'
            time.sleep(.05)

    try:
        deadline = time.monotonic()+25
        while not endpoint.exists():
            assert process.poll() is None and time.monotonic() < deadline, 'GUI startup failed'
            time.sleep(.05)
        for c in commands:
            call(c['action'], c['args'])
        assert not call('plugins.list', {})['diagnostics']
        assert base64.b64decode(call('canvas.snapshot', {})['data']) == expected
        state = call('window.list', {})
        assert {'canvas', 'toolbox', 'color', 'brush', 'navigator', 'layers', 'workflows.editor'}.issubset({p['id'] for p in state['panels']})
        capture('default-workspace.png')
        window('color', 'float', x=40, y=160, width=300, height=460)
        window('color', 'tab', target='brush')
        window('toolbox', 'float', x=40, y=65, width=820, height=125)
        window('canvas', 'float', x=160, y=240, width=900, height=590)
        for panel in state['panels']:
            if panel['id'] not in ['canvas','toolbox']:
                window(panel['id'], 'hide')
        actual = {p['id']: p for p in call('window.list', {})['actual']}
        assert actual['canvas']['floating'] and actual['toolbox']['floating']
        capture('floating-windows.png')
        assert base64.b64decode(call('canvas.snapshot', {})['data']) == expected
        window('canvas', 'hide')
        assert base64.b64decode(call('canvas.snapshot', {})['data']) == expected
        window('canvas', 'show')
        window('toolbox', 'dock_top', target='canvas')
        call('workflow.open', {})
        assert any(p['id'] == 'workflows.editor' and p['open'] for p in call('window.list', {})['actual'])
        assert base64.b64decode(call('canvas.snapshot', {})['data']) == expected
        assert call('replay.verify', {})['valid']
        call('document.save', dict(path=str(output/'windows.mcp')))
        call('document.open', dict(path=str(output/'windows.mcp')))
        assert base64.b64decode(call('canvas.snapshot', {})['data']) == expected
        assert call('replay.verify', {})['valid']
        window('canvas', 'float', x=160, y=240, width=900, height=590)
        window('toolbox', 'float', x=40, y=65, width=820, height=125)
        call('ui.dispatch', dict(id='close', value=True))
        assert process.wait(timeout=10) == 0
        workspace = json.loads((profile/'workspace.egui.json').read_text(encoding='utf-8-sig'))
        assert 'toolbox' in workspace['known_panels']
        process = subprocess.Popen([str(gui), *flags], startupinfo=startup)
        deadline = time.monotonic()+25
        while True:
            try:
                actual = {p['id']: p for p in call('window.list', {})['actual']}
                assert actual['canvas']['floating'] and actual['toolbox']['floating']
                break
            except (AssertionError, KeyError, TypeError):
                assert process.poll() is None and time.monotonic() < deadline, 'Workspace restore failed'
                time.sleep(.1)
        call('document.open', dict(path=str(output/'windows.mcp')))
        assert base64.b64decode(call('canvas.snapshot', {})['data']) == expected
        assert call('replay.verify', {})['valid']
        call('ui.dispatch', dict(id='close', value=True))
        assert process.wait(timeout=10) == 0
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=10)
    print('Plugin windows: queued/applied state, dock/float/tab/hide/show, headless fallback and PNG/replay parity passed.')
    print(output)


if __name__ == '__main__':
    main()
