"""Verify bounded raster alignment/distribution through JSONL, MCP and replay.

Seven 10x8 cases use independent raw-alpha bounds, rational layout, coordinate
translations and coverage/composite references. No GUI or local stress runs.
"""
import argparse
import base64
import copy
from fractions import Fraction
import importlib.util
import json
from pathlib import Path
import shutil
import struct
import time

from png_checks import pixels

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('arrangement_geometry', ROOT/'scripts/smoke-mask-geometry.py')
geometry = importlib.util.module_from_spec(spec)
spec.loader.exec_module(geometry)
editing, foundation, Plan = geometry.editing, geometry.foundation, geometry.Plan
WIDTH, HEIGHT, AREA = 10, 8, 80
BOXES = {1: [1, 1, 3, 3], 2: [5, 2, 8, 5], 3: [2, 5, 4, 6], 4: [0, 0, 1, 1]}


def rounded(value):
    sign = -1 if value < 0 else 1
    value = abs(value)
    return sign*((2*value.numerator+value.denominator)//(2*value.denominator))


def layout(case):
    mode, ids = case['args']['mode'], case['args']['layer_ids']
    ids = [int(i) for i in ids]
    reference = ([0, 0, WIDTH, HEIGHT] if case['args'].get('reference') == 'canvas' else
                 [min(BOXES[i][0] for i in ids), min(BOXES[i][1] for i in ids),
                  max(BOXES[i][2] for i in ids), max(BOXES[i][3] for i in ids)])
    vertical = mode in ['top', 'bottom', 'vertical_center', 'distribute_vertical_centers', 'distribute_vertical_gaps']
    axis = int(vertical)
    center = lambda identity: Fraction(BOXES[identity][axis]+BOXES[identity][axis+2], 2)
    gaps = mode.endswith('_gaps')
    order = sorted(ids, key=lambda identity: (BOXES[identity][axis] if gaps else center(identity), identity))
    result = {}
    for identity in ids:
        if gaps:
            sizes = {i: BOXES[i][axis+2]-BOXES[i][axis] for i in ids}
            gap = Fraction(BOXES[order[-1]][axis+2]-BOXES[order[0]][axis]-sum(sizes.values()),len(ids)-1)
            rank = order.index(identity)
            target = BOXES[order[0]][axis]+sum(sizes[i] for i in order[:rank])+gap*rank
            delta = rounded(target-BOXES[identity][axis])
        elif mode.startswith('distribute_'):
            target = center(order[0])+(center(order[-1])-center(order[0]))*Fraction(order.index(identity), len(order)-1)
            delta = rounded(target-center(identity))
        elif mode in ['left', 'top']:
            delta = reference[axis]-BOXES[identity][axis]
        elif mode in ['right', 'bottom']:
            delta = reference[axis+2]-BOXES[identity][axis+2]
        else:
            delta = rounded(Fraction(reference[axis]+reference[axis+2], 2)-center(identity))
        result[identity] = (0, delta) if vertical else (delta, 0)
    return reference, result


def shifted(source, dx, dy, fill):
    return [source[(y-dy)*WIDTH+x-dx] if 0 <= x-dx < WIDTH and 0 <= y-dy < HEIGHT else fill
            for y in range(HEIGHT) for x in range(WIDTH)]


def fixture(seed, path):
    comp = copy.deepcopy(seed['composition'])
    comp['layers'][1][1] = 137
    comp['selected_layer_ids'] = [1, 3, 4]
    for row in comp['layer_metadata']:
        if row['id'] in [1, 4]:
            row['links'] = dict(version=1, layer_ids=[1, 4])
    comp['groups'] = dict(version=2, roots=[dict(kind='layer', id=1), dict(kind='group', id=1),
        dict(kind='layer', id=3), dict(kind='layer', id=4)], groups=[dict(id=1, name='Ink scope', visible=True,
        opacity=193, blend_mode='normal', compositing='pass_through', children=[dict(kind='layer', id=2)])])
    comp['next_group_id'] = 2
    comp['raster_masks'] = dict(version=1, next_id=5, target=dict(kind='mask', id=4), items=[
        dict(id=1, owner=dict(kind='layer', id=1), density=173, feather_radius_px=1),
        dict(id=2, owner=dict(kind='layer', id=2), enabled=False),
        dict(id=3, owner=dict(kind='layer', id=3), linked=False),
        dict(id=4, owner=dict(kind='group', id=1))])
    comp['selection'] = dict(current=geometry.selection_wire([128 if i % 3 == 0 else 0 for i in range(AREA)], WIDTH, HEIGHT),
        previous=geometry.selection_wire([79]*AREA, WIDTH, HEIGHT))
    rasters = []
    for identity in range(1, 5):
        left, top, right, bottom = BOXES[identity]
        rasters.append([0x80335577+(identity-1)*0x10101 if left <= x < right and top <= y < bottom else 0x00123456
                        for y in range(HEIGHT) for x in range(WIDTH)])
    masks = {1: bytes((i*37+11) % 256 for i in range(AREA)), 2: bytes([91]),
             3: bytes((i*37+33) % 256 for i in range(AREA)), 4: bytes([0])}
    metadata = json.dumps(comp, separators=(',', ':')).encode()
    data = bytearray(b'MICOPI04'+struct.pack('<IIII', WIDTH, HEIGHT, 4, 3))
    for properties, raster in zip(comp['layers'], rasters):
        data += bytes([int(properties[0]), properties[1]])+struct.pack('<'+'I'*AREA, *raster)
    data += struct.pack('<I', len(metadata))+metadata+struct.pack('<I', len(masks))
    for identity, plane in masks.items():
        data += struct.pack('<QBI', identity, int(len(plane) != 1), len(plane))+plane
    # Nondefault raw-mask metadata permits an empty v4 archive slot.
    path.write_bytes(data+struct.pack('<Q', 0))
    return dict(composition=comp, rasters=rasters, masks={i: list(p)*AREA if len(p) == 1 else list(p) for i, p in masks.items()})


def expected(model, offsets):
    result = copy.deepcopy(model)
    for identity, (dx, dy) in offsets.items():
        result['rasters'][identity-1] = shifted(model['rasters'][identity-1], dx, dy, 0)
        if identity in [1, 2]:
            result['masks'][identity] = shifted(model['masks'][identity], dx, dy, 255)
    return result


def surface(model):
    first = editing.feather_density(model['masks'][1], WIDTH, HEIGHT, 1, 173)
    third = model['masks'][3]
    result = []
    for i in range(AREA):
        # Group mask0 bypasses its Pass Through subtree (including adjustment),
        # independently proving that composed/effective bounds are unsuitable.
        p = editing.over(0, editing.attenuate(model['rasters'][0][i], first[i]))
        p = editing.over(p, editing.attenuate(model['rasters'][2][i], third[i]))
        p = editing.over(p, model['rasters'][3][i])
        result.append(p)
    return WIDTH, HEIGHT, b''.join(editing.rgba(p) for p in result)


def commands(output, prefix, path, cases):
    plan = Plan()
    plan.add('nodes', 'node.catalog')
    for case in cases:
        key = case['name']
        plan.add(key+'-open', 'document.open', path=str(path))
        plan.add(key+'-adjustment', 'adjustment.create', node_type='compositing.filter',
                 parameters=dict(filter='invert', amount=.25), parent_id=1)
        plan.add(key+'-before', 'canvas.state')
        plan.add(key+'-before-png', 'canvas.snapshot')
        plan.add(key+'-plan', 'layer.arrange.plan', **case['args'])
        plan.add(key+'-after-plan', 'canvas.state')
        plan.add(key+'-apply', 'layer.arrange', **case['args'])
        plan.add(key+'-after', 'canvas.state')
        plan.add(key+'-events', 'replay.events', limit=256)
        plan.add(key+'-png', 'canvas.snapshot')
        plan.add(key+'-save', 'document.save', path=str(output/f'{prefix}-{key}.mcp'))
        if any(delta != (0, 0) for delta in layout(case)[1].values()):
            plan.add(key+'-undo', 'history.undo')
            plan.add(key+'-undone-png', 'canvas.snapshot')
            plan.add(key+'-redo', 'history.redo')
            plan.add(key+'-redone-png', 'canvas.snapshot')
        plan.add(key+'-verify', 'replay.verify')
        plan.add(key+'-seek', 'replay.seek')
        plan.add(key+'-archive', 'replay.export', path=str(output/f'{prefix}-{key}.mcreplay'))
    return plan


def validate(values, plan, output, prefix, model, cases):
    at = lambda key: plan.at(values, key)
    nodes = {n['id']: n for n in at('nodes')['nodes']}
    assert nodes['action.layer.arrange.plan']['read_only'] and not nodes['action.layer.arrange']['read_only']
    assert any(p['id'] == 'layer_ids' and p['required'] for p in nodes['action.layer.arrange.plan']['outputs'])
    result = {}
    for case in cases:
        key = case['name']
        reference, offsets = layout(case)
        prepared = at(key+'-plan')
        assert prepared['reference_bounds'] == reference and prepared['layer_ids'] == list(offsets)
        assert prepared['linked_mask_ids'] == [i for i in offsets if i in [1, 2]]
        assert prepared['staged_bytes'] == AREA*(4*len(offsets)+len(prepared['linked_mask_ids']))
        assert prepared['staging_limit_bytes'] == 192*1024*1024 and prepared['can_apply'] and not prepared['contact_active']
        for row in prepared['targets']:
            identity = row['layer_id']
            assert row['bounds'] == BOXES[identity] and (row['dx'], row['dy']) == offsets[identity]
        assert editing.document_state(at(key+'-before')) == editing.document_state(at(key+'-after-plan'))
        before, after = at(key+'-before'), at(key+'-after')
        assert after['revision'] == before['revision']+1 == at(key+'-apply')['revision']
        for field in ['layers', 'groups', 'raster_masks', 'selected_layer_ids', 'active_layer', 'selection', 'last_transform']:
            assert before[field] == after[field], (key, field)
        changed = expected(model, offsets)
        png = base64.b64decode(at(key+'-png')['data'])
        assert pixels(png) == surface(changed), key
        assert pixels(base64.b64decode(at(key+'-before-png')['data'])) == surface(model), key
        if any(delta != (0, 0) for delta in offsets.values()):
            assert pixels(base64.b64decode(at(key+'-undone-png')['data'])) == surface(model)
            assert pixels(base64.b64decode(at(key+'-redone-png')['data'])) == pixels(png)
        else:
            assert not at(key+'-apply')['changed']
        event = next(e for e in reversed(at(key+'-events')['events']) if e['source'].get('action') == 'layer.arrange')
        assert event['source']['context']['arrangement'] == {k: prepared[k] for k in ['mode', 'reference', 'reference_bounds', 'targets']}
        saved = foundation.read_project(output/f'{prefix}-{key}.mcp')
        assert (saved['width'], saved['height'], saved['active']) == (WIDTH, HEIGHT, 3)
        assert len(saved['rasters']) == 4 and saved['composition']['groups']['version'] == 3
        assert len(saved['composition']['groups']['adjustments']) == 1
        assert saved['composition']['layer_metadata'] == model['composition']['layer_metadata']
        for (_, raster), reference_pixels in zip(saved['rasters'], changed['rasters']):
            assert raster == struct.pack('<'+'I'*AREA, *reference_pixels), key
        for identity, samples in changed['masks'].items():
            plane = saved['planes'][identity]
            assert (plane*AREA if len(plane) == 1 else plane) == bytes(samples), (key, identity)
        assert at(key+'-verify')['valid']
        result[key] = dict(png=png, seek=at(key+'-seek'))
    return result


def extra_checks(cli, flags, path):
    plan = Plan()
    plan.add('open', 'document.open', path=str(path))
    plan.add('before', 'canvas.state')
    for key, args in [('singleton', dict(layer_ids=[1], mode='left')),
                      ('duplicate', dict(layer_ids=[1, 1.0], mode='left')),
                      ('canvas-distribution', dict(layer_ids=[1, 2, 3], mode='distribute_horizontal_centers', reference='canvas'))]:
        plan.add(key, 'layer.arrange', error='invalid_arguments', **args)
    plan.failed_pipeline('atomic', atomic=True, steps=[
        dict(id='arrange', action='layer.arrange', args=dict(layer_ids=[3, 1, 2], mode='right')),
        dict(id='fail', action='layer.select', args=dict(index=15))])
    plan.add('after', 'canvas.state')
    graph = dict(version=1, nodes=[
        dict(id=1, type='action.layer.arrange.plan', position=[0, 0], parameters=dict(layer_ids=[3, 1, 2], mode='left')),
        dict(id=2, type='action.layer.arrange', position=[350, 0], parameters=dict(mode='left'))],
        edges=[dict(**{'from': 1}, output='layer_ids', to=2, input='layer_ids')])
    plan.add('dry', 'workflow.run', graph=graph, atomic=True, dry_run=True)
    plan.add('after-dry', 'canvas.state')
    plan.add('run', 'workflow.run', graph=graph, atomic=True)
    plan.add('verify', 'replay.verify')
    values = editing.run_mcp(cli, flags, plan)
    at = lambda key: plan.at(values, key)
    assert editing.document_state(at('before')) == editing.document_state(at('after')) == editing.document_state(at('after-dry'))
    assert at('atomic')['rolled_back'] and at('atomic')['completed'] == ['arrange']
    assert at('dry')['ok'] and at('run')['ok'] and at('verify')['valid']


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli', default='target/build/x86_64-pc-windows-gnu/release/micopi-cli.exe')
    args = parser.parse_args()
    cli = (ROOT/args.cli).resolve()
    output = ROOT/'target/test-output/layer-arrangement-smoke'/str(time.time_ns())
    output.mkdir(parents=True)
    plugins, profile = output/'plugins', output/'profile'
    shutil.copytree(ROOT/'plugins', plugins)
    profile.mkdir()
    flags = ['--plugins', str(plugins), '--config-dir', str(profile)]
    bootstrap = Plan()
    bootstrap.add('new', 'canvas.new', width=WIDTH, height=HEIGHT)
    for i in range(3):
        bootstrap.add(f'layer-{i}', 'layer.add')
    bootstrap.add('save', 'document.save', path=str(output/'seed.mcp'))
    geometry.run_jsonl(cli, flags, bootstrap, output/'bootstrap.jsonl')
    path = output/'source.mcp'
    model = fixture(foundation.read_project(output/'seed.mcp'), path)
    cases = [dict(name=mode, args=dict(layer_ids=[3, 1, 2], mode=mode)) for mode in
             ['left', 'right', 'distribute_horizontal_centers', 'distribute_vertical_centers', 'distribute_horizontal_gaps', 'distribute_vertical_gaps']]
    cases.append(dict(name='canvas-center', args=dict(layer_ids=[2.0, 1], mode='horizontal_center', reference='canvas')))
    expected_outcomes = None
    for prefix, runner in [('jsonl', lambda p: geometry.run_jsonl(cli, flags, p, output/'arrangement.jsonl')),
                           ('mcp', lambda p: editing.run_mcp(cli, flags, p))]:
        plan = commands(output, prefix, path, cases)
        actual = validate(runner(plan), plan, output, prefix, model, cases)
        if expected_outcomes is not None:
            assert {k: pixels(v['png']) for k, v in actual.items()} == {k: pixels(v['png']) for k, v in expected_outcomes.items()}
        expected_outcomes = actual
        print(f'{prefix.upper()}: {len(cases)} tiny arrangement cases passed independent references.', flush=True)
    extra_checks(cli, flags, path)
    # Reuse the existing DLL-free archive verifier, with this script's cases.
    spec = importlib.util.spec_from_file_location('arrangement_owner_replay', ROOT/'scripts/smoke-mask-owners.py')
    owners = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(owners)
    owners.replay_without_dlls(cli, profile, output, cases, expected_outcomes)
    print('Exact layer arrangement, owned masks, typed bindings and verified outcomes passed.')
    print(output)


if __name__ == '__main__':
    main()
