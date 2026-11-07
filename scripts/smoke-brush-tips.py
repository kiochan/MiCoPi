"""Verify scalar tip intent, native painting and verified outcome transports.

All references use 7x7 frames. Hard rectangle coverage has an independently
specified byte stencil; quarter turns are exact coordinate permutations.
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
spec = importlib.util.spec_from_file_location('tips_geometry', ROOT/'scripts/smoke-mask-geometry.py')
geometry = importlib.util.module_from_spec(spec)
spec.loader.exec_module(geometry)
editing, foundation, Plan = geometry.editing, geometry.foundation, geometry.Plan
WIDTH, HEIGHT, AREA = 7, 7, 49
COLOR = 0x194f93
PARAMETERS = dict(radius=2.5, opacity=1, hardness=1, stabilizer=0, pressure_size=False,
    pressure_opacity=False, pressure_gamma=1, minimum_radius=.08, spacing=.11,
    minimum_opacity=0, flow=1, opacity_mode='dab')


def stencil(quarter=False):
    # Radius 2.5, minor semiaxis 1.25, centered at document pixel (3.5,3.5).
    horizontal = [191 if 1 <= x <= 5 and y in [2, 4] else
                  255 if 1 <= x <= 5 and y == 3 else 0
                  for y in range(HEIGHT) for x in range(WIDTH)]
    return [horizontal[(HEIGHT-1-x)*WIDTH+y] for y in range(HEIGHT) for x in range(WIDTH)] if quarter else horizontal


def cases(seed):
    rows = [dict(name='square-horizontal', angle=0), dict(name='square-vertical', angle=90),
            dict(name='square-eraser', angle=0, erase=True),
            dict(name='square-alpha-lock', angle=0, locked=True),
            dict(name='square-a8-selection', angle=90, a8=True)]
    for case in rows:
        composition = copy.deepcopy(seed['composition'])
        raster = [0x80335988 if case.get('erase') or case.get('locked') else 0]*AREA
        masks = {}
        if case.get('locked'):
            composition['layer_metadata'][0]['alpha_locked'] = True
        if case.get('a8'):
            raster = [0xffe5c798]*AREA
            masks = {1: bytes([255]*AREA)}
            composition['raster_masks'] = dict(version=1, next_id=2, target=dict(kind='mask', id=1),
                items=[dict(id=1, owner=dict(kind='layer', id=1), density=173, feather_radius_px=1, linked=False)])
            composition['selection'] = dict(current=geometry.selection_wire(bytes([128]*AREA), WIDTH, HEIGHT),
                                            previous=geometry.selection_wire(bytes([79]*AREA), WIDTH, HEIGHT))
        case['model'] = dict(width=WIDTH, height=HEIGHT, active=0, composition=composition, rasters=[raster], masks=masks)
    return rows


def write_fixture(path, model):
    if model['masks']:
        geometry.write_model(path, model)
        return
    metadata = json.dumps(model['composition'], separators=(',', ':'), ensure_ascii=False).encode()
    data = bytearray(b'MICOPI02'+struct.pack('<IIII', WIDTH, HEIGHT, 1, 0))
    properties = model['composition']['layers'][0]
    data += bytes([int(properties[0]), properties[1]])+struct.pack('<'+'I'*AREA, *model['rasters'][0])
    path.write_bytes(data+struct.pack('<I', len(metadata))+metadata)


def reference(case):
    shape = stencil(case['angle'] != 0)
    source = case['model']['rasters'][0]
    if case.get('a8'):
        # u16 Flow .25, then a single selection byte128 quantization.
        raw = bytes(223 if amount == 255 else 231 if amount == 191 else 255 for amount in shape)
        coverage = editing.feather_density(raw, WIDTH, HEIGHT, 1, 173)
        rgba = source
        surface = [editing.attenuate(p, m) for p, m in zip(rgba, coverage)]
    else:
        raw, rgba = b'', []
        for before, amount in zip(source, shape):
            if case.get('erase'):
                rgba.append((before & 0xffffff) | (((128*(255-amount)+127)//255) << 24))
            elif case.get('locked'):
                rgba.append((before & 0xff000000) | sum(
                    ((((COLOR >> shift) & 255)*amount+((before >> shift) & 255)*(255-amount)+127)//255) << shift
                    for shift in [0, 8, 16]))
            else:
                rgba.append((amount << 24) | COLOR if amount else 0)
        surface = [editing.over(0, p) for p in rgba]
    return dict(rgba=rgba, mask=raw, png=b''.join(editing.rgba(p) for p in surface))


def commands(output, prefix, scenarios):
    plan = Plan()
    add = plan.add
    add('nodes', 'node.catalog')
    add('describe', 'app.describe')
    add('providers', 'context.read', resources=['plugins.registry'])
    for case in scenarios:
        name = case['name']
        add(name+'-open', 'document.open', path=str(case['path']))
        add(name+'-tool', 'tool.select', tool='eraser' if case.get('erase') else 'brush')
        parameters = dict(PARAMETERS, tip_shape='square', tip_roundness=.5, tip_angle=case['angle'],
                          color='#000000' if case.get('a8') else '#194F93')
        if case.get('a8'):
            parameters.update(flow=.25, opacity_mode='stroke')
        add(name+'-options', 'brush.set', **parameters)
        add(name+'-before', 'canvas.state')
        add(name+'-stroke', 'canvas.stroke', points=[dict(x=3.5, y=3.5, pen=True, pressure=1, time=100)])
        add(name+'-after', 'canvas.state')
        add(name+'-tool-state', 'context.read', resources=['painting.tool'])
        add(name+'-png', 'canvas.snapshot')
        add(name+'-save', 'document.save', path=str(output/f'{prefix}-{name}.mcp'))
        if case.get('a8'):
            add(name+'-raw', 'mask.snapshot', mask_id=1, width=WIDTH, height=HEIGHT)
        add(name+'-undo', 'history.undo')
        add(name+'-undo-save', 'document.save', path=str(output/f'{prefix}-{name}-undo.mcp'))
        add(name+'-redo', 'history.redo')
        add(name+'-redone', 'canvas.snapshot')
        add(name+'-reopen', 'document.open', path=str(output/f'{prefix}-{name}.mcp'))
        add(name+'-reopened', 'canvas.state')
        add(name+'-verify', 'replay.verify')
        add(name+'-seek', 'replay.seek')
        add(name+'-archive', 'replay.export', path=str(output/f'{prefix}-{name}.mcreplay'))
    add('angle-options', 'brush.set', tip_angle=179)
    add('angle-wrap', 'brush.angle.adjust', delta_degrees=1)
    add('angle-minus', 'brush.angle.counterclockwise')
    add('angle-plus-coarse', 'brush.angle.clockwise_coarse')
    add('angle-minus-coarse', 'brush.angle.counterclockwise_coarse')
    add('tip-preset', 'brush.presets.upsert', name='Flat nib', owner='painting', tool_id='brush',
        parameters=dict(PARAMETERS, tip_shape='square', tip_roundness=.5, tip_angle=30))
    add('legacy-preset', 'brush.presets.upsert', name='Legacy round', owner='painting', tool_id='brush', parameters=PARAMETERS)
    add('preset-export', 'brush.presets.export', path=str(output/f'{prefix}-tips.json'), preset_ids=[1, 2])
    add('preset-import', 'brush.presets.import', path=str(output/f'{prefix}-tips.json'))
    add('legacy-apply', 'brush.presets.apply', preset_id=2)
    add('tip-apply', 'brush.presets.apply', preset_id=3)
    add('base-keep', 'tool.select', tool='brush')
    add('base-kept', 'canvas.state')
    add('round-reset', 'tool.select', tool='ink')
    add('last', 'canvas.state')
    return plan


def check_project(path, case, expected, original=False):
    project = foundation.read_project(path)
    rgba = case['model']['rasters'][0] if original else expected['rgba']
    assert project['rasters'][0][1] == struct.pack('<'+'I'*AREA, *rgba), path
    assert project['composition'] == case['model']['composition'], path
    if case.get('a8'):
        plane = project['planes'][1]
        assert (plane*AREA if len(plane) == 1 else plane) == (bytes([255]*AREA) if original else expected['mask']), path
    else:
        assert not project['planes'], path


def validate(values, plan, output, prefix, scenarios):
    at = lambda key: plan.at(values, key)
    nodes = {node['id']: node for node in at('nodes')['nodes']}
    properties = nodes['action.brush.set']['parameters_schema']['properties']
    assert properties['tip_shape']['enum'] == ['round', 'square']
    assert properties['tip_roundness']['minimum'] == .25 and properties['tip_roundness']['maximum'] == 1
    assert properties['tip_angle']['minimum'] == -180 and properties['tip_angle']['maximum'] == 180
    assert nodes['action.brush.angle.adjust']['parameters_schema']['properties']['delta_degrees']['maximum'] == 180
    schema = next(r for r in at('describe')['resources'] if r['id'] == 'painting.tool')['outputSchema']
    assert {'tip_shape', 'tip_roundness', 'tip_angle'} <= set(schema['properties']['parameters']['required'])
    native = at('providers')['resources']['plugins.registry']['value']['native']
    assert next(p for p in native if p['id'] == 'painting')['tool_options']['tip_geometry']
    assert not next(p for p in native if p['id'] == 'compositing')['tool_options']['tip_geometry']
    outcomes = {}
    for case in scenarios:
        name, expected = case['name'], reference(case)
        before, after = at(name+'-before'), at(name+'-after')
        assert after['revision'] == before['revision']+3, name
        for state in [after, at(name+'-reopened')]:
            assert state['parameters']['tip_shape'] == 'square' and state['parameters']['tip_roundness'] == .5
            assert state['parameters']['tip_angle'] == case['angle'] and not state['drawing']
        tool = at(name+'-tool-state')['resources']['painting.tool']['value']
        assert tool['parameters'] == after['parameters'] and tool['capabilities']['tip_geometry']
        png = base64.b64decode(at(name+'-png')['data'])
        assert pixels(png) == (WIDTH, HEIGHT, expected['png']), name
        assert pixels(base64.b64decode(at(name+'-redone')['data'])) == pixels(png), name
        check_project(output/f'{prefix}-{name}.mcp', case, expected)
        check_project(output/f'{prefix}-{name}-undo.mcp', case, expected, original=True)
        if case.get('a8'):
            assert at(name+'-raw')['samples'] == list(expected['mask'])
        assert at(name+'-verify')['valid']
        outcomes[name] = dict(png=png, seek=at(name+'-seek'))
    for key, angle in [('angle-wrap', -180), ('angle-minus', 179), ('angle-plus-coarse', -166), ('angle-minus-coarse', 179)]:
        assert at(key)['parameters']['tip_angle'] == angle
    data = (output/f'{prefix}-tips.json').read_bytes()
    assert at('preset-export')['sha256'] == hashlib.sha256(data).hexdigest()
    bundle = json.loads(data)
    assert len(bundle['items'][0]['parameters']) == 15 and len(bundle['items'][1]['parameters']) == 12
    assert at('preset-import')['imported_ids'] == [3, 4]
    for key in ['legacy-apply', 'last']:
        p = at(key)['parameters']
        assert (p['tip_shape'], p['tip_roundness'], p['tip_angle']) == ('round', 1, 0)
    p = at('tip-apply')['parameters']
    assert (p['tip_shape'], p['tip_roundness'], p['tip_angle']) == ('square', .5, 30)
    assert at('base-kept')['parameters']['tip_shape'] == 'square'
    return outcomes


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli', default='target/build/x86_64-pc-windows-gnu/release/micopi-cli.exe')
    args = parser.parse_args()
    cli = (ROOT/args.cli).resolve()
    output = ROOT/'target/test-output/brush-tips-smoke'/str(time.time_ns())
    output.mkdir(parents=True)
    plugins = output/'plugins'
    shutil.copytree(ROOT/'plugins', plugins)
    baseline = None
    for prefix in ['jsonl', 'mcp']:
        profile = output/(prefix+'-profile')
        profile.mkdir()
        (profile/'settings.json').write_text(json.dumps(dict(version=1)), encoding='utf-8')
        flags = ['--plugins', str(plugins), '--config-dir', str(profile)]
        bootstrap = Plan()
        bootstrap.add('new', 'canvas.new', width=WIDTH, height=HEIGHT)
        bootstrap.add('save', 'document.save', path=str(output/f'{prefix}-seed.mcp'))
        geometry.run_jsonl(cli, flags, bootstrap, output/f'{prefix}-bootstrap.jsonl')
        scenarios = cases(foundation.read_project(output/f'{prefix}-seed.mcp'))
        for case in scenarios:
            case['path'] = output/f'{prefix}-{case["name"]}-source.mcp'
            write_fixture(case['path'], case['model'])
        plan = commands(output, prefix, scenarios)
        values = geometry.run_jsonl(cli, flags, plan, output/'tips.jsonl') if prefix == 'jsonl' else editing.run_mcp(cli, flags, plan)
        current = validate(values, plan, output, prefix, scenarios)
        if baseline is not None:
            assert {name: pixels(result['png']) for name, result in current.items()} == {
                name: pixels(result['png']) for name, result in baseline.items()}
        baseline = current
        print(f'{prefix.upper()}: five tiny tip cases, preset formats and angle actions passed independent references.', flush=True)
    # JSONL intentionally stops at the first action failure. MCP independently
    # exercises strict original-number validation and leaves the state intact.
    negative = Plan()
    negative.add('before', 'canvas.state')
    negative.add('invalid-q', 'brush.set', tip_roundness=.249999999, error='invalid_arguments')
    negative.add('invalid-angle', 'brush.set', tip_angle=180.000001, error='invalid_arguments')
    negative.add('after', 'canvas.state')
    values = editing.run_mcp(cli, flags, negative)
    assert editing.document_state(negative.at(values, 'before')) == editing.document_state(negative.at(values, 'after'))
    spec = importlib.util.spec_from_file_location('tips_owner_replay', ROOT/'scripts/smoke-mask-owners.py')
    owners = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(owners)
    empty = output/'empty-profile'
    empty.mkdir()
    owners.replay_without_dlls(cli, empty, output, [dict(name=name) for name in baseline], baseline)
    print('Tip intent, native rectangle coverage, raw A8, Undo and DLL-free outcomes passed.')
    print(output)


if __name__ == '__main__':
    main()
