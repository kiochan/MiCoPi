"""Verify correlated root/native captures, resize and explicit unavailable outcomes.

Capture batches enter a non-atomic presentation pipeline so all requests are
queued before the desktop consumes their effects. The fixture artwork is read
only after setup; this script never moves the user's hardware pointer.
"""
import argparse
import base64
import json
from pathlib import Path
import shutil
import subprocess
import time

from capture_checks import CaptureChecks

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli', default='dist/micopi-cli.exe')
    parser.add_argument('--gui', default='dist/MiCoPi.exe')
    args = parser.parse_args()
    cli, gui = [(ROOT/value).resolve() for value in [args.cli, args.gui]]
    assert cli.is_file() and gui.is_file(), 'Build the matching Release host first'
    output = ROOT/'target/test-output/capture-smoke'/str(time.time_ns())
    output.mkdir(parents=True)
    print(output, flush=True)
    plugins, profile = output/'plugins', output/'profile'
    shutil.copytree(ROOT/'plugins', plugins)
    profile.mkdir()
    flags = ['--plugins', str(plugins), '--config-dir', str(profile)]
    startup = subprocess.STARTUPINFO()
    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = 0
    process = subprocess.Popen([str(gui), *flags], startupinfo=startup)
    endpoint = profile/'session.json'

    def call(action_name, arguments, /):
        run = subprocess.run([str(cli), 'call', action_name, json.dumps(arguments), '--attach', str(endpoint)],
                             capture_output=True, text=True, encoding='utf-8', timeout=30)
        assert run.returncode == 0, run.stderr+run.stdout
        return json.loads(run.stdout)['result']

    checks = CaptureChecks(call, process)
    captures = []

    def batch(specifications):
        checks.settle_layout()
        baseline_state = call('canvas.state', {})
        revision = baseline_state['revision']
        event_cursor = call('events.list', {})['latest']
        steps = []
        for name, panel, path, _ in specifications:
            parameters = dict(path=str(path))
            if panel:
                parameters['panel'] = panel
            steps.append(dict(id=name, action='window.capture' if panel else 'ui.capture', args=parameters))
        response = call('pipeline.run', dict(atomic=False, steps=steps))
        assert response['ok'] and not response['transactional'], response
        assert response['completed'] == [name for name, _, _, _ in specifications]
        admitted = call('canvas.state', {})
        assert admitted['revision'] == revision, ('Capture admission changed document revision',
                                                  revision, admitted['revision'], output)
        requests = [checks.queued(response['results'][name]['result'], path, panel, size)
                    for name, panel, path, size in specifications]
        outcomes = checks.wait(requests)
        captures.extend(outcomes)
        completed = call('canvas.state', {})
        diagnostic = output/f'{specifications[0][0]}-state.json'
        diagnostic.write_text(json.dumps(dict(before=baseline_state, admitted=admitted, completed=completed,
                                               events=call('events.list', dict(after=event_cursor)),
                                               captures=outcomes), indent=2)+'\n', encoding='utf-8')
        changed = [key for key in baseline_state if baseline_state[key] != completed.get(key)]
        assert completed['revision'] == revision, ('Capture completion or query changed document revision',
                                                   revision, completed['revision'], changed, diagnostic)

    def failed_capture(path, panel=None):
        checks.settle_layout()
        revision = call('canvas.state', {})['revision']
        captures.append(checks.capture(path, panel, expect_failure=True))
        assert call('canvas.state', {})['revision'] == revision, 'Failed capture changed document revision'

    def document_contract():
        state = call('canvas.state', {})
        frame = call('replay.seek', {})
        return dict(image=base64.b64decode(call('canvas.snapshot', {})['data']),
                    composition=frame['metadata']['composition'], width=state['width'], height=state['height'],
                    layers=state['layers'], selection=state['selection'], document_info=state['document_info'])

    try:
        deadline = time.monotonic()+25
        while not endpoint.exists():
            assert process.poll() is None and time.monotonic() < deadline, 'GUI startup failed'
            time.sleep(.05)
        call('canvas.new', dict(width=96, height=64))
        call('native.checkerboard', dict(cell=12))
        call('layer.rename', dict(name='Capture fixture'))
        before = document_contract()
        checks.window('tool-options', 'float', x=60, y=80, width=280, height=300)
        options, options_size = checks.ready('tool-options', (280, 300))
        checks.window('navigator', 'float', x=400, y=100, width=360, height=420)
        navigator, navigator_size = checks.ready('navigator', (360, 420))
        root = checks.root_hwnd(options)
        assert root == checks.root_hwnd(navigator)
        root_size = checks.client_size(root)
        assert len({root_size, options_size, navigator_size}) == 3
        batch([
            ('root-a', None, output/'root-a.png', root_size),
            ('options-a', 'tool-options', output/'options-a.png', options_size),
            ('root-b', None, output/'root-b.png', root_size),
            ('options-b', 'tool-options', output/'options-b.png', options_size),
            ('navigator-a', 'navigator', output/'navigator-a.png', navigator_size),
        ])
        assert document_contract() == before, 'Concurrent capture modified artwork or composition'
        # Explicit resize must reach native client pixels before a fresh capture
        # deadline begins. Different shapes expose cross-viewport framebuffer use.
        checks.window('tool-options', 'float', width=1000, height=150)
        options, wide_size = checks.ready('tool-options', (1000, 150))
        checks.window('navigator', 'float', width=420, height=260)
        navigator, short_size = checks.ready('navigator', (420, 260))
        assert wide_size != options_size and short_size != navigator_size
        root_size = checks.client_size(root)
        batch([
            ('root-resized', None, output/'root-after-resize.png', root_size),
            ('options-wide-a', 'tool-options', output/'options-wide-a.png', wide_size),
            ('options-wide-b', 'tool-options', output/'options-wide-b.png', wide_size),
            ('navigator-short', 'navigator', output/'navigator-short.png', short_size),
        ])
        assert document_contract() == before, 'Viewport resizing modified artwork or composition'
        # A request loses its target before the next paint pass. It must terminate
        # explicitly rather than disappearing from a single pending-path slot.
        gone = output/'closed-before-paint.png'
        response = call('pipeline.run', dict(atomic=False, steps=[
            dict(id='capture', action='window.capture', args=dict(panel='tool-options', path=str(gone))),
            dict(id='hide', action='window.command', args=dict(panel='tool-options', operation='hide')),
        ]))
        assert response['ok'], response
        captures.extend(checks.wait([checks.queued(response['results']['capture']['result'], gone,
                                                  'tool-options', expect_failure=True)]))
        failed_capture(output/'hidden-options.png', 'tool-options')
        checks.window('navigator', 'dock_right', target='canvas')
        failed_capture(output/'docked-navigator.png', 'navigator')
        failed_capture(output/'missing-directory'/'workspace.png')
        assert document_contract() == before, 'Unavailable capture changed source artwork'
        status = call('window.list', {})['capture_requests']
        assert not status['pending'], status
        assert len({capture['request_id'] for capture in captures}) == len(captures)
        assert call('replay.verify', {})['valid']
        (output/'captures.json').write_text(json.dumps(captures, indent=2)+'\n', encoding='utf-8')
        call('ui.dispatch', dict(id='close', value=True))
        assert process.wait(timeout=15) == 0
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=15)
    print('Concurrent root/native paths, exact nonblank client PNGs, resize, explicit failure and unchanged artwork passed.')
    print(output)


if __name__ == '__main__':
    main()
