"""Verify outcome replay across JSONL, MCP and an isolated desktop session."""
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
    parser.add_argument('--skip-gui', action='store_true')
    args = parser.parse_args()
    cli, gui = (ROOT / args.cli).resolve(), (ROOT / args.gui).resolve()
    output = ROOT / 'target/test-output/replay-smoke' / str(time.time_ns())
    output.mkdir(parents=True)
    plugins, profile = output/'plugins', output/'profile'
    shutil.copytree(ROOT/'plugins', plugins)
    profile.mkdir()
    flags = ['--plugins', str(plugins), '--config-dir', str(profile)]
    archive, project = output/'drawing.mcreplay', output/'drawing.mcp'
    commands = [
        dict(action='canvas.new', args=dict(width=640, height=420)),
        dict(action='brush.set', args=dict(radius=9, color='#668ff0', stabilizer=.35, pressure_size=True)),
    ]
    # Pressure-controlled waves make the recorded progression easy to inspect.
    import math
    for row in range(8):
        commands.append(dict(action='brush.set', args=dict(color=['#668ff0','#8c79d8','#db8cb5','#70c7b0'][row%4],radius=7+row)))
        commands.append(dict(action='canvas.stroke',args=dict(points=[dict(x=50+n*9,y=65+row*38+math.sin(n*.18)*18,pen=True,pressure=.2+.8*math.sin(n/60*math.pi),time=n*10) for n in range(61)])))
    commands.append(dict(action='graph.set',args=dict(graph=dict(enabled=True,nodes=[
        dict(id=0,kind='layer',node_type='compositing.layer',index=0,position=[0,0]),
        dict(id=1,kind='filter',node_type='compositing.filter',filter='brightness',amount=.08,position=[230,0]),
        dict(id=2,kind='output',node_type='compositing.output',position=[460,0]),
    ],edges=[{'from':0,'to':1,'input':0},{'from':1,'to':2,'input':0}]))))
    commands=commands[:2]+[dict(action='pipeline.run',args=dict(atomic=True,steps=[dict(id=f'paint_{i}',**command) for i,command in enumerate(commands[2:])]))]
    commands.extend([
        dict(action='canvas.transform',args=dict(target='layer',translate_x=4.5,angle=7,skew_x=12,skew_y=-8,pivot_x=320,pivot_y=210)),
        dict(action='canvas.transform',args=dict(target='layer',quad=[[60,50],[580,75],[550,370],[80,350]])),
        dict(action='canvas.transform.repeat_copy',args={}),
        dict(action='history.undo',args={}),dict(action='history.redo',args={}),
        dict(action='document.save',args=dict(path=str(project))),
        dict(action='replay.verify',args={}),
        dict(action='replay.export',args=dict(path=str(archive))),
        dict(action='canvas.snapshot',args={}),
        dict(action='replay.snapshot',args={}),
        dict(action='replay.frames',args=dict(path=str(output/'frames'),max_frames=4)),
    ])
    path = output/'draw.jsonl'
    path.write_text('\n'.join(json.dumps(c) for c in commands)+'\n',encoding='utf-8')
    run = subprocess.run([str(cli),'run',str(path),*flags],capture_output=True,text=True,encoding='utf-8',timeout=60)
    assert run.returncode==0,run.stdout[-1000:]+run.stderr
    results = [json.loads(line)['data']['result'] for line in run.stdout.splitlines()]
    expected = base64.b64decode(results[-3]['data'])
    assert expected==base64.b64decode(results[-2]['data'])
    frames = json.loads((output/'frames/manifest.json').read_text())['frames']
    assert len(frames)<=4 and (output/'frames'/frames[-1]['file']).read_bytes()==expected
    assert project.read_bytes().startswith(b'MICOPI03')
    # A fresh MCP process can replay without the original drawing/compositor DLLs.
    minimal=output/'minimal-plugins'
    minimal.mkdir()
    for name in ['core','replay']:
        shutil.copytree(ROOT/'plugins'/name,minimal/name)
    requests=[dict(jsonrpc='2.0',id=0,method='initialize',params=dict(protocolVersion='2025-11-25',capabilities={},clientInfo=dict(name='replay-smoke',version='1'))),dict(jsonrpc='2.0',method='notifications/initialized')]
    for i,(name,arguments) in enumerate([
        ('replay.load',dict(path=str(archive))),('replay.verify',dict(source='loaded')),
        ('replay.snapshot',dict(source='loaded')),
    ],1):
        requests.append(dict(jsonrpc='2.0',id=i,method='tools/call',params=dict(name=name,arguments=arguments)))
    requests.extend([
        dict(jsonrpc='2.0',id=4,method='tools/call',params=dict(name='pipeline.run',arguments=dict(atomic=True,steps=[dict(id='add',action='layer.add'),dict(id='fail',action='layer.select',args=dict(index=12))]))),
        dict(jsonrpc='2.0',id=5,method='tools/call',params=dict(name='canvas.state',arguments={})),
        dict(jsonrpc='2.0',id=6,method='tools/call',params=dict(name='replay.snapshot',arguments=dict(source='loaded'))),
    ])
    mcp=subprocess.run([str(cli),'mcp','--plugins',str(minimal),'--config-dir',str(profile)],input='\n'.join(json.dumps(r) for r in requests)+'\n',capture_output=True,text=True,encoding='utf-8',timeout=60)
    assert mcp.returncode==0,mcp.stderr
    replies=[json.loads(line) for line in mcp.stdout.splitlines()]
    for reply in replies:
        assert not reply.get('error'),reply
        assert bool(reply['result'].get('isError'))==(reply['id']==4),reply
    state=json.loads(next(r for r in replies if r['id']==5)['result']['content'][0]['text'])
    state=state.get('result',state)
    assert len(state['layers'])==1 and state['active_layer']==0 and state['dirty']==False
    content=replies[-1]['result']['content'][0]
    assert content['type']=='image' and base64.b64decode(content['data'])==expected
    if not args.skip_gui:
        startup=subprocess.STARTUPINFO(); startup.dwFlags|=subprocess.STARTF_USESHOWWINDOW; startup.wShowWindow=0
        process=subprocess.Popen([str(gui),*flags],startupinfo=startup)
        endpoint=profile/'session.json'
        def call(name,arguments):
            result=subprocess.run([str(cli),'call',name,json.dumps(arguments),'--attach',str(endpoint)],capture_output=True,text=True,encoding='utf-8',timeout=60)
            assert result.returncode==0,result.stdout+result.stderr
            return json.loads(result.stdout)['result']
        try:
            deadline=time.monotonic()+30
            while not endpoint.exists():
                assert process.poll() is None and time.monotonic()<deadline,'GUI startup failed'
                time.sleep(.05)
            call('document.open',dict(path=str(project)))
            assert call('canvas.transform.state',{})['available']
            assert len(call('canvas.state',{})['layers'])==2
            assert call('replay.verify',{})['valid']
            assert base64.b64decode(call('canvas.snapshot',{})['data'])==expected
            assert base64.b64decode(call('replay.snapshot',{})['data'])==expected
            call('replay.open',{})
            time.sleep(.5)
            capture=output/'replay.png'
            call('ui.dispatch',dict(id='screenshot',value=str(capture)))
            deadline=time.monotonic()+10
            while not capture.exists():
                assert time.monotonic()<deadline,'Screenshot timeout'
                time.sleep(.05)
            assert base64.b64decode(call('canvas.snapshot',{})['data'])==expected
            call('ui.dispatch',dict(id='close',value=True))
            assert process.wait(timeout=15)==0
        finally:
            if process.poll() is None: process.kill(); process.wait(timeout=10)
    print('Atomic JSONL drawing and MCP rollback, replay PNG parity, sealed project/archive verification, bounded frame export and DLL-independent playback passed.')
    print(f'Compressed replay: {archive.stat().st_size:,} bytes')
    print(output)


if __name__=='__main__': main()
