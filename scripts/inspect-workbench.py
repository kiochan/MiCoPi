"""Inspect a running MiCoPi through its authenticated Session pipe, without UI automation.

Optional PNGs contain the app framebuffer, not Windows' composed desktop backdrop.
The endpoint credential is consumed by the CLI and is never copied into the report.
"""
import argparse
import json
from pathlib import Path
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli', type=Path, required=True)
    parser.add_argument('--attach', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--capture', action='store_true')
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)

    def call(action, values=None):
        result = subprocess.run([str(args.cli.resolve()), 'call', action,
                                 json.dumps(values or {}), '--attach', str(args.attach.resolve())],
                                capture_output=True, text=True, encoding='utf-8', timeout=35)
        if result.returncode:
            raise RuntimeError(result.stderr or result.stdout)
        return json.loads(result.stdout)['result']

    report = {'workbench': call('workbench.diagnostics'),
              'document': call('canvas.state'), 'plugins': call('plugins.list'),
              'capture_includes_desktop_backdrop': False}
    if args.capture:
        path = (args.output / ('ui-' + str(time.time_ns()) + '.png')).resolve()
        request = call('ui.capture', {'path': str(path)})
        identity = request['request_id']
        deadline = time.monotonic() + 35
        while True:
            captures = call('window.list')['capture_requests']
            match = next((r for r in captures['recent'] if r['request_id'] == identity), None)
            if match:
                report['capture'] = match
                if match['status'] != 'saved':
                    raise RuntimeError(json.dumps(match))
                break
            if time.monotonic() >= deadline:
                raise TimeoutError('Capture did not finish: ' + str(identity))
            time.sleep(0.1)
    path = args.output / 'report.json'
    path.write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    print(path.resolve())


if __name__ == '__main__':
    main()
