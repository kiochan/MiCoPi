"""Verify document-wide raw-mask geometry across JSONL, MCP, desktop and replay.

Small independent byte/permutation/rational references avoid throughput claims.
Synthetic contacts do not establish physical tablet or driver compatibility.
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
import subprocess
import time
import zlib

from png_checks import density, pixels

ROOT = Path(__file__).resolve().parents[1]
WIDTH, HEIGHT = 7, 5
AREA = WIDTH*HEIGHT
LANGUAGES = ['en', 'zh-CN', 'ja', 'ko']
ANCHORS = ['top_left', 'top', 'top_right', 'left', 'center', 'right',
           'bottom_left', 'bottom', 'bottom_right']
spec = importlib.util.spec_from_file_location('mask_editing_smoke', ROOT/'scripts/smoke-mask-editing.py')
editing = importlib.util.module_from_spec(spec)
spec.loader.exec_module(editing)
foundation = editing.foundation
Plan = editing.Plan


def half_up(value):
    return (2*value.numerator+value.denominator)//(2*value.denominator)


def reframe(data, sw, sh, width, height, dx, dy, fill):
    return [data[(y-dy)*sw+x-dx] if 0 <= x-dx < sw and 0 <= y-dy < sh else fill
            for y in range(height) for x in range(width)]


def permutation(data, sw, sh, operation):
    width, height = (sh, sw) if operation in ['rotate_90_cw', 'rotate_90_ccw'] else (sw, sh)
    inverse = {
        'rotate_90_cw': lambda x, y: (y, sh-1-x),
        'rotate_90_ccw': lambda x, y: (sw-1-y, x),
        'rotate_180': lambda x, y: (sw-1-x, sh-1-y),
        'flip_horizontal': lambda x, y: (sw-1-x, y),
        'flip_vertical': lambda x, y: (x, sh-1-y),
    }[operation]
    return [data[sy*sw+sx] for y in range(height) for x in range(width)
            for sx, sy in [inverse(x, y)]]


def weights(source, target, index, method):
    if method == 'nearest':
        return [(min(source-1, (2*index+1)*source//(2*target)), Fraction(1))]
    if method == 'area' and source > target:
        left, right = Fraction(index*source, target), Fraction((index+1)*source, target)
        return [(sample, overlap/(right-left)) for sample in range(source)
                for overlap in [min(right, sample+1)-max(left, sample)] if overlap > 0]
    center = Fraction((2*index+1)*source, 2*target)-Fraction(1, 2)
    low = center.numerator//center.denominator
    fraction = center-low
    return [(min(source-1, max(0, low)), 1-fraction),
            (min(source-1, max(0, low+1)), fraction)]


def resample(data, sw, sh, width, height, method, rgba=False):
    if (sw, sh) == (width, height):
        return list(data)
    result = []
    for y in range(height):
        for x in range(width):
            samples = [(data[sy*sw+sx], wx*wy)
                       for sx, wx in weights(sw, width, x, method)
                       for sy, wy in weights(sh, height, y, method)]
            if method == 'nearest':
                result.append(samples[0][0])
            elif not rgba:
                result.append(half_up(sum(value*weight for value, weight in samples)))
            else:
                alpha = sum((value >> 24)*weight for value, weight in samples)
                rounded = half_up(alpha)
                result.append(0 if rounded == 0 else (rounded << 24) | sum(
                    half_up(sum(((value >> shift) & 255)*(value >> 24)*weight
                                for value, weight in samples)/alpha) << shift
                    for shift in [0, 8, 16]))
    return result


def selection_wire(data, width, height):
    compressor = zlib.compressobj(level=1, wbits=-15)
    encoded = compressor.compress(bytes(data))+compressor.flush()
    return dict(width=width, height=height, deflate_hex=encoded.hex())


def selection_bytes(value):
    result = {}
    for name in ['current', 'previous']:
        wire = value.get(name)
        if wire is not None:
            data = zlib.decompress(bytes.fromhex(wire['deflate_hex']), -15)
            assert len(data) == wire['width']*wire['height']
            result[name] = data
    return result


def write_fixture(path, seed):
    composition = copy.deepcopy(seed['composition'])
    composition['document_info'] = dict(resolution_ppi=25, resolution_explicit=False,
                                      png_density=dict(xppu=1000, yppu=2000, unit='meter'),
                                      initial_background='transparent')
    current = bytes((13*x+29*y+11) % 256 for y in range(HEIGHT) for x in range(WIDTH))
    previous = bytes((37*x+7*y+19) % 256 for y in range(HEIGHT) for x in range(WIDTH))
    composition['selection'] = dict(current=selection_wire(current, WIDTH, HEIGHT),
                                    previous=selection_wire(previous, WIDTH, HEIGHT))
    rasters = [[], [], []]
    for y in range(HEIGHT):
        for x in range(WIDTH):
            interior = 1 <= x <= 5 and 1 <= y <= 3
            rasters[0].append(0xffc4b6d8 if interior else 0)
            rasters[1].append(((32+x*19+y*11) << 24) | (x*31 << 16) | (y*43 << 8) | 79)
            rasters[2].append(0x91ef8c6b if interior and x >= 2 else 0)
    masks = {1: bytes((41*x+17*y+37) % 256 for y in range(HEIGHT) for x in range(WIDTH)),
             2: bytes([91]), 3: bytes([255]),
             4: bytes(64+(19*x+23*y) % 192 for y in range(HEIGHT) for x in range(WIDTH)),
             5: bytes([0])}
    metadata = json.dumps(composition, separators=(',', ':'), ensure_ascii=False).encode()
    data = bytearray(b'MICOPI04'+struct.pack('<IIII', WIDTH, HEIGHT, 3, seed['active']))
    for properties, raster in zip(composition['layers'], rasters):
        data += bytes([int(properties[0]), properties[1]])+struct.pack('<'+'I'*AREA, *raster)
    data += struct.pack('<I', len(metadata))+metadata+struct.pack('<I', len(masks))
    for identity, samples in masks.items():
        data += struct.pack('<QBI', identity, int(len(samples) != 1), len(samples))+samples
    data += struct.pack('<Q', 0)
    path.write_bytes(data)
    return dict(width=WIDTH, height=HEIGHT, active=seed['active'], composition=composition,
                rasters=rasters, masks={i: samples*AREA if len(samples) == 1 else samples
                                       for i, samples in masks.items()},
                selections=dict(current=current, previous=previous))


def write_model(path, model):
    width, height = model['width'], model['height']
    composition = model['composition']
    metadata = json.dumps(composition, separators=(',', ':'), ensure_ascii=False).encode()
    data = bytearray(b'MICOPI04'+struct.pack('<IIII', width, height, len(model['rasters']), model['active']))
    for properties, raster in zip(composition['layers'], model['rasters']):
        data += bytes([int(properties[0]), properties[1]])+struct.pack('<'+'I'*(width*height), *raster)
    data += struct.pack('<I', len(metadata))+metadata+struct.pack('<I', len(model['masks']))
    for identity, samples in sorted(model['masks'].items()):
        encoded = samples[:1] if len(set(samples)) == 1 else samples
        data += struct.pack('<QBI', identity, int(len(encoded) != 1), len(encoded))+encoded
    path.write_bytes(data+struct.pack('<Q', 0))


def bootstrap(output):
    plan = Plan()
    add = plan.add
    add('new', 'canvas.new', width=WIDTH, height=HEIGHT)
    for identity in [1, 2, 3]:
        if identity > 1:
            add(f'add-{identity}', 'layer.add')
        add(f'name-{identity}', 'layer.rename', name=['Paper', 'Hidden ink', 'Light'][identity-1])
    add('inner', 'group.create', members=[dict(kind='layer', id=2), dict(kind='layer', id=3)], name='Light group')
    add('inner-policy', 'group.set', group_id=1, compositing='pass_through', opacity=193/255)
    add('outer', 'group.create', members=[dict(kind='group', id=1)], name='Retained folder')
    add('outer-opacity', 'group.set', group_id=2, opacity=229/255)
    add('hidden', 'layer.set', layer_id=2, visible=False, pixel_locked=True, alpha_locked=True, position_locked=True)
    add('screen', 'layer.set', layer_id=3, blend_mode='screen', opacity=211/255)
    add('filters', 'graph.set', graph=foundation.source_graph())
    for identity, kind, owner in [(1, 'layer', 1), (2, 'layer', 2), (3, 'layer', 3), (4, 'group', 1), (5, 'group', 2)]:
        add(f'mask-{identity}', 'mask.create', owner=dict(kind=kind, id=owner), select=False)
    add('effects-1', 'mask.set', mask_id=1, density=203, feather_radius_px=1, linked=False)
    add('effects-2', 'mask.set', mask_id=2, density=177, feather_radius_px=2, enabled=False)
    add('group-unlinked', 'mask.set', mask_id=4, linked=False)
    add('group-disabled', 'mask.set', mask_id=5, enabled=False)
    add('selected', 'layer.selection.set', layer_ids=[1, 3], active_layer_id=3)
    add('target', 'mask.target', target=dict(kind='mask', id=1))
    add('seed', 'document.save', path=str(output/'seed.mcp'))
    return plan


def cases():
    result = []
    for width, height in [(10, 8), (4, 2)]:
        for anchor in ANCHORS:
            dx = 0 if anchor in ['left', 'top_left', 'bottom_left'] else width-WIDTH if anchor in ['right', 'top_right', 'bottom_right'] else (width-WIDTH)//2
            dy = 0 if anchor in ['top', 'top_left', 'top_right'] else height-HEIGHT if anchor in ['bottom', 'bottom_left', 'bottom_right'] else (height-HEIGHT)//2
            result.append(dict(name=f'resize-{width}-{anchor}', action='canvas.resize',
                               args=dict(width=width, height=height, anchor=anchor),
                               plan=dict(operation='resize', width=width, height=height, anchor=anchor),
                               width=width, height=height, dx=dx, dy=dy))
    result.append(dict(name='crop', action='canvas.crop', args=dict(x=1, y=1, width=5, height=3),
                       plan=dict(operation='crop', x=1, y=1, width=5, height=3), width=5, height=3, dx=-1, dy=-1))
    for operation in ['rotate_90_cw', 'rotate_90_ccw', 'rotate_180', 'flip_horizontal', 'flip_vertical']:
        width, height = (HEIGHT, WIDTH) if operation.startswith('rotate_90') else (WIDTH, HEIGHT)
        result.append(dict(name=operation, action='canvas.'+operation, args={},
                           plan=dict(operation=operation), width=width, height=height))
    for method in ['nearest', 'bilinear', 'area']:
        for width, height in [(4, 3), (9, 7)]:
            result.append(dict(name=f'resample-{method}-{width}', action='canvas.image_size',
                               args=dict(width=width, height=height, lock_aspect=False, method=method),
                               width=width, height=height, method=method))
    result.append(dict(name='trim', action='canvas.trim', args=dict(mode='transparent'),
                       width=5, height=3, dx=-1, dy=-1))
    return result


def reference(source, case):
    result = copy.deepcopy(source)
    sw, sh, width, height = source['width'], source['height'], case['width'], case['height']
    def transform(data, fill, rgba=False):
        if 'method' in case:
            return resample(data, sw, sh, width, height, case['method'], rgba)
        if 'dx' in case:
            return reframe(data, sw, sh, width, height, case['dx'], case['dy'], fill)
        return permutation(data, sw, sh, case['name'])
    result.update(width=width, height=height,
                  rasters=[transform(data, 0, True) for data in source['rasters']],
                  masks={i: bytes(transform(data, 255)) for i, data in source['masks'].items()},
                  selections={name: bytes(transform(data, 0)) for name, data in source['selections'].items()})
    if case['name'].startswith('rotate_90'):
        info = result['composition']['document_info']
        info['png_density'].update(xppu=2000, yppu=1000)
        info['resolution_ppi'] = 51
    result['composition']['selection'] = {name: selection_wire(data, width, height)
                                           for name, data in result['selections'].items()}
    return result


def composed(model):
    """Fixture's isolated outer / Pass Through inner gates over transparency."""
    width, height = model['width'], model['height']
    paper_mask = editing.feather_density(model['masks'][1], width, height, 1, 203)
    result = bytearray()
    for index, coverage in enumerate(paper_mask):
        paper = editing.attenuate(model['rasters'][0][index], coverage)
        ink_weight = (211*model['masks'][3][index]+127)//255
        ink = editing.attenuate(model['rasters'][2][index], ink_weight)
        group_weight = (193*model['masks'][4][index]+127)//255
        inner = editing.mix(0, editing.over(0, ink, 'screen'), group_weight)
        # The hidden raster/filter and disabled outer mask do not contribute.
        pixel = editing.over(paper, editing.attenuate(inner, 229))
        result += editing.rgba(pixel)
    return bytes(result)


def commands(output, prefix, fixture):
    plan = Plan()
    add = plan.add
    add('catalog', 'node.catalog')
    for case in cases():
        key = case['name']
        add(key+'-open', 'document.open', path=str(fixture))
        add(key+'-before', 'canvas.state')
        add(key+'-metadata-before', 'mask.query')
        add(key+'-events-before', 'replay.events', limit=256)
        if 'plan' in case:
            add(key+'-plan', 'canvas.geometry.plan', **case['plan'])
        elif 'method' in case:
            add(key+'-plan', 'canvas.image_size.plan', **case['args'])
        else:
            add(key+'-plan', 'canvas.trim_bounds', **case['args'])
        add(key+'-inert', 'canvas.state')
        add(key+'-operation', case['action'], **case['args'])
        add(key+'-events-after', 'replay.events', limit=256)
        add(key+'-metadata', 'mask.query')
        for identity in range(1, 6):
            add(f'{key}-mask-{identity}', 'mask.snapshot', mask_id=identity, width=256, height=256)
        add(key+'-selection', 'selection.read')
        add(key+'-info', 'canvas.info')
        add(key+'-graph', 'graph.read')
        add(key+'-png', 'canvas.snapshot')
        add(key+'-save', 'document.save', path=str(output/f'{prefix}-{key}.mcp'))
        add(key+'-undo', 'history.undo')
        add(key+'-undo-png', 'canvas.snapshot')
        add(key+'-undo-save', 'document.save', path=str(output/f'{prefix}-{key}-undo.mcp'))
        add(key+'-redo', 'history.redo')
        add(key+'-redo-png', 'canvas.snapshot')
        add(key+'-redo-save', 'document.save', path=str(output/f'{prefix}-{key}-redo.mcp'))
        add(key+'-reopen', 'document.open', path=str(output/f'{prefix}-{key}-redo.mcp'))
        add(key+'-reopened-png', 'canvas.snapshot')
        add(key+'-verify', 'replay.verify')
        add(key+'-seek', 'replay.seek')
        add(key+'-archive', 'replay.export', path=str(output/f'{prefix}-{key}.mcreplay'))
    return plan


def composition_contract(composition):
    result = copy.deepcopy(composition)
    result.pop('selection', None)
    if not result.get('document_info', {}).get('resolution_explicit'):
        result.get('document_info', {}).pop('resolution_explicit', None)
    return result


def assert_bytes(actual, expected, label):
    assert len(actual) == len(expected), (label, len(actual), len(expected))
    difference = next((i for i, values in enumerate(zip(actual, expected)) if values[0] != values[1]), None)
    assert difference is None, (label, difference, actual[difference], expected[difference])


def assert_project(path, expected):
    saved = foundation.read_project(path)
    width, height = expected['width'], expected['height']
    assert saved['magic'] == 'MICOPI04'
    assert (saved['width'], saved['height'], saved['active']) == (width, height, expected['active'])
    assert composition_contract(saved['composition']) == composition_contract(expected['composition']), path
    assert selection_bytes(saved['composition']['selection']) == expected['selections'], path
    for identity, samples in expected['masks'].items():
        actual = saved['planes'][identity]
        actual = actual*(width*height) if len(actual) == 1 else actual
        assert_bytes(actual, samples, f'{path}: mask{identity}')
    for index, (_, data) in enumerate(saved['rasters']):
        actual = struct.unpack('<'+'I'*(width*height), data)
        assert_bytes(actual, expected['rasters'][index], f'{path}: raster{index}')
    return saved


def assert_plan(value, case, drawing=False, sampler=True):
    width, height = case['width'], case['height']
    assert (value['source_width'], value['source_height'], value['width'], value['height']) == (WIDTH, HEIGHT, width, height), value
    assert value['layer_count'] == 3 and value['raster_mask_count'] == 5, value
    assert value['estimated_layer_bytes'] == width*height*12
    assert value['estimated_raster_mask_bytes'] == width*height*5
    assert value['estimated_effective_mask_bytes'] == width*height*2
    assert value['estimated_composite_bytes'] == width*height*4
    assert value['estimated_staged_bytes'] == width*height*19
    assert value['drawing'] == drawing
    assert value['contact_active'] == drawing
    assert value['raster_mask_feather_policy'] == 'fixed_canvas_pixels'
    assert value['requires_a8_sampler'] and value['a8_sampler_available'] == sampler
    if 'method' in case:
        assert value['mask_count'] == 2 and value['estimated_mask_bytes'] == width*height*2
        assert value['method'] == case['method']
    else:
        assert value['pixel_selection_count'] == 2 and value['estimated_selection_bytes'] == width*height*2
        assert value['cost_model'] == 'dense_outputs' and value['raster_mask_fill'] == 255
        assert value['operation'] == case['plan']['operation']
        if 'dx' in case:
            assert (value['offset_x'], value['offset_y']) == (case['dx'], case['dy'])


def validate(values, plan, output, prefix, source):
    at = lambda key: plan.at(values, key)
    assert 'action.canvas.geometry.plan' in json.dumps(at('catalog'))
    result = {}
    for case in cases():
        key = case['name']
        expected = reference(source, case)
        assert editing.document_state(at(key+'-before')) == editing.document_state(at(key+'-inert')), key
        if key == 'trim':
            preview = at(key+'-plan')
            assert (preview['x'], preview['y'], preview['width'], preview['height']) == (1, 1, 5, 3)
            assert preview['would_change'] and not preview['empty']
        else:
            assert_plan(at(key+'-plan'), case)
        before, after = at(key+'-events-before'), at(key+'-events-after')
        assert after['total'] == before['total']+1, (key, before, after)
        event = after['events'][-1]
        assert event['source']['action'] == case['action'] and event['success'], (key, event)
        expected_changes = 5 if (case['width'], case['height']) != (WIDTH, HEIGHT) else 2
        assert event['changed_mask_tiles'] == expected_changes, (key, event)
        metadata, old = at(key+'-metadata'), at(key+'-metadata-before')
        assert metadata['revision'] == old['revision']+1
        assert (metadata['width'], metadata['height']) == (case['width'], case['height'])
        for field in ['items', 'target', 'next_id', 'capabilities', 'limits']:
            assert metadata[field] == old[field], (key, field)
        for identity in range(1, 6):
            snapshot = at(f'{key}-mask-{identity}')
            assert snapshot['encoding'] == 'a8' and snapshot['mask_id'] == identity
            assert (snapshot['width'], snapshot['height']) == (case['width'], case['height'])
            assert_bytes(snapshot['samples'], expected['masks'][identity], f'{key}: mask{identity}')
        assert selection_bytes(at(key+'-selection')) == expected['selections'], key
        saved = assert_project(output/f'{prefix}-{key}.mcp', expected)
        assert_project(output/f'{prefix}-{key}-undo.mcp', source)
        assert_project(output/f'{prefix}-{key}-redo.mcp', expected)
        assert at(key+'-graph')['graph'] == source['composition']['graph']
        info = at(key+'-info')
        expected_info = expected['composition']['document_info']
        assert (info['width'], info['height']) == (case['width'], case['height'])
        assert info['png_density'] == expected_info['png_density']
        assert info['resolution_ppi'] == expected_info['resolution_ppi'] and info['physical_size_known']
        png = base64.b64decode(at(key+'-png')['data'])
        decoded = pixels(png)
        assert decoded[:2] == (case['width'], case['height'])
        assert_bytes(decoded[2], composed(expected), key+': composed RGBA')
        axes = expected['composition']['document_info']['png_density']
        assert density(png) == (axes['xppu'], axes['yppu'], 1)
        for suffix in ['redo-png', 'reopened-png']:
            assert base64.b64decode(at(key+'-'+suffix)['data']) == png, (key, suffix)
        undone_png = base64.b64decode(at(key+'-undo-png')['data'])
        assert pixels(undone_png) == (WIDTH, HEIGHT, composed(source)), key+': Undo coverage cache'
        assert density(undone_png) == (1000, 2000, 1)
        assert at(key+'-verify')['valid'] and not at(key+'-verify')['transaction_pending']
        assert saved['replay']
        archive = foundation.archive_metrics(output/f'{prefix}-{key}.mcreplay')
        assert archive['a8_changes'] >= 5 and archive['mask_events'] > 0
        result[key] = dict(png=png, seek=at(key+'-seek'), archive=archive,
                           undone_png=undone_png)
    assert len({value['undone_png'] for value in result.values()}) == 1
    return result


def pending_cases():
    allowed = {'canvas.resize', 'canvas.crop', 'canvas.image_size', 'canvas.image_size.plan',
               'canvas.trim', 'canvas.trim_bounds', 'canvas.rotate_90_cw', 'canvas.rotate_90_ccw',
               'canvas.rotate_180', 'canvas.flip_horizontal', 'canvas.flip_vertical'}
    return [(action, args) for action, args in editing.boundary_cases() if action not in allowed]


def negatives(plan, output, fixture):
    add = plan.add
    add('negative-open', 'document.open', path=str(fixture))
    add('negative-before', 'canvas.state')
    for key, action, error, args in [
        ('frame-budget', 'canvas.geometry.plan', 'canvas_geometry', dict(operation='resize', width=4096, height=4096)),
        ('image-budget', 'canvas.image_size.plan', 'image_size', dict(width=4096, height=4096, lock_aspect=False)),
        ('outside-crop', 'canvas.geometry.plan', 'invalid_crop', dict(operation='crop', x=1, y=0, width=WIDTH, height=HEIGHT)),
        ('rotation-frame', 'canvas.geometry.plan', 'canvas_geometry', dict(operation='rotate_180', width=3)),
        ('missing-operation', 'canvas.geometry.plan', 'invalid_arguments', dict(width=3, height=2)),
    ]:
        add(key, action, error=error, **args)
    add('negative-after', 'canvas.state')
    add('atomic-before', 'canvas.state')
    plan.failed_pipeline('atomic-failure', atomic=True, steps=[
        dict(id='frame', action='canvas.resize', args=dict(width=10, height=8, anchor='bottom_right')),
        dict(id='fail', action='mask.delete', args=dict(mask_id=99999)),
    ])
    add('atomic-after', 'canvas.state')
    add('atomic-save', 'document.save', path=str(output/'atomic-restored.mcp'))
    add('save-before', 'canvas.state')
    add('save-failure', 'document.save', error='io', path=str(output/'failure-is-directory'))
    add('save-after', 'canvas.state')
    add('contact-options', 'brush.set', radius=1, color='#000000', opacity=.8, stabilizer=.55,
        pressure_size=True, pressure_opacity=True)
    add('contact-down', 'canvas.pointer', phase='down', x=3, y=2, pressure=.4, time=100,
        pen=True, tool_override='brush')
    add('contact-before', 'canvas.state')
    add('contact-raw-before', 'mask.snapshot', mask_id=1, width=WIDTH, height=HEIGHT)
    add('contact-frame-plan', 'canvas.geometry.plan', operation='resize', width=10, height=8, anchor='center')
    add('contact-image-plan', 'canvas.image_size.plan', width=4, height=3, lock_aspect=False, method='area')
    add('contact-bounds-plan', 'canvas.trim_bounds', mode='transparent')
    for index, (action, args) in enumerate([
        ('canvas.resize', dict(width=10, height=8)),
        ('canvas.crop', dict(x=1, y=1, width=5, height=3)),
        ('canvas.rotate_90_cw', {}), ('canvas.rotate_180', {}), ('canvas.flip_vertical', {}),
        ('canvas.image_size', dict(width=4, height=3, lock_aspect=False)),
        ('canvas.trim', dict(mode='transparent')),
    ]):
        add(f'contact-rejected-{index}', action, error='stroke_active', **args)
    add('contact-after', 'canvas.state')
    add('contact-raw-after', 'mask.snapshot', mask_id=1, width=WIDTH, height=HEIGHT)
    add('contact-move', 'canvas.pointer', phase='move', x=5, y=3, pressure=.8, time=120,
        pen=True, tool_override='brush')
    add('contact-up', 'canvas.pointer', phase='up', x=5, y=3, pressure=0, time=130,
        pen=True, tool_override='brush')
    add('contact-undo', 'history.undo')
    add('contact-save', 'document.save', path=str(output/'contact-restored.mcp'))
    add('pending-pixels', 'mask.target', target=dict(kind='layer_pixels'))
    add('pending-before', 'canvas.state')
    for index, (action, args) in enumerate(pending_cases()):
        add(f'pending-{index}', action, error='mask_editing_pending', **args)
    add('pending-after', 'canvas.state')
    add('restore-target', 'mask.target', target=dict(kind='mask', id=1))
    add('negative-verify', 'replay.verify')


def validate_negatives(values, plan, output, source):
    at = lambda key: plan.at(values, key)
    state = lambda key: editing.document_state(at(key))
    assert state('negative-before') == state('negative-after')
    assert at('atomic-failure')['ok'] is False and at('atomic-failure')['rolled_back']
    assert state('atomic-before') == state('atomic-after')
    assert_project(output/'atomic-restored.mcp', source)
    assert state('save-before') == state('save-after') and at('save-before')['file'] == at('save-after')['file'], \
        'Failed persistence changed document, target, path or revision'
    assert state('contact-before') == state('contact-after')
    assert at('contact-before')['drawing'] and at('contact-after')['drawing']
    assert at('contact-raw-before') == at('contact-raw-after')
    frame = next(case for case in cases() if case['name'] == 'resize-10-center')
    assert_plan(at('contact-frame-plan'), frame, drawing=True)
    image = next(case for case in cases() if case['name'] == 'resample-area-4')
    assert_plan(at('contact-image-plan'), image, drawing=True)
    assert_project(output/'contact-restored.mcp', source)
    assert state('pending-before') == state('pending-after')
    assert at('negative-verify')['valid']


def legacy_checks(cli, plugins, profile, output, source, legacy):
    old = output/'legacy-plugins'
    shutil.copytree(plugins, old)
    shutil.copy2(legacy, old/'painting/micopi_painting_tools.dll')
    model = copy.deepcopy(source)
    model['masks'] = {identity: samples for identity, samples in model['masks'].items() if identity in [2, 5]}
    metadata = model['composition']['raster_masks']
    metadata['items'] = [item for item in metadata['items'] if item['id'] in [2, 5]]
    metadata['target'] = dict(kind='mask', id=2)
    fixture = output/'legacy-hidden-disabled.mcp'
    write_model(fixture, model)
    plan = Plan()
    add = plan.add
    add('open', 'document.open', path=str(fixture))
    add('identity-plan', 'canvas.geometry.plan', operation='resize', width=WIDTH, height=HEIGHT)
    add('identity-frame', 'canvas.resize', width=WIDTH, height=HEIGHT, anchor='bottom_right')
    add('identity-crop', 'canvas.crop', x=0, y=0, width=WIDTH, height=HEIGHT)
    add('identity-image', 'canvas.image_size', width=WIDTH, height=HEIGHT, lock_aspect=False, method='area')
    add('identity-save', 'document.save', path=str(output/'legacy-identity.mcp'))
    add('before', 'canvas.state')
    add('png-before', 'canvas.snapshot')
    add('plan', 'canvas.geometry.plan', operation='resize', width=10, height=8, anchor='center')
    for key, action, error, args in [
        ('frame', 'canvas.resize', 'canvas_size', dict(width=10, height=8)),
        ('crop', 'canvas.crop', 'canvas_size', dict(x=1, y=1, width=5, height=3)),
        ('rotate', 'canvas.rotate_90_cw', 'geometry', {}),
        ('flip', 'canvas.flip_vertical', 'geometry', {}),
        ('image', 'canvas.image_size', 'image_size', dict(width=4, height=3, lock_aspect=False)),
        ('trim', 'canvas.trim', 'trim', dict(mode='transparent')),
    ]:
        add(key, action, error=error, **args)
    add('after', 'canvas.state')
    add('png-after', 'canvas.snapshot')
    add('verify', 'replay.verify')
    values = editing.run_mcp(cli, ['--plugins', str(old), '--config-dir', str(profile)], plan)
    assert_project(output/'legacy-identity.mcp', model)
    identity = plan.at(values, 'identity-plan')
    assert identity['identity'] and identity['estimated_staged_bytes'] == 0
    assert not identity['requires_a8_sampler'] and not identity['a8_sampler_available']
    assert editing.document_state(plan.at(values, 'before')) == editing.document_state(plan.at(values, 'after'))
    assert plan.at(values, 'png-before')['data'] == plan.at(values, 'png-after')['data']
    preview = plan.at(values, 'plan')
    assert preview['requires_a8_sampler'] and not preview['a8_sampler_available']
    assert plan.at(values, 'verify')['valid']


def replay_without_dlls(cli, profile, output, fixture, expected):
    minimal = output/'replay-only-plugins'
    minimal.mkdir()
    for name in ['core', 'replay']:
        shutil.copytree(ROOT/'plugins'/name, minimal/name)
    assert not list(minimal.rglob('*.dll'))
    plan = Plan()
    plan.add('new', 'canvas.new', width=3, height=2)
    plan.add('before', 'canvas.state')
    plan.add('open', 'document.open', error='renderer_missing', path=str(fixture))
    plan.add('after', 'canvas.state')
    for case in cases():
        key = case['name']
        plan.add(key+'-load', 'replay.load', path=str(output/f'mcp-{key}.mcreplay'))
        plan.add(key+'-verify', 'replay.verify', source='loaded')
        plan.add(key+'-png', 'replay.snapshot', source='loaded')
        plan.add(key+'-seek', 'replay.seek', source='loaded')
    values = editing.run_mcp(cli, ['--plugins', str(minimal), '--config-dir', str(profile)], plan, require_masks=False)
    assert plan.at(values, 'before') == plan.at(values, 'after')
    for case in cases():
        key = case['name']
        assert plan.at(values, key+'-verify')['valid']
        assert base64.b64decode(plan.at(values, key+'-png')['data']) == expected[key]['png']
        seek = plan.at(values, key+'-seek')
        for field in ['state_hash', 'surface_hash']:
            assert seek[field] == expected[key]['seek'][field], (key, field)


def parity(actual, expected):
    assert actual.keys() == expected.keys()
    for key in expected:
        assert actual[key]['png'] == expected[key]['png'], 'Transport PNG mismatch: '+key
        assert actual[key]['undone_png'] == expected[key]['undone_png'], 'Transport Undo mismatch: '+key


def run_jsonl(cli, flags, plan, path):
    path.write_text('\n'.join(json.dumps(row) for row in plan.rows)+'\n', encoding='utf-8')
    run = subprocess.run([str(cli), 'run', str(path), *flags], capture_output=True,
                         text=True, encoding='utf-8', timeout=180)
    path.with_suffix('.responses.jsonl').write_text(run.stdout, encoding='utf-8')
    if run.stderr:
        path.with_suffix('.stderr.txt').write_text(run.stderr, encoding='utf-8')
    rows = [json.loads(line) for line in run.stdout.splitlines()]
    failed = next((i for i, row in enumerate(rows) if not row['ok']), None)
    detail = dict(exit_code=run.returncode, stderr=run.stderr, responses=len(rows),
                  command=plan.rows[failed] if failed is not None else None,
                  error=rows[failed] if failed is not None else None)
    assert run.returncode == 0 and len(rows) == len(plan.rows) and failed is None, detail
    return [row['data']['result'] for row in rows]


def gui_checks(cli, gui, flags, profile, output, fixture, source, expected):
    from capture_checks import CaptureChecks

    startup = subprocess.STARTUPINFO()
    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = 0
    process = subprocess.Popen([str(gui), *flags], startupinfo=startup)
    endpoint = profile/'session.json'
    captures = []

    def call(action, args):
        run = subprocess.run([str(cli), 'call', action, json.dumps(args), '--attach', str(endpoint)],
                             capture_output=True, text=True, encoding='utf-8', timeout=30)
        assert run.returncode == 0, (action, args, run.stderr, run.stdout)
        return json.loads(run.stdout)['result']

    checks = CaptureChecks(call, process)
    try:
        deadline = time.monotonic()+30
        while not endpoint.exists():
            assert process.poll() is None and time.monotonic() < deadline, 'GUI startup failed'
            time.sleep(.05)
        checks.settle_layout()
        plan = commands(output, 'gui', fixture)
        negative = output/'gui-boundaries'
        negative.mkdir()
        (negative/'failure-is-directory').mkdir()
        negatives(plan, negative, fixture)
        values = editing.run_mcp(cli, ['--attach', str(endpoint)], plan)
        actual = validate(values, plan, output, 'gui', source)
        parity(actual, expected)
        validate_negatives(values, plan, negative, source)
        call('document.open', dict(path=str(fixture)))
        artwork = call('canvas.snapshot', {})['data']
        graph = call('graph.read', {})
        for language in LANGUAGES:
            call('settings.update', dict(path=str(profile/'settings.json'), ui=dict(language=language)))
            stored = json.loads((profile/'settings.json').read_text(encoding='utf-8-sig'))
            assert stored['ui']['language'] == language and stored['custom']['keep'] == 42
            time.sleep(.25)
            for width in [300, 520]:
                checks.window('layers', 'float', x=70, y=70, width=width, height=760)
                entry, size = checks.ready('layers', (width, 760))
                checks.settle_layout()
                captures.append(dict(language=language, **checks.capture(
                    output/f'{language}-mask-geometry-layers-{width}.png', 'layers', size=size)))
            root = checks.root_hwnd(entry)
            root_size = checks.client_size(root)
            for command, name in [(10144, 'canvas-size'), (10145, 'crop'), (10173, 'image-size'), (10174, 'trim')]:
                call('ui.dispatch', dict(id='command', value=dict(UiCommand=command)))
                checks.settle_layout()
                before = editing.document_state(call('canvas.state', {}))
                captures.append(dict(language=language, **checks.capture(
                    output/f'{language}-masked-{name}.png', size=root_size)))
                assert editing.document_state(call('canvas.state', {})) == before
                call('ui.dispatch', dict(id='command', value='Cancel'))
                assert call('canvas.snapshot', {})['data'] == artwork
            current = call('graph.read', {})
            assert current['graph'] == graph['graph'] and current['effective_graph'] == graph['effective_graph']
        call('document.save', dict(path=str(output/'gui-capture-restored.mcp')))
        assert_project(output/'gui-capture-restored.mcp', source)
        assert call('replay.verify', {})['valid']
        assert len(captures) == 24
        (output/'captures.json').write_text(json.dumps(captures, indent=2)+'\n', encoding='utf-8')
        call('ui.dispatch', dict(id='close', value=True))
        assert process.wait(timeout=15) == 0
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=15)
    return captures


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    build = 'target/build/x86_64-pc-windows-gnu/release/'
    parser.add_argument('--cli', default=build+'micopi-cli.exe')
    parser.add_argument('--gui', default=build+'micopi.exe')
    parser.add_argument('--skip-gui', action='store_true')
    parser.add_argument('--legacy-painting', default='target/a8-sampler-baseline/micopi_painting_tools.dll',
                        help='Previous painting DLL without A8 sampler; explicitly skipped when absent.')
    args = parser.parse_args()
    cli, gui, legacy = [(ROOT/value).resolve() for value in [args.cli, args.gui, args.legacy_painting]]
    assert cli.is_file() and (args.skip_gui or gui.is_file()), 'Build/stage the matched host and DLLs first'
    output = ROOT/'target/test-output/mask-geometry-smoke'/str(time.time_ns())
    output.mkdir(parents=True)
    print(output, flush=True)
    plugins, profile = output/'plugins', output/'profile'
    shutil.copytree(ROOT/'plugins', plugins)
    (profile/'locales').mkdir(parents=True)
    for language in LANGUAGES[1:]:
        shutil.copy2(ROOT/'locales'/f'{language}.json', profile/'locales'/f'{language}.json')
    (profile/'settings.json').write_text(json.dumps(dict(version=1, custom=dict(keep=42))), encoding='utf-8')
    flags = ['--plugins', str(plugins), '--config-dir', str(profile)]
    setup = bootstrap(output)
    run_jsonl(cli, flags, setup, output/'bootstrap.jsonl')
    fixture = output/'independent-raw-mask-geometry.mcp'
    source = write_fixture(fixture, foundation.read_project(output/'seed.mcp'))
    plan = commands(output, 'jsonl', fixture)
    values = run_jsonl(cli, flags, plan, output/'geometry.jsonl')
    expected = validate(values, plan, output, 'jsonl', source)
    print(f'JSONL: {len(cases())} geometry cases / {len(plan.rows)} commands passed independent raw references.', flush=True)
    plan = commands(output, 'mcp', fixture)
    negative = output/'mcp-boundaries'
    negative.mkdir()
    (negative/'failure-is-directory').mkdir()
    negatives(plan, negative, fixture)
    (output/'geometry-mcp.jsonl').write_text('\n'.join(json.dumps(row) for row in plan.rows)+'\n', encoding='utf-8')
    values = editing.run_mcp(cli, flags, plan)
    actual = validate(values, plan, output, 'mcp', source)
    parity(actual, expected)
    validate_negatives(values, plan, negative, source)
    print(f'MCP: {len(plan.rows)} commands, {len(pending_cases())} remaining guards, contact purity and rollback passed.', flush=True)
    if legacy.is_file():
        legacy_checks(cli, plugins, profile, output, source, legacy)
        print('Previous painting DLL: identity frames retained; nonidentity hidden/disabled-mask geometry rejected safely.', flush=True)
    else:
        print(f'Legacy painting check skipped: file absent at {legacy}', flush=True)
    replay_without_dlls(cli, profile, output, fixture, actual)
    metrics = dict(cases=len(cases()), masks=5, source_width=WIDTH, source_height=HEIGHT,
                   logical_a8_bytes=AREA*5, raw_reference='independent exact rational/permutation/frame oracles',
                   synthetic_input=True, physical_tablet_tested=False,
                   journal_fault_injection='covered by Rust tests; no public transport fault hook',
                   archives={key: value['archive'] for key, value in actual.items()})
    if not args.skip_gui:
        captures = gui_checks(cli, gui, flags, profile, output, fixture, source, actual)
        metrics['captures'] = dict(count=len(captures), png_bytes=sum(row['bytes'] for row in captures))
        print('GUI geometry parity and 24 exact-size English/CJK framing/Image Size/Trim and native Layers captures passed.', flush=True)
    (output/'metrics.json').write_text(json.dumps(metrics, indent=2)+'\n', encoding='utf-8')
    print('All geometry outcome archives verified without DLLs. Synthetic input only.', flush=True)
    print(output)


if __name__ == '__main__':
    main()
