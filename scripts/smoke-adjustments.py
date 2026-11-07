"""Check true affect-below adjustments with independent tiny transport references.

Six 5x3 cases verify alpha-preserving Mix, Levels, stack/group scope, real selection A8,
metadata-only source preservation, Undo, project persistence and DLL-free replay.
No GUI, benchmark workload or physical tablet claim is involved.
"""
import argparse
import base64
import copy
import importlib.util
import json
import math
from pathlib import Path
import shutil
import struct
import time

from png_checks import pixels

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('adjustments_mask_geometry', ROOT/'scripts/smoke-mask-geometry.py')
geometry = importlib.util.module_from_spec(spec)
spec.loader.exec_module(geometry)
editing, foundation, Plan = geometry.editing, geometry.foundation, geometry.Plan
WIDTH, HEIGHT, AREA = 5, 3, 15
FILTER = dict(node_type='compositing.filter', parameters=dict(filter='invert', amount=1))
LEVELS = dict(node_type='compositing.levels', parameters=dict(parameters=dict(
    input_black=0, input_white=255, gamma=2, output_black=0, output_white=255)))


def invert(value):
    return value ^ 0xffffff if value >> 24 else value


def mixed(before, after, opacity, coverage=255):
    amount = (opacity*coverage+127)//255
    if amount in [0, 255]:
        return after if amount else before
    weights = [(before >> 24)*(255-amount), (after >> 24)*amount]
    total = sum(weights)
    alpha = (total+127)//255
    if not alpha:
        return 0
    result = alpha << 24
    for shift in [0, 8, 16]:
        channels = [(before >> shift) & 255, (after >> shift) & 255]
        result |= (sum(c*w for c, w in zip(channels, weights))+total//2)//total << shift
    return result


def rgba(values):
    return b''.join(bytes((value >> shift) & 255 for shift in [16, 8, 0, 24]) for value in values)


def write_fixture(path, model):
    # A v2 metadata-only fixture has no recording suffix. v3 requires an archive.
    metadata = json.dumps(model['composition'], separators=(',', ':')).encode()
    data = bytearray(b'MICOPI02'+struct.pack('<IIII', WIDTH, HEIGHT, 2, model['active']))
    for properties, raster in zip(model['composition']['layers'], model['rasters']):
        data += bytes([int(properties[0]), properties[1]])+struct.pack('<'+'I'*AREA, *raster)
    path.write_bytes(data+struct.pack('<I', len(metadata))+metadata)


def scenarios(seed, output):
    result = []
    for name in ['middle-root', 'top-aggregate', 'isolated', 'pass-through', 'selection-snapshot', 'levels-master']:
        model = dict(active=0, composition=copy.deepcopy(seed['composition']), rasters=[
            [(a << 24) | 0x402010 for _ in range(HEIGHT) for a in [0, 1, 73, 128, 255]],
            [0xff1122ee if i % 2 == 0 else 0x001122ee for i in range(AREA)]])
        # A single active layer uses empty stored multi-selection metadata.
        model['composition']['selected_layer_ids'] = []
        selected = [0, 64, 128, 255, 0]*HEIGHT
        if name in ['isolated', 'pass-through', 'selection-snapshot']:
            model['rasters'][1] = [0x001122ee]*AREA
        if name == 'selection-snapshot':
            model['composition']['selection'] = dict(
                current=geometry.selection_wire(selected, WIDTH, HEIGHT),
                previous=geometry.selection_wire(selected[::-1], WIDTH, HEIGHT))
        path = output/(name+'.mcp')
        write_fixture(path, model)
        result.append(dict(name=name, model=model, path=path, selection=selected))
    return result


def reference(case, final=False):
    # Transparent source RGB is stored, but the stack begins over transparency.
    lower = [p if p >> 24 else 0 for p in case['model']['rasters'][0]]
    upper = case['model']['rasters'][1]
    name = case['name']
    if name == 'middle-root':
        return [editing.over(mixed(a, invert(a), 128), b) for a, b in zip(lower, upper)]
    if name == 'top-aggregate':
        return [invert(editing.over(a, b)) for a, b in zip(lower, upper)]
    if name == 'levels-master':
        def curve(pixel):
            if not pixel >> 24:
                return pixel
            result = pixel & 0xff000000
            for shift in [0, 8, 16]:
                result |= math.floor(math.sqrt(((pixel >> shift) & 255)*255)+0.5) << shift
            return result
        return [curve(editing.over(a, b)) for a, b in zip(lower, upper)]
    if name == 'isolated':
        return lower
    if name == 'pass-through':
        return [mixed(a, invert(a), 137) for a in lower]
    coverage = case['selection']
    if final:
        coverage = editing.feather_density(coverage, WIDTH, HEIGHT, 1, 173)
    return [mixed(a, invert(a), 128, m) for a, m in zip(lower, coverage)]


def commands(output, prefix, cases):
    plan = Plan()
    add = plan.add
    add('nodes', 'node.catalog')
    add('catalog', 'adjustment.catalog')
    for case in cases:
        key = case['name']
        add(key+'-open', 'document.open', path=str(case['path']))
        if key in ['isolated', 'pass-through']:
            add(key+'-group', 'group.create', members=[dict(kind='layer', id=2)])
            add(key+'-group-policy', 'group.set', group_id=1,
                compositing='pass_through' if key == 'pass-through' else 'isolated', opacity=137/255)
        add(key+'-before', 'canvas.state')
        add(key+'-before-png', 'canvas.snapshot')
        properties = copy.deepcopy(LEVELS if key == 'levels-master' else FILTER)
        properties.update(name='Ink tone')
        if key in ['isolated', 'pass-through']:
            properties.update(parent_id=1, index=1)
        else:
            properties['index'] = 2 if key in ['top-aggregate', 'levels-master'] else 1
        if key in ['middle-root', 'selection-snapshot']:
            properties['opacity'] = 128/255
        if key == 'selection-snapshot':
            properties['mask_source'] = 'selection'
        add(key+'-create', 'adjustment.create', **properties)
        add(key+'-get', 'adjustment.get', adjustment_id=1)
        add(key+'-query', 'adjustment.query')
        add(key+'-after', 'canvas.state')
        add(key+'-created-png', 'canvas.snapshot')
        add(key+'-undo', 'history.undo')
        add(key+'-undone', 'canvas.state')
        add(key+'-undone-png', 'canvas.snapshot')
        add(key+'-redo', 'history.redo')
        add(key+'-redone-png', 'canvas.snapshot')
        if key == 'selection-snapshot':
            add(key+'-effects', 'mask.set', mask_id=1, density=173, feather_radius_px=1)
            add(key+'-deselect', 'selection.deselect')
            add(key+'-raw', 'mask.snapshot', mask_id=1, width=WIDTH, height=HEIGHT)
        add(key+'-png', 'canvas.snapshot')
        add(key+'-save', 'document.save', path=str(output/f'{prefix}-{key}.mcp'))
        add(key+'-reopen', 'document.open', path=str(output/f'{prefix}-{key}.mcp'))
        add(key+'-reopened', 'adjustment.get', adjustment_id=1)
        add(key+'-reopened-png', 'canvas.snapshot')
        add(key+'-verify', 'replay.verify')
        add(key+'-seek', 'replay.seek')
        add(key+'-archive', 'replay.export', path=str(output/f'{prefix}-{key}.mcreplay'))
    return plan


def validate(values, plan, output, prefix, cases):
    at = lambda key: plan.at(values, key)
    nodes = {node['id']: node for node in at('nodes')['nodes']}
    for action in ['create', 'set', 'move', 'delete']:
        node = nodes['action.adjustment.'+action]
        assert not node['read_only']
        output_name = 'deleted_adjustment_ids' if action == 'delete' else 'adjustment_id'
        assert any(p['id'] == output_name and p['required'] for p in node['outputs'])
    provider = next(item for item in at('catalog')['items'] if item['node_type'] == 'compositing.filter')
    assert provider['owner'] == 'compositing' and provider['parameters_schema']['additionalProperties'] is False
    outcomes = {}
    for case in cases:
        key = case['name']
        before, after = at(key+'-before'), at(key+'-after')
        created = at(key+'-create')
        assert created['adjustment_id'] == 1 and created['mask_ids'] == ([1] if key == 'selection-snapshot' else [])
        assert after['revision'] == before['revision']+1 == created['revision']
        assert len(after['layers']) == 2 and after['selected_layer_ids'] == before['selected_layer_ids']
        assert after['active_layer'] == before['active_layer'] and after['selection'] == before['selection']
        item = at(key+'-get')
        definition = LEVELS if key == 'levels-master' else FILTER
        assert item['id'] == 1 and item['node_type'] == definition['node_type'] and item['parameters'] == definition['parameters']
        assert item['provider_available'] and at(key+'-query')['items'][0]['id'] == 1
        expected = reference(case)
        assert pixels(base64.b64decode(at(key+'-created-png')['data'])) == (WIDTH, HEIGHT, rgba(expected)), key
        assert pixels(base64.b64decode(at(key+'-redone-png')['data'])) == (WIDTH, HEIGHT, rgba(expected)), key
        assert pixels(base64.b64decode(at(key+'-undone-png')['data'])) == pixels(base64.b64decode(at(key+'-before-png')['data'])), key
        if key == 'selection-snapshot':
            assert at(key+'-raw')['samples'] == case['selection']
        final = reference(case, final=True)
        png = base64.b64decode(at(key+'-png')['data'])
        assert pixels(png) == (WIDTH, HEIGHT, rgba(final)), key
        assert pixels(base64.b64decode(at(key+'-reopened-png')['data'])) == pixels(png), key
        assert at(key+'-reopened')['parameters'] == definition['parameters'] and at(key+'-verify')['valid']
        saved = foundation.read_project(output/f'{prefix}-{key}.mcp')
        assert (saved['width'], saved['height'], saved['active']) == (WIDTH, HEIGHT, 0)
        assert len(saved['rasters']) == 2
        for (_, raw), source in zip(saved['rasters'], case['model']['rasters']):
            assert raw == struct.pack('<'+'I'*AREA, *source), (key, 'source raster changed')
        tree = saved['composition']['groups']
        assert tree['version'] == 3 and len(tree['adjustments']) == 1
        adjustment = tree['adjustments'][0]
        assert adjustment['node_type'] == definition['node_type']
        if key == 'levels-master':
            assert adjustment['levels'] == LEVELS['parameters']['parameters']
            assert adjustment['filter'] == 'invert' and adjustment['amount'] == 0
        else:
            assert adjustment['filter'] == 'invert' and adjustment['amount'] == 1
            assert 'levels' not in adjustment
        assert saved['composition']['layer_metadata'] == case['model']['composition']['layer_metadata']
        if key == 'selection-snapshot':
            assert saved['planes'][1] == bytes(case['selection'])
            masks = saved['composition']['raster_masks']
            assert masks['version'] == 2 and masks['items'][0]['owner'] == dict(kind='adjustment', id=1)
            assert masks['items'][0]['density'] == 173 and masks['items'][0]['feather_radius_px'] == 1
        else:
            assert not saved['planes']
        outcomes[key] = dict(png=png, seek=at(key+'-seek'))
    return outcomes


def negative_and_binding_checks(cli, flags, cases):
    plan = Plan()
    plan.add('open', 'document.open', path=str(cases[0]['path']))
    plan.add('before', 'canvas.state')
    for key, properties in [('provider', dict(node_type='missing.filter', visible=False)),
                            ('domain', dict(node_type='compositing.blend', visible=False))]:
        plan.add(key, 'adjustment.create', error='adjustment', **properties)
    plan.add('extra', 'adjustment.create', error='invalid_arguments', **FILTER, extra=True)
    plan.failed_pipeline('atomic', atomic=True, steps=[
        dict(id='new', action='adjustment.create', args=FILTER),
        dict(id='fail', action='adjustment.delete', args=dict(adjustment_id=999999))])
    plan.add('after', 'canvas.state')
    graph = dict(version=1, nodes=[
        dict(id=1, type='action.adjustment.create', position=[0, 0], parameters=FILTER),
        dict(id=2, type='action.adjustment.set', position=[350, 0], parameters=dict(name='Bound ID'))],
        edges=[dict(**{'from': 1}, output='adjustment_id', to=2, input='adjustment_id')])
    plan.add('dry', 'workflow.run', graph=graph, atomic=True, dry_run=True)
    plan.add('after-dry', 'adjustment.query')
    plan.add('run', 'workflow.run', graph=graph, atomic=True)
    plan.add('query', 'adjustment.query')
    plan.add('inspect', 'adjustment.inspect', error='ui_unavailable', adjustment_id=1)
    plan.add('verify', 'replay.verify')
    values = editing.run_mcp(cli, flags, plan)
    at = lambda key: plan.at(values, key)
    assert editing.document_state(at('before')) == editing.document_state(at('after'))
    assert at('atomic')['rolled_back'] and at('atomic')['completed'] == ['new']
    assert at('dry')['ok'] and not at('after-dry')['items']
    assert at('run')['ok'] and at('query')['items'][0]['name'] == 'Bound ID'
    assert at('query')['items'][0]['id'] == 1 and at('verify')['valid']


def replay_without_dlls(cli, output, profile, cases, outcomes):
    minimal = output/'replay-only-plugins'
    minimal.mkdir()
    for name in ['core', 'replay']:
        shutil.copytree(ROOT/'plugins'/name, minimal/name)
    assert not list(minimal.rglob('*.dll'))
    plan = Plan()
    for case in cases:
        key = case['name']
        plan.add(key+'-load', 'replay.load', path=str(output/f'mcp-{key}.mcreplay'))
        plan.add(key+'-verify', 'replay.verify', source='loaded')
        plan.add(key+'-png', 'replay.snapshot', source='loaded')
        plan.add(key+'-seek', 'replay.seek', source='loaded')
    values = editing.run_mcp(cli, ['--plugins', str(minimal), '--config-dir', str(profile)], plan, require_masks=False)
    for case in cases:
        key = case['name']
        assert plan.at(values, key+'-verify')['valid']
        assert pixels(base64.b64decode(plan.at(values, key+'-png')['data'])) == pixels(outcomes[key]['png'])
        for field in ['state_hash', 'surface_hash']:
            assert plan.at(values, key+'-seek')[field] == outcomes[key]['seek'][field]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli', default='target/build/x86_64-pc-windows-gnu/release/micopi-cli.exe')
    args = parser.parse_args()
    cli = (ROOT/args.cli).resolve()
    assert cli.is_file(), 'Build/stage the matching host and native plugins first'
    output = ROOT/'target/test-output/adjustments-smoke'/str(time.time_ns())
    output.mkdir(parents=True)
    print(output, flush=True)
    plugins, profile = output/'plugins', output/'profile'
    shutil.copytree(ROOT/'plugins', plugins)
    profile.mkdir()
    flags = ['--plugins', str(plugins), '--config-dir', str(profile)]
    bootstrap = Plan()
    bootstrap.add('new', 'canvas.new', width=WIDTH, height=HEIGHT)
    bootstrap.add('add', 'layer.add')
    bootstrap.add('save', 'document.save', path=str(output/'seed.mcp'))
    geometry.run_jsonl(cli, flags, bootstrap, output/'bootstrap.jsonl')
    cases = scenarios(foundation.read_project(output/'seed.mcp'), output)
    expected = None
    for prefix, runner in [('jsonl', lambda p: geometry.run_jsonl(cli, flags, p, output/'adjustments.jsonl')),
                           ('mcp', lambda p: editing.run_mcp(cli, flags, p))]:
        plan = commands(output, prefix, cases)
        actual = validate(runner(plan), plan, output, prefix, cases)
        if expected is not None:
            assert {k: pixels(v['png']) for k, v in actual.items()} == {k: pixels(v['png']) for k, v in expected.items()}
        expected = actual
        print(f'{prefix.upper()}: {len(cases)} tiny cases / {len(plan.rows)} shared commands passed independent references.', flush=True)
    negative_and_binding_checks(cli, flags, cases)
    replay_without_dlls(cli, output, profile, cases, expected)
    (output/'metrics.json').write_text(json.dumps(dict(cases=len(cases), width=WIDTH, height=HEIGHT,
        independent_reference=True, physical_tablet_tested=False, performance_measured=False), indent=2)+'\n', encoding='utf-8')
    print('Adjustment stack, owned A8, typed bindings and DLL-free outcomes passed.', flush=True)
    print(output)


if __name__ == '__main__':
    main()
