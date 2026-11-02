"""Verify language switching, persistence and invariant canvas pixels in an isolated GUI."""
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
    output = ROOT / 'target/test-output/localization-smoke' / str(time.time_ns())
    output.mkdir(parents=True)
    plugins, profile = output / 'plugins', output / 'profile'
    shutil.copytree(ROOT / 'plugins', plugins)
    profile.mkdir()
    settings = profile / 'settings.json'
    flags = ['--plugins', str(plugins), '--config-dir', str(profile)]
    p = subprocess.run([str(cli), 'call', 'settings.read', json.dumps(dict(path=str(settings))), *flags], capture_output=True, text=True, encoding='utf-8', check=True)
    assert json.loads(p.stdout)['result']['ui']['language'] == 'en'
    settings.write_text(json.dumps(dict(version=1, custom=dict(keep=42), keybindings={'file.save':['Ctrl+F12']})),encoding='utf-8')
    startup = subprocess.STARTUPINFO()
    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = 0
    process = subprocess.Popen([str(gui), *flags], startupinfo=startup)
    endpoint = profile/'session.json'

    def call(action, arguments):
        p = subprocess.run([str(cli), 'call', action, json.dumps(arguments), '--attach', str(endpoint)], capture_output=True, text=True, encoding='utf-8', timeout=30)
        assert p.returncode == 0, p.stderr+p.stdout
        return json.loads(p.stdout)['result']

    def screenshot(name):
        path = output/name
        call('ui.dispatch',dict(id='screenshot',value=str(path)))
        deadline = time.monotonic()+10
        while not path.exists():
            assert time.monotonic()<deadline,'Screenshot timeout'
            time.sleep(.05)
        return path

    try:
        deadline = time.monotonic()+20
        while not endpoint.exists():
            assert process.poll() is None and time.monotonic()<deadline,'GUI startup failed'
            time.sleep(.05)
        call('canvas.new',dict(width=256,height=192))
        call('native.checkerboard',dict(cell=16))
        call('brush.set',dict(radius=8,color='#0077ff',stabilizer=.35,pressure_size=True))
        call('canvas.stroke',dict(points=[dict(x=24,y=130,pen=True,pressure=.2,time=0),dict(x=100,y=60,pen=True,pressure=1.,time=30),dict(x=225,y=130,pen=True,pressure=.5,time=60)]))
        expected = base64.b64decode(call('canvas.snapshot',{})['data'])
        graph=dict(version=1,nodes=[dict(id=1,type='native_paint.statistics',position=[0,0],parameters={}),dict(id=2,type='action.native.checkerboard',position=[460,0],parameters={})],edges=[dict(**{'from':1},output='calls',to=2,input='cell')])
        call('workflow.set',dict(graph=graph))
        for language in ['en','zh-CN','ja','ko']:
            call('settings.update',dict(path=str(settings),ui=dict(language=language)))
            stored=json.loads(settings.read_text(encoding='utf-8-sig'))
            assert stored['ui']['language']==language and stored['custom']['keep']==42
            assert stored['keybindings']['file.save']==['Ctrl+F12']
            call('workflow.open',{})
            time.sleep(.25)
            screenshot(f'{language}-workbench.png')
            call('ui.dispatch',dict(id='settings.search',value='ui.language'))
            time.sleep(.25)
            screenshot(f'{language}-settings.png')
            assert base64.b64decode(call('canvas.snapshot',{})['data'])==expected
            assert call('workflow.read',{})['graph']==graph
            # Close only this isolated settings preview through the UI effect API.
            call('ui.dispatch',dict(id='settings.close',value=True))
        call('settings.update',dict(path=str(settings),ui=dict(language='en')))
        call('document.save',dict(path=str(output/'locale-roundtrip.mcp')))
        call('document.open',dict(path=str(output/'locale-roundtrip.mcp')))
        assert call('workflow.read',{})['graph']==graph
        assert base64.b64decode(call('canvas.snapshot',{})['data'])==expected
        call('ui.dispatch',dict(id='close',value=True))
        assert process.wait(timeout=10)==0
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=10)
    print('English default and four-language switching/persistence passed; canvas PNG and workflow graph unchanged.')
    print(output)


if __name__=='__main__':
    main()
