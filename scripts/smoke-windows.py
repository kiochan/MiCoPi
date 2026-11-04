"""Verify plugin windows, command discovery and unchanged pixels in an isolated desktop."""
import argparse
import base64
import ctypes
from ctypes import wintypes
import json
from pathlib import Path
import shutil
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
USER = ctypes.WinDLL('user32', use_last_error=True)
USER.SetProcessDpiAwarenessContext.argtypes = [ctypes.c_void_p]
USER.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))
USER.SetThreadDpiAwarenessContext.argtypes = [ctypes.c_void_p]
USER.SetThreadDpiAwarenessContext.restype = ctypes.c_void_p
assert USER.SetThreadDpiAwarenessContext(ctypes.c_void_p(-4)), ctypes.get_last_error()
USER.IsWindow.argtypes = [wintypes.HWND]
USER.GetWindowRect.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.RECT)]
USER.GetWindowLongPtrW.argtypes = [wintypes.HWND, ctypes.c_int]
USER.GetWindowLongPtrW.restype = ctypes.c_ssize_t
USER.SetWindowPos.argtypes = [wintypes.HWND, wintypes.HWND, ctypes.c_int, ctypes.c_int,
                              ctypes.c_int, ctypes.c_int, wintypes.UINT]
USER.SetForegroundWindow.argtypes = [wintypes.HWND]
USER.SendMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
USER.SendMessageW.restype = wintypes.LPARAM
USER.GetWindow.argtypes = [wintypes.HWND, wintypes.UINT]
USER.GetWindow.restype = wintypes.HWND
USER.ShowWindow.argtypes = [wintypes.HWND, ctypes.c_int]
USER.ClientToScreen.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.POINT)]
USER.GetDpiForWindow.argtypes = [wintypes.HWND]


def above(first, second):
    current = first
    while current:
        current = USER.GetWindow(current, 2)  # GW_HWNDNEXT, from top toward bottom
        if current == second:
            return True
    return False


def native_rect(hwnd):
    rect = wintypes.RECT()
    assert USER.GetWindowRect(hwnd, ctypes.byref(rect))
    return rect.left, rect.top, rect.right, rect.bottom


def monitors():
    result = []
    callback_type = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HANDLE, wintypes.HDC,
                                      ctypes.POINTER(wintypes.RECT), wintypes.LPARAM)
    def visit(_, __, rect, ___):
        r = rect.contents
        result.append((r.left, r.top, r.right, r.bottom))
        return True
    callback = callback_type(visit)
    assert USER.EnumDisplayMonitors(None, None, callback, 0)
    return result


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
        # A docked tab group detaches as a unit; docking a nested floating tree keeps its shape.
        grouped = window('layers', 'float', scope='group', x=80, y=180, width=420, height=500)
        deadline = time.monotonic()+10
        while True:
            grouped = {p['id']: p for p in call('window.list', {})['actual']}
            group_hwnd = (grouped['layers'].get('native') or {}).get('hwnd')
            if group_hwnd:
                assert all(grouped[p]['native']['hwnd'] == group_hwnd for p in ['layers', 'brush', 'color'])
                break
            assert time.monotonic()<deadline, grouped
            time.sleep(.1)
        assert USER.SetWindowPos(group_hwnd, None, 100, 160, 440, 520, 0x4 | 0x10)
        window('color', 'dock_bottom', target='brush', fraction=.48)
        grouped = {p['id']: p for p in call('window.list', {})['actual']}
        assert all(grouped[p]['native']['hwnd'] == group_hwnd for p in ['layers','brush','color'])
        window('layers', 'float', scope='group', width=480, height=750)
        deadline = time.monotonic()+10
        while True:
            grouped = {p['id']: p for p in call('window.list', {})['actual']}
            if grouped['layers']['native']['bounds']['height'] >= 700:
                break
            assert time.monotonic()<deadline, 'Explicit group placement was not applied'
            time.sleep(.05)
        group_capture = output/'native-panel-group.png'
        call('window.capture', dict(panel='layers', path=str(group_capture)))
        deadline = time.monotonic()+10
        while not group_capture.exists():
            assert time.monotonic()<deadline, 'Group capture timed out'
            time.sleep(.05)
        root_group_owner = USER.GetWindow(group_hwnd, 4)
        USER.ShowWindow(root_group_owner, 4)
        USER.SetForegroundWindow(root_group_owner)
        assert USER.SetWindowPos(root_group_owner,None,0,0,0,0,0x1|0x2|0x10|0x40)
        nav = {p['id']: p for p in call('window.list', {})['actual']}['navigator']['bounds']
        root_scale = USER.GetDpiForWindow(root_group_owner)/96
        drop_point = wintypes.POINT(int((nav['x']+4)*root_scale), int((nav['y']+nav['height']*.5)*root_scale))
        assert USER.ClientToScreen(root_group_owner, ctypes.byref(drop_point))
        # The automation transport exercises the same desktop hit testing and command dispatch
        # as a native move-loop release, without moving the user's hardware pointer.
        canvas_bounds={p['id']:p for p in call('window.list',{})['actual']}['canvas']['bounds']
        no_guide=wintypes.POINT(int((canvas_bounds['x']+canvas_bounds['width']*.25)*root_scale),int((canvas_bounds['y']+canvas_bounds['height']*.3)*root_scale))
        assert USER.ClientToScreen(root_group_owner,ctypes.byref(no_guide))
        call('window.drop',dict(panel='layers',target='canvas',x=no_guide.x,y=no_guide.y))
        deadline=time.monotonic()+10
        while (call('window.list',{}).get('last_drop') or {}).get('status')=='queued':
            assert time.monotonic()<deadline,'No-guide drop timed out'
            time.sleep(.05)
        assert call('window.list',{})['last_drop']['status']=='no_target'
        assert {p['id']:p for p in call('window.list',{})['actual']}['layers']['floating']
        response=call('window.drop',dict(panel='layers',target='navigator',x=drop_point.x,y=drop_point.y))
        assert response['queued'] and not response['applied']
        deadline=time.monotonic()+10
        while {p['id']:p for p in call('window.list',{})['actual']}['layers']['floating']:
            result=call('window.list',{})
            assert (result.get('last_drop') or {}).get('status')!='no_target', result.get('last_drop')
            assert time.monotonic()<deadline,result
            time.sleep(.05)
        request=call('window.list',{})['last_request']
        assert request['scope']=='group' and request['operation']=='dock_left' and request['target']=='navigator',request
        assert USER.SetWindowPos(root_group_owner,-2,0,0,0,0,0x1|0x2|0x10)
        grouped = {p['id']: p for p in call('window.list', {})['actual']}
        assert all(not grouped[p]['floating'] for p in ['layers','brush','color'])
        assert grouped['layers']['bounds'] == grouped['brush']['bounds']
        assert grouped['color']['bounds'] != grouped['brush']['bounds']
        window('color', 'float', x=40, y=160, width=300, height=460)
        window('color', 'tab', target='brush')
        window('toolbox', 'float', x=40, y=65, width=820, height=125)
        window('canvas', 'float', x=160, y=240, width=900, height=590)
        for panel in state['panels']:
            if panel['id'] not in ['canvas','toolbox']:
                window(panel['id'], 'hide')
        actual = {p['id']: p for p in call('window.list', {})['actual']}
        assert actual['canvas']['floating'] and actual['toolbox']['floating']
        deadline = time.monotonic()+10
        while not all((actual[p].get('native') or {}).get('hwnd') for p in ['canvas', 'toolbox']):
            assert time.monotonic() < deadline, actual
            time.sleep(.1)
            actual = {p['id']: p for p in call('window.list', {})['actual']}
        canvas_hwnd, tools_hwnd = [actual[p]['native']['hwnd'] for p in ['canvas', 'toolbox']]
        assert canvas_hwnd != tools_hwnd
        for hwnd in [canvas_hwnd, tools_hwnd]:
            assert USER.IsWindow(hwnd)
            assert not USER.GetWindowLongPtrW(hwnd, -16) & 0x40000000, 'Expected top-level OS window'
            assert not USER.GetWindowLongPtrW(hwnd, -20) & 0x8, 'Palette must not be globally topmost'
        root_hwnd = USER.GetWindow(canvas_hwnd, 4)  # GW_OWNER
        assert root_hwnd and USER.GetWindow(tools_hwnd, 4) == canvas_hwnd
        USER.ShowWindow(root_hwnd, 4)
        for activated in [root_hwnd, canvas_hwnd, root_hwnd]:
            USER.SetForegroundWindow(activated)
            time.sleep(.08)
            assert above(tools_hwnd, canvas_hwnd) and above(canvas_hwnd, root_hwnd), 'Owned palette was covered by the application'
        canvas_rect = native_rect(canvas_hwnd)
        displays = monitors()
        monitor = displays[-1]
        # Move to another display when available, otherwise move independently on this display.
        x, y = monitor[0]+40, monitor[1]+80
        assert USER.SetWindowPos(tools_hwnd, None, x, y, 720, 160, 0x4 | 0x10)
        deadline = time.monotonic()+10
        while native_rect(tools_hwnd)[:2] != (x, y):
            assert time.monotonic() < deadline, native_rect(tools_hwnd)
            time.sleep(.1)
        assert native_rect(canvas_hwnd) == canvas_rect, 'Moving Tools moved Canvas'
        assert base64.b64decode(call('canvas.snapshot', {})['data']) == expected
        tools_rect = native_rect(tools_hwnd)
        old_native = actual['canvas']['native']['bounds']
        assert USER.SetWindowPos(canvas_hwnd, None, monitor[0]+60, monitor[1]+280, 900, 590, 0x4 | 0x10)
        deadline=time.monotonic()+10
        while True:
            actual = {p['id']: p for p in call('window.list', {})['actual']}
            if actual['canvas']['native']['bounds'] != old_native:
                break
            assert time.monotonic()<deadline, 'Canvas placement did not update'
            time.sleep(.1)
        assert native_rect(tools_hwnd)==tools_rect, 'Moving Canvas moved Tools'
        # Native resize observations must not be written back as repeated builder commands.
        USER.SendMessageW(canvas_hwnd, 0x231, 0, 0)  # WM_ENTERSIZEMOVE
        for n in range(12):
            assert USER.SetWindowPos(canvas_hwnd, None, monitor[0]+60+n, monitor[1]+280,
                                     900-n*12, 590-n*6, 0x4 | 0x10)
            time.sleep(.025)
        USER.SendMessageW(canvas_hwnd, 0x232, 0, 0)  # WM_EXITSIZEMOVE
        settled = native_rect(canvas_hwnd)
        for _ in range(6):
            time.sleep(.08)
            call('window.list', {})
            assert native_rect(canvas_hwnd) == settled, 'Native resize oscillated after release'
        # Native client coordinates must reach the same painting Session.
        USER.SetForegroundWindow(canvas_hwnd)
        time.sleep(.2)
        actual = {p['id']: p for p in call('window.list', {})['actual']}
        native = actual['canvas']['native']
        rect = native['image_rect']
        ppp = native['pixels_per_point']
        px = int((rect['min']['x']+rect['max']['x'])*.5*ppp)
        py = int((rect['min']['y']+rect['max']['y'])*.5*ppp)
        def mouse(message, dx=0, pressed=0):
            USER.SendMessageW(canvas_hwnd, message, pressed, ((py & 65535) << 16) | ((px+dx) & 65535))
        mouse(0x200)
        mouse(0x201, pressed=1)
        time.sleep(.08)
        mouse(0x200, dx=50, pressed=1)
        time.sleep(.08)
        mouse(0x202, dx=50)
        deadline = time.monotonic()+10
        while base64.b64decode(call('canvas.snapshot', {})['data']) == expected:
            assert time.monotonic() < deadline, 'Native Canvas did not receive a mouse stroke'
            time.sleep(.1)
        call('history.undo', {})
        assert base64.b64decode(call('canvas.snapshot', {})['data']) == expected
        # Renderer captures contain only this viewport, independent of occlusion and monitor coordinates.
        for panel in ['canvas', 'toolbox']:
            path=output/f'native-{panel}.png'
            call('window.capture', dict(panel=panel, path=str(path)))
            deadline=time.monotonic()+10
            while not path.exists():
                assert time.monotonic()<deadline, 'Native viewport capture timed out'
                time.sleep(.05)
        def check_header(panel):
            actual = {p['id']: p for p in call('window.list', {})['actual']}
            entry = actual[panel]
            header = entry['header']
            centers = [(header[key]['min']['y']+header[key]['max']['y'])/2
                       for key in ['grip', 'tabs', 'maximize', 'close']]
            assert max(centers)-min(centers) < .01, 'Group controls must share one row'
            assert header['grip']['max']['x'] < header['tabs']['min']['x']
            assert header['tabs']['max']['x'] < header['maximize']['min']['x']
            assert header['maximize']['max']['x'] <= header['close']['min']['x']
        # Header widget clicks are tested with egui RawInput. Isolated Windows
        # desktops may discard synthetic button events when the real pointer is
        # outside the viewport, so verify native geometry and commands separately.
        check_header('canvas')
        check_header('toolbox')
        normal_rect=native_rect(canvas_hwnd)
        window('canvas','maximize')
        deadline=time.monotonic()+10
        while not {p['id']:p for p in call('window.list', {})['actual']}['canvas']['native']['maximized']:
            assert time.monotonic()<deadline, 'Canvas maximize timed out'
            time.sleep(.05)
        window('canvas','restore')
        deadline=time.monotonic()+10
        while native_rect(canvas_hwnd)!=normal_rect:
            assert time.monotonic()<deadline, (normal_rect,native_rect(canvas_hwnd))
            time.sleep(.05)
        # Theme updates are shared by every native viewport without changing pixels.
        call('settings.update', dict(path=str(profile/'settings.json'),ui=dict(theme='light')))
        time.sleep(.25)
        for panel in ['canvas','toolbox']:
            path=output/f'native-{panel}-light.png'
            call('window.capture',dict(panel=panel,path=str(path)))
            deadline=time.monotonic()+10
            while not path.exists():
                assert time.monotonic()<deadline,'Light viewport capture timed out'
                time.sleep(.05)
        assert base64.b64decode(call('canvas.snapshot', {})['data'])==expected
        call('settings.update',dict(path=str(profile/'settings.json'),ui=dict(theme='graphite')))
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
        # A row closes its leaf, preserving another group in the same native window.
        for panel in ['layers', 'brush', 'color']:
            window(panel, 'show')
        window('brush', 'tab', target='layers')
        window('color', 'tab', target='layers')
        window('layers', 'float', scope='tabs', x=100, y=160, width=360, height=650)
        window('color', 'dock_bottom', target='layers')
        time.sleep(.3)
        check_header('layers')
        window('layers', 'hide', scope='tabs')
        deadline=time.monotonic()+10
        while True:
            actual={p['id']:p for p in call('window.list',{})['actual']}
            if not actual['layers']['open'] and not actual['brush']['open']:
                break
            assert time.monotonic()<deadline, 'Close-group command did not hide the tabs'
            time.sleep(.05)
        assert actual['color']['open'] and actual['color']['floating']
        assert actual['canvas']['open'] and actual['toolbox']['open']
        assert base64.b64decode(call('canvas.snapshot', {})['data']) == expected
        call('ui.dispatch', dict(id='close', value=True))
        assert process.wait(timeout=10) == 0
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=10)
    print('Plugin windows: queued/applied state, dock/float/tab/hide/show, headless fallback and PNG/replay parity passed.')
    print(f'Owned palette stacking, group topology, stable native resizing and mouse drawing passed; {len(displays)} display(s) available.')
    print(output)


if __name__ == '__main__':
    main()
