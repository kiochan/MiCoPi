"""Verify public raster-mask editing through JSONL, MCP, desktop and outcome replay.

The feather/density and compositing references are independent integer oracles.
Synthetic pressure contacts do not certify tablet or driver compatibility.
"""
import argparse
import base64
import copy
import importlib.util
import json
from pathlib import Path
import shutil
import struct
import subprocess
import time
import zlib

from png_checks import pixels

ROOT = Path(__file__).resolve().parents[1]
WIDTH, HEIGHT = 19, 13
AREA = WIDTH * HEIGHT
LANGUAGES = ['en', 'zh-CN', 'ja', 'ko']
MASK_ACTIONS = {'mask.query', 'mask.create', 'mask.delete', 'mask.set', 'mask.target',
                'mask.fill', 'mask.invert', 'mask.from_selection', 'mask.to_selection',
                'mask.snapshot', 'mask.apply'}
spec = importlib.util.spec_from_file_location('mask_foundation_smoke', ROOT/'scripts/smoke-raster-masks.py')
foundation = importlib.util.module_from_spec(spec)
spec.loader.exec_module(foundation)


class Plan(foundation.Rows):
    def __init__(self):
        super().__init__()
        self.failed_results = set()

    def failed_pipeline(self, key, **args):
        self.add(key, 'pipeline.run', **args)
        self.failed_results.add(key)


def feather_density(samples, width, height, radius, density):
    """Naive clamped six-pass reference, intentionally without native halo code."""
    data = list(samples)
    assert len(data) == width*height
    for radius in [(radius+2)//3, (radius+1)//3, radius//3]:
        for horizontal in [True, False]:
            result = []
            for y in range(height):
                for x in range(width):
                    values = []
                    for offset in range(-radius, radius+1):
                        sx = min(width-1, max(0, x+offset)) if horizontal else x
                        sy = y if horizontal else min(height-1, max(0, y+offset))
                        values.append(data[sy*width+sx])
                    result.append((sum(values)+len(values)//2)//len(values))
            data = result
    return bytes(255-((255-value)*density+127)//255 for value in data)


def over(before, after, mode='normal'):
    ab, aa = before >> 24, after >> 24
    if not aa:
        return before
    alpha = aa+(ab*(255-aa)+127)//255
    out = alpha << 24
    for shift in [16, 8, 0]:
        b, a = (before >> shift) & 255, (after >> shift) & 255
        blend = a if mode == 'normal' else 255-((255-a)*(255-b)+127)//255
        channel = ((255-aa)*ab*b+(255-ab)*aa*a+aa*ab*blend+alpha*127)//(alpha*255)
        out |= min(255, channel) << shift
    return out


def attenuate(pixel, amount):
    alpha = (pixel >> 24)*amount//255
    return (pixel & 0xffffff) | (alpha << 24) if alpha else 0


def mix(before, after, amount):
    if amount in [0, 255]:
        return after if amount else before
    ab, aa = before >> 24, after >> 24
    weight = ab*(255-amount)+aa*amount
    alpha = (weight+127)//255
    if not alpha:
        return 0
    out = alpha << 24
    for shift in [16, 8, 0]:
        numerator = ((before >> shift) & 255)*ab*(255-amount)+((after >> shift) & 255)*aa*amount
        out |= ((numerator+weight//2)//weight) << shift
    return out


def rgba(pixel):
    return bytes((pixel >> shift) & 255 for shift in [16, 8, 0, 24])


def composed(raw, filters=False, isolated=False, applied=False):
    coverage = feather_density(raw, WIDTH, HEIGHT, 3, 177)
    group = feather_density(bytes([128])*AREA, WIDTH, HEIGHT, 2, 203)[0]
    gate = (193*group+127)//255
    out = bytearray()
    for value in coverage:
        paper, color = 0xff203040, 0xff1f7fbf if filters else 0xffe08040
        alpha = value*197//255 if applied else (value*197+127)//255
        ink = attenuate(color, alpha)
        if isolated:
            out += rgba(over(paper, attenuate(ink, gate)))
        else:
            out += rgba(mix(paper, over(paper, ink, 'screen'), gate))
    return bytes(out)


def source_graph(enabled=True):
    graph = dict(enabled=enabled, linked=True, nodes=[
        dict(id=0, kind='layer', index=0, position=[0, 0]),
        dict(id=1, kind='layer', index=1, position=[0, 280]),
        dict(id=2, kind='filter', filter='invert', amount=1, position=[250, 280]),
        dict(id=3, kind='blend', mode='screen', opacity=1, position=[500, 140]),
        dict(id=4, kind='output', position=[750, 140]),
    ], edges=[dict(**{'from': a}, to=b, input=i) for a, b, i in
              [(1, 2, 0), (0, 3, 0), (2, 3, 1), (3, 4, 0)]])
    for node in graph['nodes']:
        node['node_type'] = 'compositing.'+node['kind']
    return graph


def workflow():
    return dict(version=1, nodes=[
        dict(id=1, type='action.mask.create', position=[0, 0],
             parameters=dict(owner=dict(kind='layer', id=1), select=False)),
        dict(id=2, type='action.mask.fill', position=[380, 0], parameters=dict(value=64)),
        dict(id=3, type='state.layers.masks', position=[0, 300], parameters={}),
    ], edges=[dict(**{'from': 1}, output='mask_id', to=2, input='mask_id')])


def commands(output, prefix):
    plan = Plan()
    add = plan.add
    add('workflow-new', 'canvas.new', width=WIDTH, height=HEIGHT)
    add('workflow-color', 'layer.fill_color', color='#5477CC')
    add('catalog', 'node.catalog')
    add('workflow-draft', 'workflow.set', graph=workflow())
    add('workflow-before', 'mask.query')
    add('workflow-revision', 'canvas.state')
    add('workflow-dry', 'workflow.run', atomic=True, dry_run=True)
    add('workflow-dry-state', 'canvas.state')
    add('workflow-inert', 'mask.query')
    add('workflow-run', 'workflow.run', atomic=True)
    add('workflow-mask', 'mask.snapshot', mask_id=1, width=WIDTH, height=HEIGHT)
    add('workflow-invert', 'mask.invert', mask_id=1)
    add('workflow-delete', 'mask.delete', mask_id=1)
    add('workflow-deleted', 'mask.query')
    add('workflow-delete-undo', 'history.undo')
    add('workflow-restored', 'mask.snapshot', mask_id=1, width=WIDTH, height=HEIGHT)
    add('workflow-state', 'context.read', resources=['layers.masks'])
    add('workflow-png', 'canvas.snapshot')
    add('workflow-seek', 'replay.seek')
    add('workflow-archive', 'replay.export', path=str(output/f'{prefix}-workflow.mcreplay'))
    add('new', 'canvas.new', width=WIDTH, height=HEIGHT)
    add('paper', 'layer.fill_color', color='#203040')
    add('paper-name', 'layer.rename', name='Paper')
    add('ink-add', 'layer.add')
    add('ink', 'layer.fill_color', color='#E08040')
    add('ink-name', 'layer.rename', name='Ink cel')
    add('ink-opacity', 'layer.set', opacity=197/255, blend_mode='screen')
    add('group', 'group.create', members=[dict(kind='layer', id=2)], name='Light group')
    add('pass', 'group.set', group_id=1, compositing='pass_through', opacity=193/255)
    add('source-filters', 'graph.set', graph=source_graph())
    add('selection', 'selection.shape', kind='rectangle', x=2, y=3, width=13, height=7)
    add('selection-source', 'selection.read')
    add('create', 'mask.create', owner=dict(kind='layer', id=2), source='selection')
    add('created', 'mask.snapshot', mask_id=1, width=WIDTH, height=HEIGHT)
    add('create-undo', 'history.undo')
    add('create-undone', 'mask.query')
    add('create-redo', 'history.redo')
    add('create-redone', 'mask.snapshot', mask_id=1, width=WIDTH, height=HEIGHT)
    add('fill', 'mask.fill', mask_id=1, value=64)
    add('filled', 'mask.snapshot', mask_id=1, width=WIDTH, height=HEIGHT)
    add('invert', 'mask.invert', mask_id=1)
    add('inverted', 'mask.snapshot', mask_id=1, width=WIDTH, height=HEIGHT)
    add('from-selection', 'mask.from_selection', mask_id=1)
    add('from-selection-raw', 'mask.snapshot', mask_id=1, width=WIDTH, height=HEIGHT)
    add('effects', 'mask.set', mask_id=1, density=177, feather_radius_px=3, linked=False)
    add('load-effective', 'mask.to_selection', mask_id=1, effective=True)
    add('effective-selection', 'selection.read')
    add('load-raw', 'mask.to_selection', mask_id=1)
    add('raw-selection', 'selection.read')
    add('zero-density', 'mask.set', mask_id=1, density=0, feather_radius_px=256)
    add('load-zero-density', 'mask.to_selection', mask_id=1, effective=True)
    add('zero-density-selection', 'selection.read')
    add('white-fill', 'mask.fill', mask_id=1, value=255)
    add('white-effects', 'mask.set', mask_id=1, density=128, feather_radius_px=256)
    add('load-white-edges', 'mask.to_selection', mask_id=1, effective=True)
    add('white-edges', 'selection.read')
    add('contact-effects', 'mask.set', mask_id=1, density=177, feather_radius_px=3)
    add('pixels-target', 'mask.target', target=dict(kind='layer_pixels'))
    add('pixels-target-query', 'mask.query')
    add('mask-target', 'mask.target', target=dict(kind='mask', id=1))
    add('clip', 'selection.shape', kind='rectangle', x=2, y=2, width=15, height=9)
    add('clip-source', 'selection.read')
    add('owner-locks', 'layer.set', layer_id=2, alpha_locked=True, pixel_locked=True)
    add('brush', 'tool.select', tool='brush')
    add('brush-options', 'brush.set', radius=2.5, color='#000000', opacity=.8,
        stabilizer=.55, pressure_size=True, pressure_opacity=True, spacing=.2, minimum_opacity=.1)
    points = [dict(x=x, y=y, pressure=p, time=t, pen=True, tool_override='brush')
              for x, y, p, t in [(0.5, 9, .2, 0), (8, 3, .9, 40), (17, 10, .4, 80)]]
    add('brush-stroke', 'canvas.stroke', points=points)
    add('brush-raw', 'mask.snapshot', mask_id=1, width=WIDTH, height=HEIGHT)
    add('brush-undo', 'history.undo')
    add('brush-undone', 'mask.snapshot', mask_id=1, width=WIDTH, height=HEIGHT)
    add('brush-redo', 'history.redo')
    add('brush-redone', 'mask.snapshot', mask_id=1, width=WIDTH, height=HEIGHT)
    add('eraser', 'tool.select', tool='eraser')
    add('eraser-stroke', 'canvas.stroke', points=[dict(p, tool_override='eraser') for p in points])
    add('eraser-raw', 'mask.snapshot', mask_id=1, width=WIDTH, height=HEIGHT)
    add('eraser-undo', 'history.undo')
    add('eraser-undone', 'mask.snapshot', mask_id=1, width=WIDTH, height=HEIGHT)
    add('eraser-redo', 'history.redo')
    add('eraser-redone', 'mask.snapshot', mask_id=1, width=WIDTH, height=HEIGHT)
    add('long-reset', 'mask.fill', mask_id=1, value=255)
    add('long-brush', 'tool.select', tool='brush')
    add('long-options', 'brush.set', radius=1.4)
    add('long-down', 'canvas.pointer', phase='down', x=3, y=4, pressure=.25,
        time=200, pen=True, tool_override='brush')
    for index in range(1, 261):
        add(f'long-{index}', 'canvas.pointer', phase='move', x=2+index % 15,
            y=2+(index*3) % 9, pressure=.2+(index % 7)*.1, time=200+index,
            pen=True, tool_override='brush')
        if index in [128, 256]:
            add(f'long-query-{index}', 'mask.query', mask_id=1)
            add(f'long-state-{index}', 'canvas.state')
    add('long-up', 'canvas.pointer', phase='up', x=7, y=8, pressure=0,
        time=461, pen=True, tool_override='brush')
    add('long-raw', 'mask.snapshot', mask_id=1, width=WIDTH, height=HEIGHT)
    add('long-undo', 'history.undo')
    add('long-undone', 'mask.snapshot', mask_id=1, width=WIDTH, height=HEIGHT)
    add('long-redo', 'history.redo')
    add('long-redone', 'mask.snapshot', mask_id=1, width=WIDTH, height=HEIGHT)
    add('owner-unlock', 'layer.set', layer_id=2, alpha_locked=False, pixel_locked=False)
    add('deselect', 'selection.deselect')
    add('group-mask', 'mask.create', owner=dict(kind='group', id=1), select=False)
    add('group-fill', 'mask.fill', mask_id=2, value=128)
    add('group-effects', 'mask.set', mask_id=2, density=203, feather_radius_px=2)
    add('pass-png', 'canvas.snapshot')
    add('isolated', 'group.set', group_id=1, compositing='isolated')
    add('isolated-png', 'canvas.snapshot')
    add('pass-restore', 'history.undo')
    add('filters-off', 'graph.set', graph=source_graph(False))
    add('filters-off-png', 'canvas.snapshot')
    add('final-layer-raw', 'mask.snapshot', mask_id=1, width=WIDTH, height=HEIGHT)
    add('final-group-raw', 'mask.snapshot', mask_id=2, width=WIDTH, height=HEIGHT)
    add('apply', 'mask.apply', mask_id=1)
    add('applied-query', 'mask.query')
    add('applied-png', 'canvas.snapshot')
    add('applied-save', 'document.save', path=str(output/f'{prefix}-applied.mcp'))
    add('apply-undo', 'history.undo')
    add('apply-undone', 'mask.snapshot', mask_id=1, width=WIDTH, height=HEIGHT)
    add('apply-undone-png', 'canvas.snapshot')
    add('apply-redo', 'history.redo')
    add('apply-redone-png', 'canvas.snapshot')
    add('apply-restore', 'history.undo')
    add('final-target', 'mask.target', target=dict(kind='mask', id=1))
    add('final-query', 'mask.query')
    add('final-graph', 'graph.read')
    add('final-png', 'canvas.snapshot')
    add('save', 'document.save', path=str(output/f'{prefix}.mcp'))
    add('reopen', 'document.open', path=str(output/f'{prefix}.mcp'))
    add('reopened-raw', 'mask.snapshot', mask_id=1, width=WIDTH, height=HEIGHT)
    add('reopened-png', 'canvas.snapshot')
    add('verify', 'replay.verify')
    add('seek', 'replay.seek')
    add('events', 'replay.events', limit=256)
    add('archive', 'replay.export', path=str(output/f'{prefix}.mcreplay'))
    return plan


def raw_snapshot(value):
    assert value['encoding'] == 'a8' and (value['width'], value['height']) == (WIDTH, HEIGHT)
    result = bytes(value['samples'])
    assert len(result) == AREA
    return result


def selection(value):
    current = value['current']
    assert (current['width'], current['height']) == (WIDTH, HEIGHT)
    result = zlib.decompress(bytes.fromhex(current['deflate_hex']), -15)
    assert len(result) == AREA
    return result


def image(value):
    return pixels(base64.b64decode(value['data']))


def expanded_plane(saved, identity):
    data = saved['planes'][identity]
    return data*AREA if len(data) == 1 else data


def validate(values, plan, output, prefix):
    at = lambda key: plan.at(values, key)
    raw = lambda key: raw_snapshot(at(key))
    catalog = json.dumps(at('catalog'))
    assert all('action.'+name in catalog for name in MASK_ACTIONS)
    assert 'state.layers.masks' in catalog
    assert at('workflow-dry')['ok'] and at('workflow-run')['ok']
    assert at('workflow-revision')['revision'] == at('workflow-dry-state')['revision']
    assert at('workflow-before')['items'] == at('workflow-inert')['items'] == []
    assert raw('workflow-mask') == bytes([64])*AREA
    assert at('workflow-deleted')['items'] == []
    assert raw('workflow-restored') == bytes([191])*AREA
    public = at('workflow-state')['resources']['layers.masks']
    assert public['ok'] and public['value']['items'][0]['id'] == 1
    assert image(at('workflow-png')) == (WIDTH, HEIGHT, bytes([84, 119, 204, 191])*AREA)
    source = selection(at('selection-source'))
    assert at('create')['mask_id'] == 1 and at('group-mask')['mask_id'] == 2
    assert raw('created') == raw('create-redone') == raw('from-selection-raw') == source
    assert at('create-undone')['items'] == [] and at('create-undone')['target']['kind'] == 'layer_pixels'
    assert raw('filled') == bytes([64])*AREA and raw('inverted') == bytes([191])*AREA
    assert selection(at('effective-selection')) == feather_density(source, WIDTH, HEIGHT, 3, 177)
    assert selection(at('raw-selection')) == source
    assert selection(at('zero-density-selection')) == selection(at('white-edges')) == bytes([255])*AREA
    assert at('pixels-target-query')['target'] == dict(kind='layer_pixels')
    clip = selection(at('clip-source'))
    brush, erased = raw('brush-raw'), raw('eraser-raw')
    assert min(brush) < 255 and any(a > b for a, b in zip(erased, brush))
    assert all(a >= b for a, b in zip(erased, brush)), 'Mask eraser must reveal toward 255'
    assert all(value == 255 for value, amount in zip(brush, clip) if amount == 0)
    assert raw('brush-undone') == bytes([255])*AREA
    assert raw('brush-redone') == raw('eraser-undone') == brush
    assert raw('eraser-redone') == erased
    for boundary in [128, 256]:
        assert at(f'long-state-{boundary}')['drawing']
        assert at(f'long-query-{boundary}')['target'] == dict(kind='mask', id=1)
    painted = raw('long-raw')
    assert min(painted) < 255 and raw('long-undone') == bytes([255])*AREA
    assert raw('long-redone') == raw('final-layer-raw') == raw('apply-undone') == raw('reopened-raw') == painted
    assert raw('final-group-raw') == bytes([128])*AREA
    expected = composed(painted)
    assert image(at('pass-png')) == (WIDTH, HEIGHT, composed(painted, filters=True))
    assert image(at('isolated-png')) == (WIDTH, HEIGHT, composed(painted, filters=True, isolated=True))
    for key in ['filters-off-png', 'apply-undone-png', 'final-png', 'reopened-png']:
        assert image(at(key)) == (WIDTH, HEIGHT, expected), key
    for key in ['applied-png', 'apply-redone-png']:
        assert image(at(key)) == (WIDTH, HEIGHT, composed(painted, applied=True)), key
    assert at('apply')['layer_id'] == 2 and [m['id'] for m in at('applied-query')['items']] == [2]
    query = at('final-query')
    assert query['target'] == dict(kind='mask', id=1)
    assert query['capabilities'] == dict(composition=True, effects=True)
    assert [(m['id'], m['owner']['kind'], m['density'], m['feather_radius_px']) for m in query['items']] == [
        (1, 'layer', 177, 3), (2, 'group', 203, 2)]
    graph = at('final-graph')
    assert graph['graph'] == source_graph(False)
    kinds = [node['kind'] for node in graph['effective_graph']['nodes']]
    assert kinds.count('mask_source') == 2 and kinds.count('apply_mask') == 1 and kinds.count('masked_mix') == 1
    assert 'filter' not in kinds, 'Disabling source filters must retain mask/group gates'
    saved = foundation.read_project(output/f'{prefix}.mcp')
    assert saved['magic'] == 'MICOPI04' and saved['replay']
    assert expanded_plane(saved, 1) == painted and expanded_plane(saved, 2) == bytes([128])*AREA
    expected_rgba = [struct.pack('<I', color)*AREA for color in [0xff203040, 0xffe08040]]
    assert [data for _, data in saved['rasters']] == expected_rgba, 'A8 painting changed raw RGBA'
    applied = foundation.read_project(output/f'{prefix}-applied.mcp')
    coverage = feather_density(painted, WIDTH, HEIGHT, 3, 177)
    baked = b''.join(struct.pack('<I', ((value << 24) | 0xe08040) if value else 0) for value in coverage)
    assert applied['rasters'][0][1] == expected_rgba[0] and applied['rasters'][1][1] == baked
    assert applied['composition']['graph'] == source_graph(False)
    assert applied['composition']['layers'][1] == [True, 197, 'screen']
    assert at('verify')['valid'] and not at('verify')['transaction_pending']
    events = at('events')['events']
    assert len(events) == at('events')['total'] < 256
    times = sorted(sample['time'] for event in events for sample in event['source'].get('samples', [])
                   if 200 <= sample['time'] <= 461)
    assert times == list(range(200, 462)), 'Streaming contact lost or repeated samples across recording boundaries'
    archive = foundation.archive_metrics(output/f'{prefix}.mcreplay')
    assert archive['a8_changes'] > 0 and archive['mask_events'] > 0
    return dict(painted=painted, brush=brush, erased=erased, rgba=expected,
                png=base64.b64decode(at('final-png')['data']), saved=saved, archive=archive,
                seek=at('seek'), workflow_png=base64.b64decode(at('workflow-png')['data']),
                workflow_seek=at('workflow-seek'))


def run_mcp(cli, flags, plan, require_masks=True):
    requests = foundation.INITIALIZE+[dict(jsonrpc='2.0', id=2, method='tools/list', params={})]
    requests += [foundation.request(10+i, row['action'], row['args']) for i, row in enumerate(plan.rows)]
    run = subprocess.run([str(cli), 'mcp', *flags], input='\n'.join(json.dumps(row) for row in requests)+'\n',
                         capture_output=True, text=True, encoding='utf-8', timeout=180)
    assert run.returncode == 0, run.stderr+run.stdout[-3000:]
    replies = {row['id']: row for row in map(json.loads, run.stdout.splitlines()) if 'id' in row}
    if require_masks:
        names = {row['name'] for row in replies[2]['result']['tools']}
        assert MASK_ACTIONS <= names, MASK_ACTIONS-names
    errors = {plan.positions[key]: code for key, code in plan.errors.items()}
    failed = {plan.positions[key] for key in plan.failed_results}
    values = []
    for index, row in enumerate(plan.rows):
        reply = replies[10+index]
        assert 'error' not in reply, (index, row, reply)
        result = reply['result']
        structured = result['structuredContent']
        if index in errors:
            assert result.get('isError') and structured['error']['code'] == errors[index], (index, row, reply)
            values.append(structured['error'])
        elif index in failed:
            value = structured['result']
            assert result.get('isError') and value['ok'] is False, (index, row, reply)
            values.append(value)
        else:
            assert not result.get('isError'), (index, row, reply)
            values.append(structured['result'])
    return values


def boundary_cases():
    cases = [
        ('canvas.move', dict(dx=1, dy=0, target='selection')),
        ('canvas.transform', dict(target='selection', translate_x=1)),
        ('canvas.transform.plan', dict(target='selection', translate_x=1)),
        ('canvas.transform.repeat', {}), ('canvas.transform.repeat_copy', {}),
        ('canvas.nudge', dict(direction='right', target='selection')),
        ('layer.via_copy', {}), ('layer.via_cut', {}),
        ('layer.merge_down', {}), ('layer.merge', {}),
        ('layer.merge_selected', dict(layer_ids=[1, 2])),
        ('layer.merge_selected.plan', dict(layer_ids=[1, 2])),
        ('layer.merge_visible', {}), ('layer.flatten', {}), ('layer.stamp_visible', {}),
        ('layer.batch', dict(action='layer.set', layer_ids=[1, 2], args=dict(opacity=.5))),
        ('group.ungroup', dict(group_id=1)), ('group.ungroup_parent', {}),
        ('group.merge', dict(group_id=1)),
        ('clipboard.cut', {}), ('clipboard.paste', {}), ('clipboard.paste_in_place', {}),
    ]
    cases += [(action, {}) for action in [
        'layer.rotate_90_cw', 'layer.rotate_90_ccw', 'layer.rotate_180',
        'layer.flip_horizontal', 'layer.flip_vertical',
    ]]
    return cases


def negatives(plan, output, project):
    add = plan.add
    add('negative-open', 'document.open', path=str(project))
    add('boundary-pixels', 'mask.target', target=dict(kind='layer_pixels'))
    add('boundary-before', 'canvas.state')
    add('boundary-masks-before', 'mask.query')
    add('boundary-png-before', 'canvas.snapshot')
    add('boundary-headless-preview', 'canvas.transform.configure', error='ui_unavailable',
        parameters=dict(target='layer', translate_x=1))
    for index, (action, parameters) in enumerate(boundary_cases()):
        add(f'boundary-{index}', action, error='mask_editing_pending', **parameters)
    add('boundary-after', 'canvas.state')
    add('boundary-masks-after', 'mask.query')
    add('boundary-png-after', 'canvas.snapshot')
    add('boundary-save', 'document.save', path=str(output/'boundaries-unchanged.mcp'))
    add('inactive-owner', 'layer.selection.set', layer_ids=[1, 2], active_layer_id=1)
    add('pixel-lock', 'layer.set', layer_id=2, pixel_locked=True)
    add('pixel-apply-rejected', 'mask.apply', error='raster_mask', mask_id=1)
    add('alpha-lock', 'layer.set', layer_id=2, pixel_locked=False, alpha_locked=True)
    add('alpha-apply-rejected', 'mask.apply', error='raster_mask', mask_id=1)
    add('unlock', 'layer.set', layer_id=2, alpha_locked=False)
    add('group-apply-rejected', 'mask.apply', error='raster_mask', mask_id=2)
    add('locked-raw', 'mask.snapshot', mask_id=1, width=WIDTH, height=HEIGHT)
    add('atomic-before', 'canvas.state')
    plan.failed_pipeline('atomic-rollback', atomic=True, steps=[
        dict(id='fill', action='mask.fill', args=dict(mask_id=1, value=0)),
        dict(id='fail', action='mask.delete', args=dict(mask_id=99999)),
    ])
    add('atomic-after', 'canvas.state')
    add('atomic-raw', 'mask.snapshot', mask_id=1, width=WIDTH, height=HEIGHT)
    add('atomic-png', 'canvas.snapshot')
    add('hidden-owner', 'layer.set', layer_id=2, visible=False)
    add('hidden-target', 'mask.target', target=dict(kind='mask', id=1))
    add('hidden-contact', 'canvas.pointer', error='mask_painting', phase='down', x=3, y=3,
        pressure=.5, time=700, pen=True, tool_override='brush')
    add('hidden-state', 'canvas.state')
    add('show-owner', 'layer.set', layer_id=2, visible=True)
    add('contact-options', 'brush.set', color='#000000', radius=1, stabilizer=.55)
    add('contact-down', 'canvas.pointer', phase='down', x=3, y=3, pressure=.3,
        time=800, pen=True, tool_override='brush')
    add('contact-before', 'canvas.state')
    add('contact-raw-before', 'mask.snapshot', mask_id=1, width=WIDTH, height=HEIGHT)
    add('contact-property-rejected', 'mask.set', error='stroke_active', mask_id=1, density=0)
    add('contact-geometry-rejected', 'canvas.resize', error='stroke_active', width=8, height=8)
    add('contact-after', 'canvas.state')
    add('contact-raw-after', 'mask.snapshot', mask_id=1, width=WIDTH, height=HEIGHT)
    add('contact-move', 'canvas.pointer', phase='move', x=10, y=7, pressure=.9,
        time=801, pen=True, tool_override='brush')
    add('contact-up', 'canvas.pointer', phase='up', x=15, y=8, pressure=0,
        time=802, pen=True, tool_override='brush')
    add('contact-complete', 'canvas.state')
    add('contact-undo', 'history.undo')
    add('contact-restored', 'mask.snapshot', mask_id=1, width=WIDTH, height=HEIGHT)
    add('negative-save', 'document.save', path=str(output/'negative-verified.mcp'))
    add('negative-verify', 'replay.verify')


def document_state(state):
    keys = ['document_info', 'revision', 'width', 'height', 'dirty', 'active_layer',
            'selected_layer_ids', 'groups', 'raster_masks', 'layers', 'selection', 'drawing',
            'selected_tool', 'tool_kind', 'parameters', 'last_transform', 'graph_enabled', 'render_error']
    return {key: state[key] for key in keys}


def validate_negatives(values, plan, output, expected):
    at = lambda key: plan.at(values, key)
    assert document_state(at('boundary-before')) == document_state(at('boundary-after'))
    assert at('boundary-masks-before') == at('boundary-masks-after')
    assert image(at('boundary-png-before')) == image(at('boundary-png-after')) == (WIDTH, HEIGHT, expected['rgba'])
    saved = foundation.read_project(output/'boundaries-unchanged.mcp')
    assert expanded_plane(saved, 1) == expected['painted'] and expanded_plane(saved, 2) == bytes([128])*AREA
    assert raw_snapshot(at('locked-raw')) == expected['painted']
    assert document_state(at('atomic-before')) == document_state(at('atomic-after'))
    assert at('atomic-rollback')['rolled_back'] is True
    assert raw_snapshot(at('atomic-raw')) == expected['painted']
    assert image(at('atomic-png')) == (WIDTH, HEIGHT, expected['rgba'])
    assert not at('hidden-state')['drawing']
    assert at('contact-before')['drawing'] and document_state(at('contact-before')) == document_state(at('contact-after'))
    assert raw_snapshot(at('contact-raw-before')) == raw_snapshot(at('contact-raw-after'))
    assert not at('contact-complete')['drawing']
    assert raw_snapshot(at('contact-restored')) == expected['painted']
    assert at('negative-verify')['valid']
    saved = foundation.read_project(output/'negative-verified.mcp')
    assert [data for _, data in saved['rasters']] == [data for _, data in expected['saved']['rasters']]


def write_project(path, saved, composition):
    """Re-encode the reviewed raw fixture without an inconsistent embedded journal."""
    data = bytearray(b'MICOPI04'+struct.pack('<IIII', WIDTH, HEIGHT, len(saved['rasters']), saved['active']))
    for properties, (_, raster) in zip(composition['layers'], saved['rasters']):
        data += bytes([int(properties[0]), properties[1]])+raster
    metadata = json.dumps(composition, separators=(',', ':'), ensure_ascii=False).encode()
    data += struct.pack('<I', len(metadata))+metadata+struct.pack('<I', len(saved['planes']))
    for identity, samples in sorted(saved['planes'].items()):
        data += struct.pack('<QBI', identity, int(len(samples) != 1), len(samples))+samples
    data += struct.pack('<Q', 0)
    path.write_bytes(data)


def legacy_checks(cli, profile, plugins, legacy, output, expected):
    old_plugins = output/'legacy-plugins'
    shutil.copytree(plugins, old_plugins)
    shutil.copy2(legacy, old_plugins/'compositing/micopi_compositing.dll')
    hidden = copy.deepcopy(expected['saved']['composition'])
    hidden['layers'][1][0] = False
    for mask in hidden['raster_masks']['items']:
        mask['enabled'] = False
        mask['density'] = 255
        mask['feather_radius_px'] = 0
    hidden['raster_masks']['items'][0].update(density=177, feather_radius_px=3)
    path = output/'hidden-disabled-effects.mcp'
    write_project(path, expected['saved'], hidden)
    plan = Plan()
    add = plan.add
    add('new', 'canvas.new', width=WIDTH, height=HEIGHT)
    add('fill', 'layer.fill_color', color='#336699')
    add('create', 'mask.create', owner=dict(kind='layer', id=1), select=False)
    add('disabled', 'mask.set', mask_id=1, enabled=False)
    add('hidden', 'layer.set', visible=False)
    add('before', 'canvas.state')
    add('mask-before', 'mask.query')
    add('png-before', 'canvas.snapshot')
    for index, properties in enumerate([dict(density=177), dict(feather_radius_px=3),
                                        dict(density=0, feather_radius_px=256)]):
        add(f'unsupported-{index}', 'mask.set', error='raster_mask', mask_id=1, **properties)
    add('invert', 'mask.invert', error='raster_mask', mask_id=1)
    add('open', 'document.open', error='renderer_missing', path=str(path))
    add('after', 'canvas.state')
    add('mask-after', 'mask.query')
    add('png-after', 'canvas.snapshot')
    values = run_mcp(cli, ['--plugins', str(old_plugins), '--config-dir', str(profile)], plan)
    at = lambda key: plan.at(values, key)
    assert at('mask-before')['capabilities'] == dict(composition=True, effects=False)
    assert document_state(at('before')) == document_state(at('after'))
    assert at('mask-before') == at('mask-after')
    assert image(at('png-before')) == image(at('png-after'))


def playback_without_dlls(cli, profile, output, expected):
    minimal = output/'replay-only-plugins'
    minimal.mkdir()
    for name in ['core', 'replay']:
        shutil.copytree(ROOT/'plugins'/name, minimal/name)
    assert not list(minimal.rglob('*.dll'))
    plan = Plan()
    plan.add('empty-new', 'canvas.new', width=7, height=5)
    plan.add('empty-state', 'canvas.state')
    plan.add('empty-png', 'canvas.snapshot')
    plan.add('ordinary-open', 'document.open', error='renderer_missing', path=str(output/'mcp.mcp'))
    plan.add('unchanged-state', 'canvas.state')
    plan.add('unchanged-png', 'canvas.snapshot')
    for name, archive, png, seek in [
        ('document', output/'mcp.mcreplay', expected['png'], expected['seek']),
        ('workflow', output/'mcp-workflow.mcreplay', expected['workflow_png'], expected['workflow_seek']),
    ]:
        plan.add(name+'-load', 'replay.load', path=str(archive))
        plan.add(name+'-verify', 'replay.verify', source='loaded')
        plan.add(name+'-snapshot', 'replay.snapshot', source='loaded')
        plan.add(name+'-seek', 'replay.seek', source='loaded')
    values = run_mcp(cli, ['--plugins', str(minimal), '--config-dir', str(profile)], plan, require_masks=False)
    assert plan.at(values, 'empty-state') == plan.at(values, 'unchanged-state')
    assert image(plan.at(values, 'empty-png')) == image(plan.at(values, 'unchanged-png'))
    for name, png, seek in [('document', expected['png'], expected['seek']),
                             ('workflow', expected['workflow_png'], expected['workflow_seek'])]:
        assert plan.at(values, name+'-verify')['valid']
        assert base64.b64decode(plan.at(values, name+'-snapshot')['data']) == png
        current = plan.at(values, name+'-seek')
        assert current['state_hash'] == seek['state_hash'] and current['surface_hash'] == seek['surface_hash']


def parity(actual, expected):
    for key in ['painted', 'brush', 'erased', 'rgba', 'png', 'workflow_png']:
        assert actual[key] == expected[key], 'Transport mismatch: '+key
    assert actual['saved']['composition'] == expected['saved']['composition']
    assert actual['saved']['rasters'] == expected['saved']['rasters']
    assert actual['saved']['planes'] == expected['saved']['planes']


def gui_checks(cli, gui, flags, profile, output, expected):
    # Keep the independent headless oracle usable without importing Win32 helpers.
    from capture_checks import CaptureChecks

    startup = subprocess.STARTUPINFO()
    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = 0
    process = subprocess.Popen([str(gui), *flags], startupinfo=startup)
    endpoint = profile/'session.json'

    def call(action_name, parameters):
        run = subprocess.run([str(cli), 'call', action_name, json.dumps(parameters),
                              '--attach', str(endpoint)], capture_output=True,
                             text=True, encoding='utf-8', timeout=30)
        assert run.returncode == 0, (action_name, parameters, run.stderr, run.stdout)
        return json.loads(run.stdout)['result']

    checks = CaptureChecks(call, process)
    captures = []
    try:
        deadline = time.monotonic()+30
        while not endpoint.exists():
            assert process.poll() is None and time.monotonic() < deadline, 'GUI startup failed'
            time.sleep(.05)
        plan = commands(output, 'gui')
        boundaries = output/'gui-boundaries'
        boundaries.mkdir()
        negatives(plan, boundaries, output/'gui.mcp')
        values = run_mcp(cli, ['--attach', str(endpoint)], plan)
        actual = validate(values, plan, output, 'gui')
        parity(actual, expected)
        validate_negatives(values, plan, boundaries, actual)
        call('document.open', dict(path=str(output/'gui.mcp')))
        call('window.command', dict(panel='layers', operation='show'))
        call('window.command', dict(panel='layers', operation='focus'))
        original = call('graph.read', {})
        raw = call('mask.snapshot', dict(mask_id=1, width=WIDTH, height=HEIGHT))
        artwork = call('canvas.snapshot', {})
        for language in LANGUAGES:
            call('settings.update', dict(path=str(profile/'settings.json'), ui=dict(language=language)))
            stored = json.loads((profile/'settings.json').read_text(encoding='utf-8-sig'))
            assert stored['ui']['language'] == language and stored['custom']['keep'] == 42
            # External catalogs load in the bounded background watcher.
            time.sleep(.25)
            for width in [300, 520]:
                height = 760
                checks.window('layers', 'float', x=70, y=70, width=width, height=height)
                entry, size = checks.ready('layers', (width, height))
                for identity, owner in [(1, 'layer'), (2, 'group')]:
                    call('mask.target', dict(target=dict(kind='mask', id=identity)))
                    checks.settle_layout()
                    before = call('canvas.state', {})
                    path = output/f'{language}-layers-{owner}-mask-{width}.png'
                    capture = checks.capture(path, 'layers', size=size)
                    captures.append(dict(language=language, owner=owner, logical_width=width,
                                         **capture))
                    assert document_state(call('canvas.state', {})) == document_state(before), \
                        'Capture admission/completion changed document state'
                    if width == 520 and identity == 1:
                        root = checks.root_hwnd(entry)
                        root_size = checks.client_size(root)
                        captures.append(dict(language=language, **checks.capture(
                            output/f'{language}-mask-workspace.png', size=root_size)))
            assert raw_snapshot(call('mask.snapshot', dict(mask_id=1, width=WIDTH, height=HEIGHT))) == raw_snapshot(raw)
            assert call('canvas.snapshot', {})['data'] == artwork['data']
            current = call('graph.read', {})
            assert current['graph'] == original['graph'] and current['effective_graph'] == original['effective_graph']
        call('mask.target', dict(target=dict(kind='mask', id=1)))
        assert call('replay.verify', {})['valid']
        assert len(captures) == 20
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
    parser.add_argument('--legacy-compositor',
                        default='target/packaging/eff8e54-20261106/stage/plugins/compositing/micopi_compositing.dll',
                        help='Previous masks-only compositor; explicitly skipped when absent in fresh CI.')
    args = parser.parse_args()
    cli, gui, legacy = [(ROOT/value).resolve() for value in [args.cli, args.gui, args.legacy_compositor]]
    assert cli.is_file(), f'Build/stage the matching host and DLLs first: {cli}'
    assert args.skip_gui or gui.is_file(), gui
    output = ROOT/'target/test-output/mask-editing-smoke'/str(time.time_ns())
    output.mkdir(parents=True)
    print(output, flush=True)
    plugins, profile = output/'plugins', output/'profile'
    shutil.copytree(ROOT/'plugins', plugins)
    (profile/'locales').mkdir(parents=True)
    for language in LANGUAGES[1:]:
        shutil.copy2(ROOT/'locales'/f'{language}.json', profile/'locales'/f'{language}.json')
    (profile/'settings.json').write_text(json.dumps(dict(version=1, custom=dict(keep=42))), encoding='utf-8')
    flags = ['--plugins', str(plugins), '--config-dir', str(profile)]
    plan = commands(output, 'jsonl')
    script = output/'mask-editing.jsonl'
    script.write_text('\n'.join(json.dumps(row) for row in plan.rows)+'\n', encoding='utf-8')
    run = subprocess.run([str(cli), 'run', str(script), *flags], capture_output=True,
                         text=True, encoding='utf-8', timeout=180)
    (output/'jsonl-results.jsonl').write_text(run.stdout, encoding='utf-8')
    if run.stderr:
        (output/'jsonl-stderr.txt').write_text(run.stderr, encoding='utf-8')
    records = [json.loads(line) for line in run.stdout.splitlines()]
    failed = next((i for i, row in enumerate(records) if not row['ok']), None)
    diagnostic = dict(exit_code=run.returncode, stderr=run.stderr, records=len(records),
                      command=plan.rows[failed] if failed is not None else None,
                      key=next((key for key, index in plan.positions.items() if index == failed), None),
                      error=records[failed] if failed is not None else None)
    assert run.returncode == 0 and len(records) == len(plan.rows) and failed is None, diagnostic
    expected = validate([row['data']['result'] for row in records], plan, output, 'jsonl')
    print(f'JSONL: {len(plan.rows)} commands and independent A8/composite references passed.', flush=True)
    plan = commands(output, 'mcp')
    boundaries = output/'mcp-boundaries'
    boundaries.mkdir()
    negatives(plan, boundaries, output/'mcp.mcp')
    (output/'mask-editing-mcp.jsonl').write_text(
        '\n'.join(json.dumps(row) for row in plan.rows)+'\n', encoding='utf-8')
    values = run_mcp(cli, flags, plan)
    actual = validate(values, plan, output, 'mcp')
    parity(actual, expected)
    validate_negatives(values, plan, boundaries, actual)
    print(f'MCP: {len(plan.rows)} commands, {len(boundary_cases())} geometry boundaries, locks, contacts and rollback passed.', flush=True)
    if legacy.is_file():
        legacy_checks(cli, profile, plugins, legacy, output, actual)
        print('Previous 40-byte compositor: effects rejected even for hidden/disabled masks before mutation.', flush=True)
    else:
        print(f'Legacy compositor check skipped: file absent at {legacy}', flush=True)
    playback_without_dlls(cli, profile, output, actual)
    saved = actual['saved']
    metrics = dict(width=WIDTH, height=HEIGHT, masks=len(saved['planes']),
                   logical_a8_bytes=AREA*len(saved['planes']), a8_payload_bytes=saved['a8_payload_bytes'],
                   a8_suffix_without_archive_bytes=saved['suffix_bytes']-len(saved['replay']),
                   embedded_archive_bytes=len(saved['replay']), archive=actual['archive'],
                   synthetic_input=True, physical_tablet_tested=False)
    if not args.skip_gui:
        captures = gui_checks(cli, gui, flags, profile, output, actual)
        metrics['captures'] = dict(count=len(captures), png_bytes=sum(row['bytes'] for row in captures))
        print('GUI transport parity and 20 exact-size English/CJK root/native mask-control captures passed.', flush=True)
    (output/'metrics.json').write_text(json.dumps(metrics, indent=2)+'\n', encoding='utf-8')
    print('DLL-free verified outcome playback passed; raw A8 and RGBA remain independently checked.', flush=True)
    print(json.dumps(metrics, sort_keys=True))
    print(output)


if __name__ == '__main__':
    main()
