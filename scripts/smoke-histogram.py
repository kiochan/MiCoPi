"""Check bounded read-only color statistics across JSONL, MCP and schema nodes.

Tiny uniform references are independent of the native sampling implementation.
No GUI, stress workload, benchmark or physical-device claim is involved.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('histogram_geometry_helpers', ROOT/'scripts/smoke-mask-geometry.py')
geometry = importlib.util.module_from_spec(spec)
spec.loader.exec_module(geometry)
editing, Plan = geometry.editing, geometry.Plan


def commands(*, negative=False):
    p = Plan()
    p.add('nodes', 'node.catalog')
    p.add('new', 'canvas.new', width=4, height=4)
    p.add('fill', 'layer.fill_color', color='#204060')
    p.add('before', 'canvas.state')
    p.add('raw', 'histogram.query', source='layer', layer_id=1)
    p.add('sampled', 'histogram.query', max_samples=4)
    p.add('after', 'canvas.state')
    p.add('hidden', 'layer.set', layer_id=1, visible=False, pixel_locked=True, position_locked=True)
    p.add('locked-raw', 'histogram.query', source='layer', layer_id=1.0, max_samples=65536.0)
    p.add('empty-composite', 'histogram.query')
    p.add('invalid-before', 'canvas.state')
    if negative:
        p.add('no-id', 'histogram.query', error='histogram_source', source='layer')
        p.add('wrong-scope', 'histogram.query', error='histogram_source', layer_id=1)
    p.add('invalid-after', 'canvas.state')
    p.add('workflow', 'workflow.set', graph=dict(version=1, nodes=[dict(
        id=1, type='action.histogram.query', position=[0, 0],
        parameters=dict(source='layer', layer_id=1, max_samples=4))], edges=[]))
    p.add('dry', 'workflow.validate')
    p.add('run', 'workflow.run', atomic=True)
    p.add('verify', 'replay.verify')
    return p


def validate(values, plan):
    at = lambda key: plan.at(values, key)
    nodes = {n['id']: n for n in at('nodes')['nodes']}
    assert nodes['action.histogram.query']['read_only']
    assert at('before') == at('after')
    assert at('invalid-before') == at('invalid-after')
    histograms = {}
    for key, count, exact in [('raw', 16, True), ('sampled', 4, False), ('locked-raw', 16, True)]:
        actual = at(key)
        assert actual['exact'] is exact and actual['total_pixels'] == 16
        assert actual['sampled_pixels'] == actual['nontransparent_pixels'] == count
        assert actual['ignored_transparent_pixels'] == 0
        for channel, value in [('red', 32), ('green', 64), ('blue', 96)]:
            assert actual[channel] == [count if i == value else 0 for i in range(256)]
        assert actual['rgb'] == [count if i in [32, 64, 96] else 0 for i in range(256)]
        # Document identities are session-local, while counts and sample grids
        # are deterministic across transports.
        histograms[key] = {k: v for k, v in actual.items() if k not in ['document_identity', 'revision']}
    empty = at('empty-composite')
    assert empty['nontransparent_pixels'] == 0 and empty['ignored_transparent_pixels'] == 16
    assert all(not any(empty[k]) for k in ['red', 'green', 'blue', 'rgb'])
    assert at('dry')['ok'] and at('run')['ok'] and at('verify')['valid']
    return histograms


def negative_jsonl(cli, flags, output):
    # Batch execution intentionally stops at its first rejection. Exercise each
    # negative separately; MCP below also checks state after both rejections.
    for name, args in [('no-id', dict(source='layer')), ('wrong-scope', dict(layer_id=1))]:
        rows = [dict(action='canvas.new', args=dict(width=4, height=4)),
                dict(action='layer.fill_color', args=dict(color='#204060')),
                dict(action='canvas.state', args={}),
                dict(action='histogram.query', args=args),
                dict(action='canvas.state', args={})]
        path = output/(name+'.jsonl')
        path.write_text('\n'.join(json.dumps(row) for row in rows)+'\n', encoding='utf-8')
        run = subprocess.run([str(cli), 'run', str(path), *flags], capture_output=True,
                             text=True, encoding='utf-8', timeout=60)
        path.with_suffix('.responses.jsonl').write_text(run.stdout, encoding='utf-8')
        replies = [json.loads(line) for line in run.stdout.splitlines()]
        assert run.returncode != 0 and 'batch_failed' in run.stderr, (name, run.stderr)
        assert len(replies) == 4 and all(row['ok'] for row in replies[:3]), (name, replies)
        assert replies[3]['ok'] is False and replies[3]['error']['code'] == 'histogram_source', (name, replies[3])
        assert replies[3]['revision'] == replies[2]['data']['result']['revision'], (name, replies)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli', default='target/build/x86_64-pc-windows-gnu/release/micopi-cli.exe')
    args = parser.parse_args()
    cli = (ROOT/args.cli).resolve()
    assert cli.is_file(), 'Build/stage matching native plugins first'
    output = ROOT/'target/test-output/histogram-smoke'/str(time.time_ns())
    output.mkdir(parents=True)
    plugins, profile = output/'plugins', output/'profile'
    shutil.copytree(ROOT/'plugins', plugins)
    profile.mkdir()
    flags = ['--plugins', str(plugins), '--config-dir', str(profile)]
    plan = commands()
    batch = validate(geometry.run_jsonl(cli, flags, plan, output/'histogram.jsonl'), plan)
    negative_plan = commands(negative=True)
    mcp = validate(editing.run_mcp(cli, flags, negative_plan, require_masks=False), negative_plan)
    negative_jsonl(cli, flags, output)
    assert batch == mcp
    print('Histogram: independent exact/sampled bins, locked raw scope, read-only state and JSONL/MCP/node contracts passed.')
    print(output)


if __name__ == '__main__':
    main()
