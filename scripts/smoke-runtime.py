"""End-to-end: JSONL, MCP stdio and live GUI share identical canvas pixels.

Runs only its own GUI process with an isolated profile and closes that process.
Leaves output under target/test-output/runtime-smoke for inspection.
"""
import argparse
import base64
import json
from pathlib import Path
import shutil
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]


def message(method, params=None, number=None):
    result = dict(jsonrpc="2.0", method=method)
    if params is not None:
        result["params"] = params
    if number is not None:
        result["id"] = number
    return result


def main():
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument("--cli", default="dist/micopi-cli.exe")
    parser.add_argument("--gui", default="dist/MiCoPi.exe")
    parser.add_argument("--skip-gui", action="store_true")
    args = parser.parse_args()
    cli = (ROOT / args.cli).resolve()
    gui = (ROOT / args.gui).resolve()
    output = ROOT / "target/test-output/runtime-smoke" / str(time.time_ns())
    output.mkdir(parents=True)
    plugins = output / "plugins"
    shutil.copytree(ROOT / "plugins", plugins)
    shutil.copytree(ROOT / "examples/plugins/quick-tools", plugins / "quick-tools")
    profile = output / "profile"
    profile.mkdir()
    selection_defaults=dict(tolerance=0,contiguous=False,sample_merged=False)
    (profile/'settings.json').write_text(json.dumps(dict(version=1,selection=selection_defaults)),encoding='utf-8')
    startup_state=subprocess.run([str(cli),'call','canvas.state','{}','--plugins',str(plugins),'--config-dir',str(profile)],capture_output=True,text=True,encoding='utf-8',check=True)
    assert json.loads(startup_state.stdout)['result']['selection_defaults']==selection_defaults

    commands = [
        dict(action="canvas.new", args=dict(width=256, height=192)),
        dict(action="pipeline.run", args=dict(steps=[
            dict(id="context",action="assistant.context",args={}),
            dict(id="paint",action="assistant.checkerboard",args=dict(cell=32)),
            dict(id="read",action="canvas.state",args={}),
            dict(id="view",action="view.set",args=dict(zoom={"$ref":"read#/result/view/zoom"})),
        ])),
        dict(action="brush.set", args=dict(color="#0077ff", radius=9, stabilizer=0, pressure_size=True,pressure_opacity=True,spacing=.2,minimum_opacity=.25)),
        dict(action="tool.select", args=dict(tool="brush")),
        dict(action="canvas.stroke", args=dict(points=[
            dict(x=28, y=130, pen=True, pressure=.2, time=10),
            dict(x=64, y=150, pen=True, pressure=.5, time=26),
            dict(x=112, y=120, pen=True, pressure=.8, time=42),
            dict(x=180, y=54, pen=True, pressure=1., time=58),
            dict(x=226, y=68, pen=True, pressure=.4, time=74),
        ])),
        dict(action="quick.blue_stroke", args=dict(points=[
            dict(x=40, y=165, pressure=.5, pen=True),
            dict(x=120, y=165, pressure=1., pen=True),
            dict(x=210, y=165, pressure=.3, pen=True),
        ])),
    ]
    commands += [
        dict(action="layer.rename", args=dict(layer_id=1, name="Ink 墨")),
        dict(action="layer.duplicate", args=dict(layer_id=1)),
        dict(action="layer.flip_horizontal", args={}),
        dict(action="layer.flip_vertical", args={}),
        dict(action="layer.rotate_180", args={}),
        dict(action="layer.translate", args=dict(dx=7, dy=-3)),
        dict(action="history.undo", args={}),
        dict(action="history.redo", args={}),
        dict(action="layer.move", args=dict(layer_id=2, to=0)),
        dict(action="layer.move", args=dict(layer_id=2, to=1)),
        dict(action="layer.delete", args=dict(layer_id=2)),
        dict(action="layer.fill", args=dict(x=0,y=0,color="#228855",tolerance=8,opacity=.7,contiguous=False,expand=1,feather=2)),
        dict(action="layer.gradient", args=dict(x0=10,y0=20,x1=130,y1=90,start_color="#ee7700cc",end_color="#0033ff00",kind="radial",opacity=.65,spread="reflect",reverse=True)),
        dict(action="native.invert_region", args=dict(x=8,y=8,width=128,height=96)),
        dict(action="history.undo",args={}),
        dict(action="history.redo",args={}),
        dict(action="native.invert",args={}),
        dict(action="native.stats",args={}),
    ]
    commands += [
        dict(action="layer.add", args={}),
        dict(action="layer.fill_color", args=dict(color="#33445580")),
        dict(action="selection.shape", args=dict(kind="rectangle", x=30, y=30, width=110, height=75)),
        dict(action="selection.shape", args=dict(kind="rectangle", x=170, y=30, width=30, height=75, mode="add")),
        dict(action="layer.fill", args=dict(x=40,y=40,color="#ee5500",expand=2,feather=2)),
        dict(action="history.undo", args={}),
        dict(action="layer.fill_color", args=dict(color="#5522cc80")),
        dict(action="canvas.stroke", args=dict(points=[dict(x=0,y=50,pen=True,pressure=.3),dict(x=190,y=60,pen=True,pressure=1.)])),
        dict(action="selection.invert", args={}),
        dict(action="native.invert", args={}),
        dict(action="selection.deselect", args={}),
        dict(action="selection.reselect", args={}),
        dict(action="selection.expand", args=dict(radius=3)),
        dict(action="selection.contract", args=dict(radius=2,edge="zero")),
        dict(action="selection.feather", args=dict(radius=2)),
        dict(action="selection.smooth", args=dict(radius=1)),
        dict(action="selection.wand", args=dict(x=40, y=40, tolerance=0, contiguous=True)),
        dict(action="selection.grow", args=dict(tolerance=8)),
        dict(action="selection.similar", args=dict(tolerance=8, sample_merged=True)),
        dict(action="selection.wand", args=dict(x=40, y=40, tolerance=32, contiguous=False, sample_merged=True, mode="add")),
        dict(action="selection.all", args={}),
        dict(action="selection.read", args={}),
        dict(action="layer.via_copy", args={}),
        dict(action="history.undo", args={}),
        dict(action="layer.via_cut", args={}),
        dict(action="history.undo", args={}),
        dict(action="history.redo", args={}),
        dict(action="history.undo", args={}),
        dict(action="clipboard.copy", args={}),
        dict(action="clipboard.state", args={}),
        dict(action="clipboard.cut", args={}),
        dict(action="history.undo", args={}),
        dict(action="clipboard.paste_in_place", args={}),
        dict(action="history.undo", args={}),
        dict(action="clipboard.copy_merged", args={}),
        dict(action="clipboard.paste", args=dict(x=-10,y=20)),
        dict(action="history.undo", args={}),
        dict(action="history.redo", args={}),
        dict(action="history.undo", args={}),
        dict(action="selection.deselect", args={}),
        dict(action="layer.fill_color", args=dict(color="#dd884480")),
        dict(action="layer.stamp_visible", args={}),
        dict(action="history.undo", args={}),
        dict(action="layer.merge_down", args={}),
        dict(action="history.undo", args={}),
        dict(action="layer.merge_visible", args={}),
        dict(action="history.undo", args={}),
        dict(action="layer.flatten", args={}),
        dict(action="history.undo", args={}),
        dict(action="layer.delete", args={}),
        dict(action="replay.verify", args={}),
    ]
    graph=dict(enabled=True,nodes=[
        dict(id=0,kind="layer",index=0,position=[0,20]),
        dict(id=1,kind="filter",filter="brightness",amount=.125,position=[230,20]),
        dict(id=2,kind="layer",index=1,position=[230,260]),
        dict(id=3,kind="blend",mode="multiply",opacity=.75,position=[460,20]),
        dict(id=4,kind="output",position=[690,20]),
    ])
    graph["edges"]=[{"from":0,"to":1,"input":0},{"from":1,"to":3,"input":0},{"from":2,"to":3,"input":1},{"from":3,"to":4,"input":0}]
    for node in graph["nodes"]:
        node["node_type"] = "compositing." + node["kind"]
    commands += [
        dict(action="layer.add",args={}),
        dict(action="brush.set",args=dict(color="#f29a50",radius=20,opacity=.65)),
        dict(action="canvas.stroke",args=dict(points=[dict(x=40,y=80),dict(x=210,y=100)])),
        dict(action="layer.set",args=dict(alpha_locked=True)),
        dict(action="layer.fill_color",args=dict(color="#a64eff",opacity=.6)),
        dict(action="canvas.stroke",args=dict(points=[dict(x=60,y=80),dict(x=180,y=100)])),
        dict(action="graph.set",args=dict(graph=graph)),
        dict(action="history.undo",args={}),dict(action="history.redo",args={}),
    ]
    batch_png = output / "batch.png"
    commands += [
        dict(action="canvas.resize", args=dict(width=280, height=210, anchor="bottom_right")),
        dict(action="canvas.crop", args=dict(x=7, y=5, width=260, height=195)),
        dict(action="history.undo", args={}),
        dict(action="history.redo", args={}),
    ]
    commands += [dict(action=action,args={}) for action in [
        "canvas.rotate_90_cw", "canvas.flip_horizontal", "canvas.rotate_180",
        "canvas.rotate_90_ccw", "canvas.flip_vertical",
    ]]
    commands.append(dict(action="layer.set",args=dict(alpha_locked=False)))
    commands += [dict(action=action,args={}) for action in [
        "layer.rotate_90_cw", "history.undo", "layer.rotate_90_ccw", "history.undo",
    ]]
    commands += [
        dict(action="canvas.image_size",args=dict(width=130,height=98,lock_aspect=False,method="bilinear")),
        dict(action="canvas.image_size",args=dict(width=65,height=49,lock_aspect=False,method="area")),
        dict(action="canvas.image_size",args=dict(width=260,height=195,lock_aspect=False,method="nearest")),
    ]
    script = output / "draw.jsonl"
    script.write_text("\n".join(json.dumps(c) for c in commands + [dict(action="canvas.export", args=dict(path=str(batch_png)))]) + "\n", encoding="utf-8")
    run = subprocess.run([str(cli), "run", str(script), "--plugins", str(plugins), "--config-dir", str(profile)], capture_output=True, text=True, encoding="utf-8", timeout=30)
    assert run.returncode == 0, run.stderr + run.stdout
    assert all(json.loads(line)["ok"] for line in run.stdout.splitlines())
    pipeline=json.loads(run.stdout.splitlines()[1])["data"]["result"]
    assert pipeline["ok"] and pipeline["completed"]==["context","paint","read","view"]
    assert batch_png.read_bytes().startswith(b"\x89PNG\r\n\x1a\n")
    requests = [message("initialize", dict(protocolVersion="2025-11-25", capabilities={}, clientInfo=dict(name="smoke", version="1")), 1), message("notifications/initialized"), message("tools/list", {}, 2)]
    requests += [message("tools/call", dict(name=c["action"], arguments=c["args"]), n) for n, c in enumerate(commands, 3)]
    requests.append(message("tools/call", dict(name="canvas.state", arguments={}), 10000))
    requests.append(message("tools/call", dict(name="canvas.snapshot", arguments={}), 40000))
    requests += [message("resources/list",{},40101),message("resources/read",dict(uri="micopi://state/ai_assistant.context"),40102)]
    run = subprocess.run([str(cli), "mcp", "--plugins", str(plugins), "--config-dir", str(profile)], input="\n".join(json.dumps(r) for r in requests) + "\n", capture_output=True, text=True, encoding="utf-8", timeout=30)
    assert run.returncode == 0, run.stderr
    replies = [json.loads(line) for line in run.stdout.splitlines()]
    assert all("error" not in r and not r.get("result", {}).get("isError") for r in replies), replies
    defaults_reply=next(r for r in replies if r['id']==10000)['result']
    defaults_state=defaults_reply.get('structuredContent') or json.loads(defaults_reply['content'][0]['text'])
    assert defaults_state['result']['selection_defaults']==selection_defaults
    tools = next(r for r in replies if r["id"] == 2)["result"]["tools"]
    assert any(t["name"] == "quick.blue_stroke" for t in tools), "Custom plugin missing from MCP tools"
    assert any(t["name"] == "native.invert" for t in tools), "Native DLL action missing from MCP tools"
    for name in ("layer.merge_down", "layer.merge_visible", "layer.flatten", "layer.stamp_visible", "selection.wand", "selection.grow", "selection.similar"):
        assert any(t["name"] == name for t in tools), f"Layer consolidation action missing from MCP tools: {name}"
    assert any(r["uri"]=="micopi://state/ai_assistant.context" for r in next(r for r in replies if r["id"]==40101)["result"]["resources"])
    context=json.loads(next(r for r in replies if r["id"]==40102)["result"]["contents"][0]["text"])
    assert context["resources"]["native_paint.stats"]["ok"]
    snapshot = next(r for r in replies if r["id"] == 40000)["result"]["content"][0]
    assert base64.b64decode(snapshot["data"]) == batch_png.read_bytes()
    (output / "mcp.png").write_bytes(base64.b64decode(snapshot["data"]))
    print("JSONL and MCP stdio: native DLL filters produce identical PNG pixels/bytes")
    if args.skip_gui:
        print(output)
        return
    startup = subprocess.STARTUPINFO()
    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = 0
    process = subprocess.Popen([str(gui), "--config-dir", str(profile), "--plugins", str(plugins), "--config-dir", str(profile)], startupinfo=startup)
    endpoint = profile / "session.json"

    def call(action, arguments):
        result = subprocess.run([str(cli), "call", action, json.dumps(arguments), "--attach", str(endpoint)], capture_output=True, text=True, encoding="utf-8", timeout=35)
        assert result.returncode == 0, result.stderr
        return json.loads(result.stdout)

    try:
        deadline = time.monotonic() + 15
        while not endpoint.exists():
            assert process.poll() is None, "GUI exited before creating session.json"
            assert time.monotonic() < deadline, "GUI IPC startup timed out"
            time.sleep(.05)
        for command in commands:
            result=call(command["action"], command["args"])
            if isinstance(result["result"],dict):
                assert result["result"].get("ok",True), result
        assert call("app.describe",{})["result"]["runtime"]=="desktop"
        state = call("canvas.state", {})["result"]
        assert state["selection_defaults"] == selection_defaults
        assert state["width"] == 260 and state["height"] == 195 and state["dirty"]
        shot = call("canvas.snapshot", {})["result"]
        assert base64.b64decode(shot["data"]) == batch_png.read_bytes()
        (output / "gui-canvas.png").write_bytes(base64.b64decode(shot["data"]))
        call("graph.open",{})
        nodes_capture=output/"gui-nodes.png"
        call("ui.dispatch",dict(id="screenshot",value=str(nodes_capture)))
        deadline=time.monotonic()+10
        while not nodes_capture.exists():
            assert time.monotonic()<deadline,"Node editor screenshot timed out"
            time.sleep(.05)
        graph_state=call("graph.read",{})["result"]
        assert graph_state["graph"]==graph and graph_state["renderer_available"]
        project=output/"node-roundtrip.mcp"
        call("document.save",dict(path=str(project)))
        call("document.open",dict(path=str(project)))
        assert call("graph.read",{})["result"]["graph"]==graph
        assert base64.b64decode(call("canvas.snapshot",{})["result"]["data"])==batch_png.read_bytes()
        # Streaming input must survive GUI frames even when the window has no focus.
        call("canvas.pointer", dict(phase="down", x=25, y=25, pen=True, pressure=.3, time=100))
        assert call("canvas.state", {})["result"]["drawing"]
        call("canvas.pointer", dict(phase="move", x=90, y=25, pen=True, pressure=.8, time=120))
        call("canvas.pointer", dict(phase="up", x=120, y=25, pen=True, pressure=0, time=140))
        assert not call("canvas.state", {})["result"]["drawing"]
        capture = output / "gui-workbench.png"
        call("ui.dispatch", dict(id="screenshot", value=str(capture)))
        deadline = time.monotonic() + 10
        while not capture.exists():
            assert time.monotonic() < deadline, "GUI screenshot timed out"
            time.sleep(.05)
        # The same settings action updates the live GUI and persists headless defaults.
        settings_path = profile / "settings.json"
        call("settings.update", dict(path=str(settings_path), ui=dict(panel_padding=16), painting=dict(pressure_gamma=1.5), plugins=dict(disabled=["quick"])))
        configuration = call("settings.read", dict(path=str(settings_path)))["result"]
        assert configuration["painting"]["pressure_gamma"] == 1.5
        call("ui.dispatch", dict(id="settings.search", value="压感"))
        time.sleep(.35)  # Let egui's window fade finish before visual inspection.
        settings_capture = output / "settings-search.png"
        call("ui.dispatch", dict(id="screenshot", value=str(settings_capture)))
        deadline = time.monotonic() + 10
        while not settings_capture.exists():
            assert time.monotonic() < deadline, "Settings screenshot timed out"
            time.sleep(.05)
        # Plugin disable applies at the next startup and leaves manifests intact.
        listing = subprocess.run([str(cli), "actions", "--plugins", str(plugins), "--config-dir", str(profile)], capture_output=True, text=True, encoding="utf-8", timeout=30, check=True)
        assert not any(a["id"] == "quick.blue_stroke" for a in json.loads(listing.stdout))
        assert (plugins / "quick-tools/manifest.json").is_file()
        call("ui.dispatch", dict(id="close", value=True))
        assert process.wait(timeout=10) == 0
        print("Live GUI attachment: identical canvas; streaming input, searchable settings and plugin preferences passed")
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=10)
    print(output)


if __name__ == "__main__":
    main()
