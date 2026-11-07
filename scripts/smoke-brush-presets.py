"""Verify user preset data, real painting and DLL-free replay in tiny profiles.

JSONL/MCP use separate profiles and independent u16/dab references on 7x5 frames.
This script starts no GUI and makes no physical tablet/performance claim.
"""
import argparse
import base64
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import struct
import time

from png_checks import pixels

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('presets_geometry', ROOT/'scripts/smoke-mask-geometry.py')
geometry = importlib.util.module_from_spec(spec)
spec.loader.exec_module(geometry)
editing, foundation, Plan = geometry.editing, geometry.foundation, geometry.Plan
WIDTH, HEIGHT, AREA = 7, 5, 35
COLOR = 0x194f93
PARAMETERS = dict(radius=20, opacity=.4, hardness=1, stabilizer=0, pressure_size=False,
    pressure_opacity=False, pressure_gamma=1, minimum_radius=.08, spacing=.01,
    minimum_opacity=0, flow=.25, opacity_mode='stroke')


def record(name, dab=False):
    parameters = copy.deepcopy(PARAMETERS)
    if dab:
        parameters.update(flow=1, opacity_mode='dab')
    return dict(name=name, owner='painting', tool_id='brush', parameters=parameters)


def reference(dab):
    if dab:
        alpha = 0
        for _ in range(3):
            alpha = 102+(alpha*153+127)//255
    else:
        influence = 0
        for _ in range(3):
            influence += ((26214-influence)*16384+32767)//65535
        alpha = (influence*255+32767)//65535
    return [(alpha << 24) | COLOR]*AREA


def rgba(words):
    return bytes((word >> shift) & 255 for word in words for shift in [16, 8, 0, 24])


def commands(output, prefix):
    plan = Plan()
    add = plan.add
    add('nodes', 'node.catalog')
    add('registry-before', 'context.read', resources=['plugins.registry'])
    add('initial', 'brush.presets.query')
    add('glaze', 'brush.presets.upsert', **record('Glaze'))
    add('dab', 'brush.presets.upsert', **record('Build Up', True))
    add('catalog', 'brush.presets.query')
    bundle = output/f'{prefix}-bundle.json'
    add('export', 'brush.presets.export', path=str(bundle), preset_ids=[2, 1])
    add('import', 'brush.presets.import', path=str(bundle))
    add('imported', 'brush.presets.query')
    add('delete', 'brush.presets.delete', preset_id=4)
    add('fresh', 'brush.presets.upsert', **record('Fresh after delete'))
    add('reload', 'brush.presets.reload')
    add('registry-after', 'context.read', resources=['plugins.registry'])
    add('resource', 'context.read', resources=['painting.presets'])
    for name, identity in [('glaze', 1), ('build-up', 2)]:
        add(name+'-new', 'canvas.new', width=WIDTH, height=HEIGHT)
        add(name+'-swatches', 'brush.set', color='#194F93', background_color='#EEDDCC', radius=2)
        add(name+'-apply', 'brush.presets.apply', preset_id=identity)
        add(name+'-applied', 'canvas.state')
        add(name+'-query', 'brush.presets.query')
        add(name+'-stroke', 'canvas.stroke', points=[dict(x=x, y=2, pressure=.5, pen=True, time=t)
            for x, t in [(2, 100), (2.5, 120), (2, 140)]])
        add(name+'-png', 'canvas.snapshot')
        add(name+'-save', 'document.save', path=str(output/f'{prefix}-{name}.mcp'))
        add(name+'-undo', 'history.undo')
        add(name+'-blank', 'canvas.snapshot')
        add(name+'-redo', 'history.redo')
        add(name+'-redone', 'canvas.snapshot')
        add(name+'-verify', 'replay.verify')
        add(name+'-seek', 'replay.seek')
        add(name+'-archive', 'replay.export', path=str(output/f'{prefix}-{name}.mcreplay'))
    return plan


def validate(values, plan, output, prefix, profile):
    at = lambda key: plan.at(values, key)
    assert at('initial')['version'] == 1 and not at('initial')['user']
    assert at('glaze')['preset_id'] == 1 and at('dab')['preset_id'] == 2
    assert at('import')['imported_ids'] == [3, 4]
    assert at('import')['id_map'] == [dict(source_id=2, preset_id=3), dict(source_id=1, preset_id=4)]
    assert len(at('imported')['user']) == 4 and at('fresh')['preset_id'] == 5
    exported = (output/f'{prefix}-bundle.json').read_bytes()
    assert at('export')['sha256'] == hashlib.sha256(exported).hexdigest() and at('export')['bytes'] == len(exported)
    bundle = json.loads(exported)
    assert bundle['format'] == 'micopi.brush-presets' and bundle['version'] == 1
    assert [p['id'] for p in bundle['items']] == [2, 1]
    for item in bundle['items']:
        assert len(item['parameters']) == 12 and 'color' not in item['parameters'] and 'background_color' not in item['parameters']
    before = at('registry-before')['resources']['plugins.registry']['value']
    after = at('registry-after')['resources']['plugins.registry']['value']
    assert before['tools'] == after['tools'] and before['contribution_owners'] == after['contribution_owners']
    nodes = {node['id']: node for node in at('nodes')['nodes']}
    assert nodes['action.brush.presets.query']['read_only'] and not nodes['action.brush.presets.apply']['read_only']
    assert 'state.painting.presets' in nodes
    assert at('resource')['resources']['painting.presets']['value']['user']
    settings = json.loads((profile/'settings.json').read_text(encoding='utf-8'))
    assert settings['custom'] == dict(keep=42) and settings['brush_presets']['version'] == 1
    assert [p['id'] for p in settings['brush_presets']['items']] == [1, 2, 3, 5]
    outcomes = {}
    for name, dab in [('glaze', False), ('build-up', True)]:
        state = at(name+'-applied')
        assert state['selected_tool'] == 'brush' and state['parameters']['color'] == '#194F93'
        assert state['parameters']['background_color'] == '#EEDDCC' and state['parameters']['radius'] == 20
        assert state['parameters']['flow'] == (1 if dab else .25)
        assert state['parameters']['opacity_mode'] == ('dab' if dab else 'stroke')
        assert at(name+'-apply')['parameters'] == state['parameters']
        assert (2 if dab else 1) in at(name+'-query')['matching_preset_ids']
        words = reference(dab)
        png = base64.b64decode(at(name+'-png')['data'])
        assert pixels(png) == (WIDTH, HEIGHT, rgba(words)), name
        assert pixels(base64.b64decode(at(name+'-blank')['data'])) == (WIDTH, HEIGHT, bytes(AREA*4))
        assert pixels(base64.b64decode(at(name+'-redone')['data'])) == pixels(png)
        saved = foundation.read_project(output/f'{prefix}-{name}.mcp')
        assert len(saved['rasters']) == 1 and saved['rasters'][0][1] == struct.pack('<'+'I'*AREA, *words)
        assert not saved['planes'] and at(name+'-verify')['valid']
        outcomes[name] = dict(png=png, seek=at(name+'-seek'))
    return outcomes


def rejection_and_node_checks(cli, flags, output):
    plan = Plan()
    add = plan.add
    add('new', 'canvas.new', width=WIDTH, height=HEIGHT)
    add('parameters', 'brush.set', radius=1, hardness=1, opacity=1, flow=1, opacity_mode='dab',
        stabilizer=0, pressure_size=False, spacing=.11)
    add('down', 'canvas.pointer', phase='down', x=2, y=2, pen=True, pressure=.3, time=100)
    add('move', 'canvas.pointer', phase='move', x=2.1, y=2, pen=True, pressure=.6, time=120)
    add('before', 'canvas.state')
    add('catalog-before', 'brush.presets.query')
    add('upsert-live', 'brush.presets.upsert', error='stroke_active', **record('Rejected live write'))
    add('delete-live', 'brush.presets.delete', error='stroke_active', preset_id=1)
    add('import-live', 'brush.presets.import', error='stroke_active', path=str(output/'mcp-bundle.json'))
    add('export-live', 'brush.presets.export', error='stroke_active', path=str(output/'live-export.json'), preset_ids=[1])
    add('after', 'canvas.state')
    add('catalog-after', 'brush.presets.query')
    add('move-again', 'canvas.pointer', phase='move', x=2.2, y=2, pen=True, pressure=.8, time=140)
    add('up', 'canvas.pointer', phase='up', x=2.2, y=2, pen=True, pressure=.2, time=160)
    add('events', 'replay.events', limit=256)
    graph = dict(version=1, nodes=[
        dict(id=1, type='action.brush.presets.upsert', position=[0, 0], parameters=record('Workflow preset')),
        dict(id=2, type='action.brush.presets.apply', position=[350, 0])],
        edges=[dict(**{'from': 1}, output='preset_id', to=2, input='preset_id')])
    add('dry', 'workflow.run', graph=graph, atomic=False, dry_run=True)
    add('after-dry', 'brush.presets.query')
    add('workflow', 'workflow.run', graph=graph, atomic=False)
    add('workflow-query', 'brush.presets.query')
    add('atomic-file', 'pipeline.run', error='non_atomic_action', atomic=True,
        steps=[dict(id='file', action='brush.presets.upsert', args=record('Non-atomic file'))])
    add('verify', 'replay.verify')
    values = editing.run_mcp(cli, flags, plan)
    at = lambda key: plan.at(values, key)
    assert editing.document_state(at('before')) == editing.document_state(at('after'))
    assert at('catalog-before') == at('catalog-after')
    samples = [p for e in at('events')['events'] for p in e['source'].get('samples', [])]
    assert [p['time'] for p in samples] == [100, 120, 140, 160]
    assert [p['phase'] for p in samples] == ['down', 'move', 'move', 'up']
    assert at('dry')['ok'] and len(at('after-dry')['user']) == 4
    assert at('workflow')['ok'] and len(at('workflow-query')['user']) == 5
    assert at('workflow-query')['user'][-1]['preset']['name'] == 'Workflow preset'
    assert at('verify')['valid'] and not (output/'live-export.json').exists()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli', default='target/build/x86_64-pc-windows-gnu/release/micopi-cli.exe')
    args = parser.parse_args()
    cli = (ROOT/args.cli).resolve()
    output = ROOT/'target/test-output/brush-presets-smoke'/str(time.time_ns())
    output.mkdir(parents=True)
    plugins = output/'plugins'
    shutil.copytree(ROOT/'plugins', plugins)
    expected = None
    profiles = {}
    for prefix in ['jsonl', 'mcp']:
        profile = output/(prefix+'-profile')
        profile.mkdir()
        (profile/'settings.json').write_text(json.dumps(dict(version=1, custom=dict(keep=42))), encoding='utf-8')
        profiles[prefix] = profile
        flags = ['--plugins', str(plugins), '--config-dir', str(profile)]
        plan = commands(output, prefix)
        values = geometry.run_jsonl(cli, flags, plan, output/'presets.jsonl') if prefix == 'jsonl' else editing.run_mcp(cli, flags, plan)
        actual = validate(values, plan, output, prefix, profile)
        if expected is not None:
            assert {k: pixels(v['png']) for k, v in actual.items()} == {k: pixels(v['png']) for k, v in expected.items()}
        expected = actual
        print(f'{prefix.upper()}: preset data and two tiny painting cases passed independent references.', flush=True)
    rejection_and_node_checks(cli, ['--plugins', str(plugins), '--config-dir', str(profiles['mcp'])], output)
    spec = importlib.util.spec_from_file_location('presets_owner_replay', ROOT/'scripts/smoke-mask-owners.py')
    owners = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(owners)
    # An empty profile proves replay does not require the original user library.
    empty = output/'empty-profile'
    empty.mkdir()
    owners.replay_without_dlls(cli, empty, output, [dict(name=n) for n in expected], expected)
    print('Versioned preset data, real painting, shared admission and DLL-free outcomes passed.')
    print(output)


if __name__ == '__main__':
    main()
