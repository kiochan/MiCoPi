"""Request-correlated GUI capture checks for isolated Windows smoke processes."""
import ctypes
from ctypes import wintypes
import hashlib
import json
from pathlib import Path
import struct
import time

from png_checks import chunks, pixels

USER = ctypes.WinDLL('user32', use_last_error=True)
USER.SetProcessDpiAwarenessContext.argtypes = [ctypes.c_void_p]
USER.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))
USER.SetThreadDpiAwarenessContext.argtypes = [ctypes.c_void_p]
USER.SetThreadDpiAwarenessContext.restype = ctypes.c_void_p
assert USER.SetThreadDpiAwarenessContext(ctypes.c_void_p(-4)), ctypes.get_last_error()
USER.IsWindow.argtypes = [wintypes.HWND]
USER.IsWindowVisible.argtypes = [wintypes.HWND]
USER.GetClientRect.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.RECT)]
USER.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
USER.GetWindow.argtypes = [wintypes.HWND, ctypes.c_uint]
USER.GetWindow.restype = wintypes.HWND
USER.GetWindowTextLengthW.argtypes = [wintypes.HWND]
USER.GetWindowTextW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
USER.GetClassNameW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
ENUM_WINDOWS_CALLBACK = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
USER.EnumWindows.argtypes = [ENUM_WINDOWS_CALLBACK, wintypes.LPARAM]
USER.EnumWindows.restype = wintypes.BOOL


def diagnostic(stage, **values):
    return stage+': '+json.dumps(values, ensure_ascii=False, sort_keys=True, default=str)


def png_capture(path, expected_size, record, decode_pixels=True):
    """Validate PNG framing, CRCs, pixels and dimensions without the host codec."""
    assert path.is_file(), diagnostic('Saved capture has no file', path=path, record=record)
    encoded = path.read_bytes()
    header = struct.unpack('>IIBBBBB', chunks(encoded)[0][1])
    width, height = header[:2]
    assert header[2:] == (8, 6, 0, 0, 0), header
    assert (width, height) == tuple(expected_size), diagnostic(
        'Capture dimensions differ from native client pixels', path=path,
        expected=expected_size, actual=[width, height], record=record)
    assert (record.get('width'), record.get('height')) == (width, height), diagnostic(
        'Completion dimensions differ from the PNG', path=path, record=record, actual=[width, height])
    result = dict(path=str(path), request_id=record['request_id'], width=width, height=height,
                  target=record['target'], status='saved', expected_client_size=list(expected_size),
                  png_sha256=hashlib.sha256(encoded).hexdigest(), bytes=len(encoded))
    if 'panel' in record:
        result['panel'] = record['panel']
    if decode_pixels:
        _, _, data = pixels(encoded)
        colors = set()
        visible = False
        for index in range(0, len(data), 4):
            color = data[index:index+4]
            visible |= color[3] != 0
            colors.add(color)
            if visible and len(colors) >= 16:
                break
        assert visible and len(colors) >= 16, diagnostic(
            'Capture is blank or uniform', path=path, visible=visible, colors=len(colors), record=record)
        result['rgba_sha256'] = hashlib.sha256(data).hexdigest()
    return result


class CaptureChecks:
    def __init__(self, call, process):
        self.call, self.process = call, process
        self.root_candidates = []

    def alive(self):
        assert self.process.poll() is None, diagnostic('Isolated GUI exited', exit_code=self.process.poll())

    def client_size(self, hwnd):
        self.alive()
        assert USER.IsWindow(hwnd), diagnostic('Native viewport is unavailable', hwnd=hwnd)
        owner = wintypes.DWORD()
        assert USER.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
        assert owner.value == self.process.pid, diagnostic(
            'Viewport belongs to a different process', hwnd=hwnd, actual_pid=owner.value,
            expected_pid=self.process.pid)
        rectangle = wintypes.RECT()
        assert USER.GetClientRect(hwnd, ctypes.byref(rectangle)), ctypes.get_last_error()
        result = rectangle.right-rectangle.left, rectangle.bottom-rectangle.top
        assert min(result) > 0, diagnostic('Viewport client has no pixels', hwnd=hwnd, size=result)
        return result

    def root_hwnd(self, floating=None):
        if floating:
            root = USER.GetWindow(floating['native']['hwnd'], 4)  # GW_OWNER; Canvas stays docked here.
            assert root, diagnostic('Floating viewport has no workbench owner', window=floating)
            self.client_size(root)
            return root
        deadline = time.monotonic()+10
        while True:
            self.alive()
            roots = []

            def visit(hwnd, _):
                owner = wintypes.DWORD()
                USER.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
                if owner.value == self.process.pid and USER.IsWindowVisible(hwnd):
                    title = ctypes.create_unicode_buffer(USER.GetWindowTextLengthW(hwnd)+1)
                    USER.GetWindowTextW(hwnd, title, len(title))
                    name = ctypes.create_unicode_buffer(256)
                    USER.GetClassNameW(hwnd, name, len(name))
                    rectangle = wintypes.RECT()
                    size = ([rectangle.right-rectangle.left, rectangle.bottom-rectangle.top]
                            if USER.GetClientRect(hwnd, ctypes.byref(rectangle)) else None)
                    roots.append(dict(hwnd=hwnd, title=title.value, window_class=name.value,
                                      client_size=size, owner=USER.GetWindow(hwnd, 4)))
                return True

            assert USER.EnumWindows(ENUM_WINDOWS_CALLBACK(visit), 0), ctypes.get_last_error()
            self.root_candidates = roots
            # Studio::update sets the root title. Plugin viewports use
            # "MiCoPi — <panel>" or their unique bootstrap title instead.
            workbench = [entry['hwnd'] for entry in roots if not entry['owner'] and
                         (entry['title'] == 'MiCoPi' or entry['title'].endswith(' — MiCoPi'))]
            if len(workbench) == 1:
                self.client_size(workbench[0])
                return workbench[0]
            assert time.monotonic() < deadline, diagnostic(
                'Expected one isolated workbench window', roots=roots, pid=self.process.pid)
            time.sleep(.05)

    def window(self, panel, operation, **parameters):
        response = self.call('window.command', dict(panel=panel, operation=operation, **parameters))
        assert response['queued'] and not response['applied'], response
        deadline, state = time.monotonic()+10, None
        while True:
            self.alive()
            state = self.call('window.list', {})
            result = state.get('last_result') or {}
            if result.get('status') != 'queued':
                assert result.get('status') == 'applied', diagnostic(
                    'Window command failed', panel=panel, operation=operation, response=response, state=state)
                return state
            assert time.monotonic() < deadline, diagnostic(
                'Window placement timeout', panel=panel, operation=operation, response=response, state=state)
            time.sleep(.05)

    def ready(self, panel, logical_size=None):
        """Wait for a real viewport and requested client pixels, not just docking state."""
        deadline, previous, stable, state = time.monotonic()+10, None, 0, None
        while True:
            self.alive()
            state = self.call('window.list', {})
            entry = next((entry for entry in state.get('actual') or [] if entry['id'] == panel), None)
            native = entry.get('native') if entry else None
            if entry and entry.get('open') and entry.get('floating') and native and native.get('hwnd') and USER.IsWindow(native['hwnd']) and not native.get('minimized'):
                actual = self.client_size(native['hwnd'])
                desired = tuple(int(value*native['pixels_per_point']+.5) for value in logical_size) if logical_size else actual
                key = native['hwnd'], actual
                stable = stable+1 if key == previous else 0
                previous = key
                if actual == desired and stable >= 1:
                    return entry, actual
            assert time.monotonic() < deadline, diagnostic(
                'Native viewport readiness timeout', panel=panel, logical_size=logical_size,
                last_client=previous, state=state)
            time.sleep(.05)

    def settle_layout(self):
        """Observe the existing 750-ms layout publication before a pure-capture baseline."""
        deadline, previous, quiet_since, state = time.monotonic()+10, None, time.monotonic(), None
        while True:
            self.alive()
            state = self.call('window.list', {})
            revision = self.call('canvas.state', {})['revision']
            geometry = []
            for entry in state.get('actual') or []:
                native = entry.get('native') or {}
                geometry.append(dict(id=entry['id'], open=entry['open'], floating=entry['floating'],
                                     bounds=entry.get('bounds'), hwnd=native.get('hwnd'),
                                     native_bounds=native.get('bounds'),
                                     pixels_per_point=native.get('pixels_per_point'),
                                     minimized=native.get('minimized'), maximized=native.get('maximized')))
            current = revision, json.dumps(geometry, sort_keys=True)
            if current != previous:
                previous, quiet_since = current, time.monotonic()
            if time.monotonic()-quiet_since >= 1:
                return state
            assert time.monotonic() < deadline, diagnostic(
                'Layout did not settle before capture baseline', revision=revision, state=state)
            time.sleep(.05)

    def queued(self, response, path, panel=None, size=None, expect_failure=False):
        assert response.get('queued') is True and type(response.get('request_id')) is int, diagnostic(
            'Capture was not assigned a request ID', response=response, path=path, panel=panel)
        return dict(response=response, request_id=response['request_id'], path=Path(path), panel=panel,
                    size=size, expect_failure=expect_failure)

    def wait(self, requests, decode_pixels=True):
        """Each call has a fresh deadline; terminal failures never become file timeouts."""
        assert len({request['request_id'] for request in requests}) == len(requests)
        deadline, complete, state = time.monotonic()+10, {}, None
        while len(complete) != len(requests):
            self.alive()
            state = self.call('window.list', {})
            captures = state.get('capture_requests')
            assert isinstance(captures, dict), diagnostic('Capture status contract missing', state=state)
            recent = {record['request_id']: record for record in captures['recent']}
            for request in requests:
                identity = request['request_id']
                if identity in complete or identity not in recent:
                    continue
                record = recent[identity]
                assert record['status'] in ['saved', 'failed'], record
                assert Path(record['path']).resolve() == request['path'].resolve(), diagnostic(
                    'Capture completion used another path', request=request, record=record)
                assert record['target'] == ('panel' if request['panel'] else 'workspace'), record
                if request['panel']:
                    assert record['panel'] == request['panel'], record
                expected = 'failed' if request['expect_failure'] else 'saved'
                assert record['status'] == expected, diagnostic(
                    'Unexpected capture outcome', request=request, record=record, capture_requests=captures)
                complete[identity] = record
            assert len(complete) == len(requests) or time.monotonic() < deadline, diagnostic(
                'Capture completion timeout', requests=requests, completed=complete, state=state)
            if len(complete) != len(requests):
                time.sleep(.05)
        outcomes = []
        for request in requests:
            record = complete[request['request_id']]
            if request['expect_failure']:
                assert record.get('error') and not request['path'].exists(), diagnostic(
                    'Failed capture lacks an error or created a destination', request=request, record=record)
                outcomes.append(record)
            else:
                outcomes.append(png_capture(request['path'], request['size'], record, decode_pixels))
        return outcomes

    def capture(self, path, panel=None, size=None, expect_failure=False, decode_pixels=True):
        action = 'window.capture' if panel else 'ui.capture'
        args = dict(path=str(path))
        if panel:
            args['panel'] = panel
        response = self.call(action, args)
        return self.wait([self.queued(response, path, panel, size, expect_failure)], decode_pixels)[0]
