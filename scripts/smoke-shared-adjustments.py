"""Tiny JSONL/MCP shared per-source binding, preview and verified outcome gates."""
import argparse
import importlib.util
from pathlib import Path
import shutil
import time

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('shared_adjustment_geometry', ROOT/'scripts/smoke-mask-geometry.py')
geometry = importlib.util.module_from_spec(spec)
spec.loader.exec_module(geometry)
editing, Plan = geometry.editing, geometry.Plan


def commands():
    p = Plan()
    p.add('nodes', 'node.catalog')
    p.add('new', 'canvas.new', width=7, height=5)
    p.add('fill', 'layer.fill_color', color='#4879AB')
    p.add('copy', 'layer.duplicate', layer_id=1)
    p.add('controller', 'adjustment.create', node_type='compositing.filter',
          parameters=dict(filter='invert', amount=1), target_layer_ids=[1, 2])
    p.add('bound', 'adjustment.get', adjustment_id=1)
    p.add('edit', 'pipeline.run', atomic=True, steps=[
        dict(id='read', action='graph.read', args={}),
        dict(id='before', action='canvas.state', args={}),
        dict(id='preview', action='adjustment.preview', args=dict(
            document_identity={'$ref': 'read#/result/document_identity'},
            document_revision={'$ref': 'read#/result/revision'}, adjustment_id=1,
            target_layer_ids=[2], parameters=dict(amount=.5))),
        dict(id='after', action='canvas.state', args={}),
        dict(id='apply', action='adjustment.set', args={'$ref': 'preview#/result/apply/args'})])
    p.add('image', 'canvas.snapshot')
    p.add('undo', 'history.undo')
    p.add('original-binding', 'adjustment.get', adjustment_id=1)
    p.add('redo', 'history.redo')
    p.add('redone-binding', 'adjustment.get', adjustment_id=1)
    p.add('empty', 'adjustment.set', adjustment_id=1, target_layer_ids=[])
    p.add('empty-state', 'adjustment.get', adjustment_id=1)
    p.add('verify', 'replay.verify')
    return p


def validate(values, p):
    at = lambda k: p.at(values, k)
    assert at('bound')['scope'] == 'targets'
    assert at('bound')['target_layer_ids'] == [1, 2]
    edit = at('edit')
    assert edit['ok'], edit
    r = edit['results']
    assert r['before']['result'] == r['after']['result']
    assert r['preview']['result']['data'] == at('image')['data']
    assert at('original-binding')['target_layer_ids'] == [1, 2]
    assert at('redone-binding')['target_layer_ids'] == [2]
    assert at('empty-state')['scope'] == 'targets' and at('empty-state')['target_layer_ids'] == []
    assert at('verify')['valid']
    assert any(n['id'] == 'action.adjustment.preview' for n in at('nodes')['nodes'])
    return at('image')['data']


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli', default='target/build/x86_64-pc-windows-gnu/release/micopi-cli.exe')
    cli = (ROOT/parser.parse_args().cli).resolve()
    output = ROOT/'target/test-output/shared-adjustments-smoke'/str(time.time_ns())
    output.mkdir(parents=True)
    plugins, profile = output/'plugins', output/'profile'
    shutil.copytree(ROOT/'plugins', plugins)
    profile.mkdir()
    flags = ['--plugins', str(plugins), '--config-dir', str(profile)]
    p = commands()
    batch = validate(geometry.run_jsonl(cli, flags, p, output/'shared-adjustments.jsonl'), p)
    mcp = validate(editing.run_mcp(cli, flags, p), p)
    assert batch == mcp
    print('Shared adjustments: explicit source binding, reversible PNG preview, Undo/Redo and outcomes passed.')


if __name__ == '__main__':
    main()
