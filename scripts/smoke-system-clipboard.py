"""Run opt-in Win32 clipboard tests in a fresh noninteractive window station.

Never opens, snapshots or changes WinSta0's user clipboard. The Rust fixture also
checks the actual station name before creating a window or accessing clipboard data.
"""
import argparse
import ctypes
from ctypes import wintypes as w
from pathlib import Path
import subprocess
import time


def main():
    parser=argparse.ArgumentParser(__doc__)
    parser.add_argument('--test-binary',type=Path,required=True)
    args=parser.parse_args()
    user=ctypes.WinDLL('user32',use_last_error=True)
    def bind(name,result,*params):
        fn=getattr(user,name);fn.restype=result;fn.argtypes=list(params);return fn
    create=bind('CreateWindowStationW',w.HANDLE,w.LPCWSTR,w.DWORD,w.DWORD,w.LPVOID)
    current=bind('GetProcessWindowStation',w.HANDLE)
    set_station=bind('SetProcessWindowStation',w.BOOL,w.HANDLE)
    create_desktop=bind('CreateDesktopW',w.HANDLE,w.LPCWSTR,w.LPCWSTR,w.LPVOID,w.DWORD,w.DWORD,w.LPVOID)
    close_desktop=bind('CloseDesktop',w.BOOL,w.HANDLE)
    close_station=bind('CloseWindowStation',w.BOOL,w.HANDLE)
    original=current();station=None;desktop=None
    name='MiCoPi-clipboard-test-'+str(time.time_ns())
    def checked(value):
        if not value:raise ctypes.WinError(ctypes.get_last_error())
        return value
    try:
        station=checked(create(name,0,0x003f,None))
        checked(set_station(station))
        desktop=checked(create_desktop('default',None,None,0,0x00c3,None))
        checked(set_station(original))
        startup=subprocess.STARTUPINFO();startup.lpDesktop=name+'\\default'
        run=subprocess.run([str(args.test_binary.resolve()),'isolated_windows_clipboard_roundtrip','--ignored','--nocapture','--test-threads=1'],startupinfo=startup,capture_output=True,text=True,encoding='utf-8',timeout=60)
        print(run.stdout,end='');print(run.stderr,end='')
        if run.returncode:raise SystemExit(run.returncode)
        assert '1 passed' in run.stdout,'Expected exactly one isolated clipboard test'
        print('System clipboard test passed without accessing the interactive clipboard.')
    finally:
        set_station(original)
        if desktop:close_desktop(desktop)
        if station:close_station(station)


if __name__=='__main__':
    try:
        main()
    except OSError as error:
        import sys
        print(f'Isolated clipboard roundtrip NOT VERIFIED: {error}',file=sys.stderr)
        print('The fixture did not access the interactive clipboard.',file=sys.stderr)
        raise SystemExit(2)
