"""Tiny JSONL/MCP Liquify preview, grouped Undo and replay transport gates.

Native scalar/pixel references run in Rust. This smoke checks shared action
execution and preview parity; it makes no latency or physical tablet claim.
"""
import argparse
import importlib.util
from pathlib import Path
import shutil
import time

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('liquify_geometry', ROOT/'scripts/smoke-mask-geometry.py')
geometry = importlib.util.module_from_spec(spec)
spec.loader.exec_module(geometry)
editing, Plan = geometry.editing, geometry.Plan


def commands():
    p = Plan()
    p.add('nodes', 'node.catalog')
    p.add('new', 'canvas.new', width=12, height=8, background='transparent')
    p.add('checker', 'native.checkerboard', cell=2)
    p.add('duplicate', 'layer.duplicate', layer_id=1)
    p.add('mask', 'mask.create', owner=dict(kind='layer', id=1), select=False)
    p.add('before', 'canvas.snapshot')
    args = dict(document_identity={'$ref': 'read#/result/document_identity'},
                document_revision={'$ref': 'read#/result/revision'}, layer_ids=[2, 1],
                spacing=1, steps=[[dict(x=5, y=4, dx=1, dy=.5, radius=4, strength=.8)]])
    p.add('edit', 'pipeline.run', atomic=True, steps=[
        dict(id='read', action='graph.read', args={}),
        dict(id='before', action='canvas.state', args={}),
        dict(id='plan', action='canvas.liquify.plan', args=args),
        dict(id='preview', action='canvas.liquify.preview', args=args),
        dict(id='after', action='canvas.state', args={}),
        dict(id='apply', action='canvas.liquify', args=args)])
    p.add('after', 'canvas.snapshot')
    p.add('undo', 'history.undo')
    p.add('undone', 'canvas.snapshot')
    p.add('redo', 'history.redo')
    p.add('redone', 'canvas.snapshot')
    p.add('verify', 'replay.verify')
    return p


def validate(values, p):
    at = lambda k: p.at(values, k)
    edit = at('edit')
    assert edit['ok'], edit
    results = edit['results']
    assert results['before']['result'] == results['after']['result']
    assert results['plan']['result']['mask_ids'] == [1]
    assert results['apply']['result']['changed']
    assert results['preview']['result']['data'] == at('after')['data']
    assert at('undone')['data'] == at('before')['data']
    assert at('redone')['data'] == at('after')['data']
    assert at('verify')['valid']
    for action in ['canvas.liquify', 'canvas.liquify.plan', 'canvas.liquify.preview']:
        assert any(node['id'] == 'action.'+action for node in at('nodes')['nodes'])
    return at('after')['data']


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli', default='target/build/x86_64-pc-windows-gnu/release/micopi-cli.exe')
    cli = (ROOT/parser.parse_args().cli).resolve()
    output = ROOT/'target/test-output/liquify-smoke'/str(time.time_ns())
    output.mkdir(parents=True)
    plugins, profile = output/'plugins', output/'profile'
    shutil.copytree(ROOT/'plugins', plugins)
    profile.mkdir()
    flags = ['--plugins', str(plugins), '--config-dir', str(profile)]
    p = commands()
    batch = validate(geometry.run_jsonl(cli, flags, p, output/'liquify.jsonl'), p)
    mcp = validate(editing.run_mcp(cli, flags, p), p)
    assert batch == mcp
    print('Liquify: shared JSONL/MCP previews, Apply, one Undo/Redo and verified outcomes passed.')


if __name__ == '__main__':
    main()
