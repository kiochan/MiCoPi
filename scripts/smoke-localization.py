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
    shutil.copytree(ROOT/'examples/plugins/quick-tools', plugins/'quick-tools')
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
        call('ui.capture',dict(path=str(path)))
        deadline = time.monotonic()+10
        while not path.exists():
            assert time.monotonic()<deadline,'Screenshot timeout'
            time.sleep(.05)
        return path

    def selection_options_capture(language, width, height):
        result = call('window.command',dict(panel='tool-options',operation='float',x=60,y=80,width=width,height=height))
        assert result['queued']
        deadline = time.monotonic()+10
        while call('window.list',{})['last_result']['status']=='queued':
            assert time.monotonic()<deadline,'Selection options placement timeout'
            time.sleep(.05)
        path=output/f'{language}-wand-{width}.png'
        call('window.capture',dict(panel='tool-options',path=str(path)))
        while not path.exists():
            assert time.monotonic()<deadline,'Selection options capture timeout'
            time.sleep(.05)

    try:
        deadline = time.monotonic()+20
        while not endpoint.exists():
            assert process.poll() is None and time.monotonic()<deadline,'GUI startup failed'
            time.sleep(.05)
        # Availability flags only: do not read or write the interactive clipboard.
        assert call('clipboard.system_state',{})['scope']=='windows'
        inventory = call('plugins.list', {})
        quick = next(p for p in inventory['packages'] if p['id']=='quick')
        assert quick['status']=='loaded' and 'locales' in quick['capabilities']
        assert inventory['contribution_owners']['panels']['quick.panel']=='quick'
        assert inventory['lifecycle']['locales']=='hot_reload'
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
        call('selection.shape',dict(kind='ellipse',x=30,y=30,width=180,height=120))
        expected = base64.b64decode(call('canvas.snapshot',{})['data'])
        graph=dict(version=1,nodes=[dict(id=1,type='native_paint.statistics',position=[0,0],parameters={}),dict(id=2,type='action.native.checkerboard',position=[460,0],parameters={})],edges=[dict(**{'from':1},output='calls',to=2,input='cell')])
        call('workflow.set',dict(graph=graph))
        call('settings.update',dict(path=str(settings),selection=dict(tolerance=19,contiguous=False,sample_merged=True)))
        call('ui.dispatch',dict(id='painting.mode',value='gradient'))
        for language in ['en','zh-CN','ja','ko']:
            call('settings.update',dict(path=str(settings),ui=dict(language=language)))
            stored=json.loads(settings.read_text(encoding='utf-8-sig'))
            assert stored['ui']['language']==language and stored['custom']['keep']==42
            assert stored['keybindings']['file.save']==['Ctrl+F12']
            call('toolbox.mode.selection_ellipse',{})
            call('toolbox.mode.selection_wand',{})
            call('toolbox.selection.options',dict(tolerance=16,contiguous=True,sample_merged=True))
            call('workflow.open',{})
            time.sleep(.25)
            screenshot(f'{language}-workbench.png')
            for width,height in [(280,300),(1000,150)]:
                selection_options_capture(language,width,height)
            call('window.command',dict(panel='tool-options',operation='dock_top',target='canvas'))

            call('window.command', dict(panel='quick.panel',operation='focus'))
            time.sleep(.15)
            screenshot(f'{language}-plugin-panel.png')
            call('window.command', dict(panel='layers',operation='focus'))
            for command, name in [(10144, 'canvas-size'), (10145, 'canvas-crop'), (10173, 'image-size')]:
                call('ui.dispatch', dict(id='command', value=dict(UiCommand=command)))
                time.sleep(.15)
                screenshot(f'{language}-{name}.png')
                # Close the preview without applying any document operation.
                call('ui.dispatch', dict(id='command', value='Cancel'))
            # Dynamic action commands use the same stable identity as Keymap.
            for action in ['selection.feather','selection.smooth','clipboard.paste']:
                identity=2166136261
                for byte in ('plugin.'+action).encode():
                    identity=((identity^byte)*16777619)&0xffffffff
                call('ui.dispatch',dict(id='command',value=dict(UiCommand=identity|0x80000000)))
                time.sleep(.15)
                screenshot(f'{language}-{action}.png')
                call('ui.dispatch',dict(id='command',value='Cancel'))
            call('canvas.resize',dict(width=320,height=256,anchor='center'))
            call('ui.dispatch',dict(id='command',value=dict(UiCommand=10174)))
            time.sleep(.15)
            screenshot(f'{language}-canvas-trim.png')
            call('ui.dispatch',dict(id='command',value='Cancel'))
            call('history.undo',{})
            call('canvas.image_size',dict(width=128,method='area'))
            time.sleep(.15)
            screenshot(f'{language}-area-reduction.png')
            # Exercise the GUI Undo adapter, which must resize its own texture cache.
            call('ui.dispatch',dict(id='command',value='Undo'))
            deadline=time.monotonic()+10
            while call('canvas.state',{})['width']!=256:
                assert process.poll() is None and time.monotonic()<deadline,'Area undo timeout'
                time.sleep(.05)
            call('canvas.rotate_90_cw',{})
            time.sleep(.15)
            screenshot(f'{language}-canvas-quarter-turn.png')
            call('history.undo',{})
            call('layer.via_copy',{})
            time.sleep(.15)
            screenshot(f'{language}-selection-to-layer.png')
            call('history.undo',{})
            call('ui.dispatch',dict(id='painting.mode',value='fill'))
            time.sleep(.15)
            screenshot(f'{language}-fill-controls.png')
            call('ui.dispatch',dict(id='painting.mode',value='gradient'))
            call('ui.dispatch',dict(id='settings.search',value='ui.language'))
            time.sleep(.25)
            screenshot(f'{language}-settings.png')
            call('ui.dispatch',dict(id='settings.search',value='selection.'))
            time.sleep(.15)
            screenshot(f'{language}-selection-settings.png')
            assert call('canvas.state',{})['selection_defaults']==dict(tolerance=19,contiguous=False,sample_merged=True)

            call('ui.dispatch',dict(id='settings.search',value='plugins'))
            time.sleep(.15)
            screenshot(f'{language}-plugins.png')
            assert base64.b64decode(call('canvas.snapshot',{})['data'])==expected
            assert call('workflow.read',{})['graph']==graph
            # Close only this isolated settings preview through the UI effect API.
            call('ui.dispatch',dict(id='settings.close',value=True))
        call('settings.update',dict(path=str(settings),ui=dict(language='en')))
        # Exercise the live watcher without restarting the GUI or changing artwork.
        call('settings.update',dict(path=str(settings),ui=dict(language='zh-CN')))
        call('window.command',dict(panel='quick.panel',operation='focus'))
        plugin_locale=plugins/'quick-tools/locales/zh-CN.json'
        plugin_original=plugin_locale.read_text(encoding='utf-8')
        plugin_edited=json.loads(plugin_original)
        plugin_edited['Select Blue Soft Brush']='插件热加载测试'
        plugin_locale.write_text(json.dumps(plugin_edited,ensure_ascii=False),encoding='utf-8')
        time.sleep(1.2)
        screenshot('plugin-hot-reload-valid.png')
        plugin_locale.write_text('{',encoding='utf-8')
        time.sleep(1.2)
        screenshot('plugin-hot-reload-invalid-keeps-last.png')
        plugin_locale.write_text(plugin_original,encoding='utf-8')
        time.sleep(1.2)
        screenshot('plugin-hot-reload-restored.png')
        call('window.command',dict(panel='layers',operation='focus'))
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
