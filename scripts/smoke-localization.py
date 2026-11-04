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
    locale_dir = profile / 'locales'
    locale_dir.mkdir()
    for language in ['zh-CN', 'ja', 'ko']:
        shutil.copy2(ROOT/'locales'/f'{language}.json', locale_dir/f'{language}.json')
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
        call('brush.set',dict(radius=8,color='#0077ff',stabilizer=.35,pressure_size=True,pressure_opacity=True,spacing=.2,minimum_opacity=.25))
        call('canvas.stroke',dict(points=[dict(x=24,y=130,pen=True,pressure=.2,time=0),dict(x=100,y=60,pen=True,pressure=1.,time=30),dict(x=225,y=130,pen=True,pressure=.5,time=60)]))
        call('layer.rename',dict(name='Ink 墨'))
        call('layer.fill',dict(x=0,y=0,color='#22aa88',tolerance=8,expand=1,feather=2))
        call('layer.gradient',dict(x0=0,y0=0,x1=128,y1=96,start_color='#dd7700',end_color='#2200ff00',opacity=.3,spread='repeat',reverse=True))
        call('layer.duplicate',{})
        call('layer.translate',dict(dx=9,dy=-5))
        call('layer.set',dict(opacity=.45))
        call('layer.set',dict(alpha_locked=True))
        call('layer.fill_color',dict(color='#a64eff',opacity=.4))
        call('layer.move',dict(layer_id=1,to=1))
        # Reopen the existing Layers tab so captures exercise its editing controls,
        # rather than the sample plugin tab that is selected on first discovery.
        call('ui.dispatch',dict(id='command',value='ToggleLayers'))
        call('ui.dispatch',dict(id='command',value='ToggleLayers'))
        expected = base64.b64decode(call('canvas.snapshot',{})['data'])
        graph=dict(version=1,nodes=[dict(id=1,type='native_paint.statistics',position=[0,0],parameters={}),dict(id=2,type='action.native.checkerboard',position=[460,0],parameters={})],edges=[dict(**{'from':1},output='calls',to=2,input='cell')])
        call('workflow.set',dict(graph=graph))
        call('ui.dispatch',dict(id='painting.mode',value='gradient'))
        for language in ['en','zh-CN','ja','ko']:
            call('settings.update',dict(path=str(settings),ui=dict(language=language)))
            stored=json.loads(settings.read_text(encoding='utf-8-sig'))
            assert stored['ui']['language']==language and stored['custom']['keep']==42
            assert stored['keybindings']['file.save']==['Ctrl+F12']
            call('workflow.open',{})
            time.sleep(.25)
            screenshot(f'{language}-workbench.png')
            for command, name in [(10144, 'canvas-size'), (10145, 'canvas-crop')]:
                call('ui.dispatch', dict(id='command', value=dict(UiCommand=command)))
                time.sleep(.15)
                screenshot(f'{language}-{name}.png')
                # Close the preview without applying any document operation.
                call('ui.dispatch', dict(id='command', value='Cancel'))
            call('ui.dispatch',dict(id='painting.mode',value='fill'))
            time.sleep(.15)
            screenshot(f'{language}-fill-controls.png')
            call('ui.dispatch',dict(id='painting.mode',value='gradient'))
            call('ui.dispatch',dict(id='settings.search',value='ui.language'))
            time.sleep(.25)
            screenshot(f'{language}-settings.png')
            assert base64.b64decode(call('canvas.snapshot',{})['data'])==expected
            assert call('workflow.read',{})['graph']==graph
            # Close only this isolated settings preview through the UI effect API.
            call('ui.dispatch',dict(id='settings.close',value=True))
        call('settings.update',dict(path=str(settings),ui=dict(language='en')))
        # Exercise the live watcher without restarting the GUI or changing artwork.
        call('settings.update',dict(path=str(settings),ui=dict(language='zh-CN')))
        chinese = locale_dir/'zh-CN.json'
        original = chinese.read_text(encoding='utf-8')
        edited = json.loads(original)
        edited['File'] = '热加载测试'
        chinese.write_text(json.dumps(edited,ensure_ascii=False),encoding='utf-8')
        time.sleep(1.2)
        screenshot('hot-reload-valid.png')
        chinese.write_text('{',encoding='utf-8')
        time.sleep(1.2)
        screenshot('hot-reload-invalid-keeps-last.png')
        chinese.write_text(original,encoding='utf-8')
        time.sleep(1.2)
        screenshot('hot-reload-restored.png')
        assert base64.b64decode(call('canvas.snapshot',{})['data'])==expected
        assert call('workflow.read',{})['graph']==graph
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
