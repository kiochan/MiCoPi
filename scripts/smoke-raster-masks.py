"""Verify the document A8 foundation through JSONL, MCP, GUI and outcome replay.

The MICOPI04 fixture and integer pixel references are built independently here.
This gate has no public mask painting or mask creation commands. Synthetic RGBA
contacts exercise pressure/stabilizer continuity; they do not certify tablets.
"""
import argparse
import base64
import copy
import hashlib
import json
from pathlib import Path
import shutil
import struct
import subprocess
import time
import zlib

from png_checks import pixels

ROOT = Path(__file__).resolve().parents[1]
WIDTH, HEIGHT = 97, 65
AREA = WIDTH * HEIGHT
LANGUAGES = ['en', 'zh-CN', 'ja', 'ko']
INITIALIZE = [dict(jsonrpc='2.0', id=1, method='initialize',
                   params=dict(protocolVersion='2025-11-25')),
              dict(jsonrpc='2.0', method='notifications/initialized')]


def packed(value, kind='I'):
    return struct.pack('<'+kind, value)


def rgba(value):
    return bytes((value >> shift) & 255 for shift in [16, 8, 0, 24])


def attenuate(pixel, coverage):
    alpha = (pixel >> 24)*coverage//255
    return (pixel & 0xffffff) | (alpha << 24) if alpha else 0


def coverage(opacity, sample):
    return (opacity*sample+127)//255


def over(before, after, mode='normal'):
    """Straight-alpha source-over with the documented encoded-byte blend step."""
    ab, aa = before >> 24, after >> 24
    alpha = aa+(ab*(255-aa)+127)//255
    if not alpha:
        return 0
    result = alpha << 24
    for shift in [16, 8, 0]:
        b, a = (before >> shift) & 255, (after >> shift) & 255
        if mode == 'normal':
            blended = a
        elif mode == 'screen':
            blended = (255*255-(255-a)*(255-b)+127)//255
        else:
            raise AssertionError(mode)
        channel = ((255-aa)*ab*b+(255-ab)*aa*a+aa*ab*blended+alpha*127)//(alpha*255)
        result |= min(255, channel) << shift
    return result


def mix(before, after, amount):
    """Premultiplied interpolation of the completed Pass Through change."""
    if amount == 0:
        return before
    if amount == 255:
        return after
    ab, aa = before >> 24, after >> 24
    weight = ab*(255-amount)+aa*amount
    alpha = (weight+127)//255
    if not alpha:
        return 0
    result = alpha << 24
    for shift in [16, 8, 0]:
        numerator = ((before >> shift) & 255)*ab*(255-amount)+((after >> shift) & 255)*aa*amount
        result |= ((2*numerator+weight)//(2*weight)) << shift
    return result


def source_graph():
    nodes = [dict(id=0, position=[0, 0], kind='layer', index=0),
             dict(id=1, position=[0, 280], kind='layer', index=1),
             dict(id=2, position=[260, 280], kind='filter', filter='invert', amount=1),
             dict(id=3, position=[520, 140], kind='blend', mode='normal', opacity=1),
             dict(id=4, position=[0, 560], kind='layer', index=2),
             dict(id=5, position=[780, 280], kind='blend', mode='screen', opacity=1),
             dict(id=6, position=[1040, 280], kind='output')]
    edges = [dict(**{'from': a}, to=b, input=i) for a, b, i in
             [(1, 2, 0), (0, 3, 0), (2, 3, 1), (3, 5, 0), (4, 5, 1), (5, 6, 0)]]
    return dict(enabled=True, linked=True, nodes=nodes, edges=edges)


def fixture():
    layers = [[], [], []]
    dense_layer, dense_group = bytearray(), bytearray()
    for y in range(HEIGHT):
        for x in range(WIDTH):
            layers[0].append(0xffede6de)
            layers[1].append(0xc4000000 | ((64+x) << 16) | ((80+y) << 8) | 163)
            layers[2].append(0xa6f4ba73)
            dense_layer.append(x*255//(WIDTH-1))
            dense_group.append([0, 64, 128, 255][(x//24+y//17) % 4])
    item = lambda kind, identity: dict(kind=kind, id=identity)
    composition = dict(
        graph=source_graph(), layers=[[True, 255, 'normal'], [True, 197, 'normal'], [True, 211, 'screen']],
        workflow=dict(version=1, nodes=[], edges=[]),
        layer_metadata=[dict(id=i+1, name=name) for i, name in enumerate(['Paper', 'Ink cel', 'Warm light'])],
        next_layer_id=4,
        groups=dict(version=2, roots=[item('layer', 1), item('group', 1), item('group', 2)],
                    groups=[dict(id=1, name='Isolated ink', visible=True, opacity=191,
                                 blend_mode='normal', children=[item('layer', 2)]),
                            dict(id=2, name='Pass Through light', visible=True, opacity=193,
                                 blend_mode='normal', compositing='pass_through', children=[item('layer', 3)])]),
        next_group_id=3,
        raster_masks=dict(version=1, next_id=4, items=[
            dict(id=1, owner=item('layer', 2)), dict(id=2, owner=item('group', 1)),
            dict(id=3, owner=item('group', 2))]))
    return composition, layers, {1: bytes(dense_layer), 2: bytes([173]), 3: bytes(dense_group)}


def project_bytes(composition, layers, planes=None, version=4):
    result = bytearray(f'MICOPI0{version}'.encode()+struct.pack('<IIII', WIDTH, HEIGHT, len(layers), 1))
    for properties, samples in zip(composition['layers'], layers):
        result += bytes([int(properties[0]), properties[1]])
        result += struct.pack('<'+'I'*AREA, *samples)
    if version == 1:
        return bytes(result)
    metadata = json.dumps(composition, separators=(',', ':'), ensure_ascii=False).encode()
    result += packed(len(metadata))+metadata
    if version == 4:
        result += packed(len(planes))
        for identity, samples in sorted(planes.items()):
            assert len(samples) in [1, AREA]
            result += packed(identity, 'Q')+bytes([int(len(samples) != 1)])+packed(len(samples))+samples
    if version in [3, 4]:
        result += packed(0, 'Q')
    return bytes(result)


def read_project(path):
    """Independent strict parser, retaining raw RGBA and raw A8 separately."""
    data, offset = path.read_bytes(), 8
    assert data[:8] in [b'MICOPI01', b'MICOPI02', b'MICOPI03', b'MICOPI04'], path

    def take(length):
        nonlocal offset
        assert offset+length <= len(data), (path, offset, length)
        value = data[offset:offset+length]
        offset += length
        return value

    def number(kind='I'):
        return struct.unpack('<'+kind, take(struct.calcsize('<'+kind)))[0]

    width, height, count, active = [number() for _ in range(4)]
    rasters = []
    for _ in range(count):
        flags = take(2)
        rasters.append((flags, take(width*height*4)))
    composition = json.loads(take(number())) if data[:8] != b'MICOPI01' else None
    suffix_start = offset
    planes, encodings, replay = {}, {}, b''
    if data[:8] == b'MICOPI04':
        for _ in range(number()):
            identity, encoding, length = number('Q'), take(1)[0], number()
            assert identity not in planes
            assert (encoding == 0 and length == 1) or (encoding == 1 and length == width*height)
            encodings[identity], planes[identity] = encoding, take(length)
    if data[:8] in [b'MICOPI03', b'MICOPI04']:
        replay = take(number('Q'))
    assert offset == len(data), (path, 'trailing bytes')
    return dict(magic=data[:8].decode(), width=width, height=height, active=active, rasters=rasters,
                composition=composition, planes=planes, encodings=encodings, replay=replay,
                suffix_bytes=offset-suffix_start, a8_payload_bytes=sum(map(len, planes.values())))


def expected_image(layers, planes, layer_opacity=197, group_opacity=191, filters=True):
    result = bytearray()
    for index, (paper, ink, light) in enumerate(zip(*layers)):
        if filters:
            ink = (ink & 0xff000000) | ((ink ^ 0xffffff) & 0xffffff)
        ink = attenuate(ink, coverage(layer_opacity, planes[1][index]))
        isolated = attenuate(over(0, ink), coverage(group_opacity, planes[2][0]))
        before = over(paper, isolated)
        after = over(before, attenuate(light, 211), 'screen')
        result += rgba(mix(before, after, coverage(193, planes[3][index])))
    return bytes(result)


class Rows:
    def __init__(self):
        self.rows, self.positions, self.errors = [], {}, {}

    def add(self, key, action_name, /, error=None, **args):
        assert key not in self.positions
        self.positions[key] = len(self.rows)
        self.rows.append(dict(action=action_name, args=args))
        if error:
            self.errors[key] = error

    def at(self, values, key):
        return values[self.positions[key]]


def commands(output, prefix, source, composition):
    plan = Rows()
    add = plan.add
    add('open', 'document.open', path=str(source))
    add('initial-state', 'canvas.state')
    add('initial-graph', 'graph.read')
    add('initial', 'canvas.snapshot')
    add('layer-opacity', 'layer.set', layer_id=2, opacity=128/255)
    add('layer-opacity-png', 'canvas.snapshot')
    add('layer-undo', 'history.undo')
    add('layer-redo', 'history.redo')
    add('layer-redone-png', 'canvas.snapshot')
    add('layer-restore', 'history.undo')
    add('group-opacity', 'group.set', group_id=1, opacity=128/255)
    add('group-opacity-png', 'canvas.snapshot')
    add('group-restore', 'history.undo')
    graph = copy.deepcopy(composition['graph'])
    graph['enabled'] = False
    add('filters-off', 'graph.set', graph=graph)
    add('filters-off-png', 'canvas.snapshot')
    add('filters-restore', 'history.undo')
    add('brush', 'brush.set', radius=5, color='#F6E4A4', stabilizer=.55,
        pressure_size=True, pressure_opacity=True)
    add('stroke', 'canvas.stroke', points=[dict(x=12, y=52, pressure=.2, pen=True, time=0),
                                         dict(x=45, y=12, pressure=.9, pen=True, time=40),
                                         dict(x=86, y=50, pressure=.4, pen=True, time=80)])
    add('stroke-png', 'canvas.snapshot')
    add('stroke-undo', 'history.undo')
    add('stroke-restored-png', 'canvas.snapshot')
    add('add-layer', 'layer.add')
    add('added-png', 'canvas.snapshot')
    add('add-undo', 'history.undo')
    add('final-png', 'canvas.snapshot')
    add('save', 'document.save', path=str(output/f'{prefix}.mcp'))
    add('reopen', 'document.open', path=str(output/f'{prefix}.mcp'))
    add('reopened-png', 'canvas.snapshot')
    add('final-graph', 'graph.read')
    add('verify', 'replay.verify')
    add('seek', 'replay.seek')
    add('events', 'replay.events', limit=256)
    add('archive', 'replay.export', path=str(output/f'{prefix}.mcreplay'))
    return plan


def image(value):
    return pixels(base64.b64decode(value['data']))


def validate(values, plan, composition, layers, planes, output, prefix):
    at = lambda key: plan.at(values, key)
    expected = expected_image(layers, planes)
    for key in ['initial', 'stroke-restored-png', 'added-png', 'final-png', 'reopened-png']:
        assert image(at(key)) == (WIDTH, HEIGHT, expected), key
    for key in ['layer-opacity-png', 'layer-redone-png']:
        assert image(at(key)) == (WIDTH, HEIGHT, expected_image(layers, planes, layer_opacity=128)), key
    assert image(at('group-opacity-png')) == (WIDTH, HEIGHT, expected_image(layers, planes, group_opacity=128))
    assert image(at('filters-off-png')) == (WIDTH, HEIGHT, expected_image(layers, planes, filters=False))
    assert image(at('stroke-png')) != (WIDTH, HEIGHT, expected), 'Synthetic pressure stroke did not edit RGBA'
    assert at('verify')['valid'] and not at('verify')['transaction_pending']
    assert at('seek')['metadata']['composition']['raster_masks'] == composition['raster_masks']
    initial, final = at('initial-graph'), at('final-graph')
    assert initial['graph'] == final['graph'], 'Source Layer Filters changed during generated composition'
    assert initial['effective_graph'] == final['effective_graph']
    kinds = [n['kind'] for n in final['effective_graph']['nodes']]
    assert kinds.count('mask_source') == 3 and kinds.count('apply_mask') == 2 and kinds.count('masked_mix') == 1
    assert not any(n['kind'] in ['mask_source', 'apply_mask', 'masked_mix'] for n in final['graph']['nodes'])
    saved = read_project(output/f'{prefix}.mcp')
    assert saved['magic'] == 'MICOPI04' and saved['planes'] == planes
    assert saved['encodings'] == {1: 1, 2: 0, 3: 1}, 'Uniform A8 must stay compact'
    assert saved['composition']['raster_masks'] == composition['raster_masks']
    assert saved['replay'], 'Saved mask project must include verified outcomes'
    assert [data for _, data in saved['rasters']] == [struct.pack('<'+'I'*AREA, *samples) for samples in layers]
    events = at('events')['events']
    assert len(events) == at('events')['total'] < 256
    assert all(e.get('mask_version') == 1 for e in events)
    counts = [e['changed_mask_tiles'] for e in events]
    assert counts[0] == 3*((WIDTH+63)//64)*((HEIGHT+63)//64) and not any(counts[1:]), counts
    archive = archive_metrics(output/f'{prefix}.mcreplay')
    assert archive['a8_changes'] == sum(counts)
    return dict(png=base64.b64decode(at('final-png')['data']), rgba=expected,
                saved=saved, archive=archive, state_hash=at('seek')['state_hash'],
                surface_hash=at('seek')['surface_hash'])


def archive_metrics(path):
    data, offset, head, events, payload_bytes = path.read_bytes(), 8, bytes(32), [], 0
    assert data[:8] == b'MCRPLY02'
    while True:
        length = struct.unpack_from('<I', data, offset)[0]
        offset += 4
        if not length:
            count = struct.unpack_from('<Q', data, offset)[0]
            assert count == len(events) and data[offset+8:offset+40] == head and offset+40 == len(data)
            break
        stored, payload = data[offset:offset+32], data[offset+32:offset+32+length]
        assert len(payload) == length
        assert hashlib.sha256(head+packed(length)+payload).digest() == stored
        decoded = zlib.decompress(payload, -15)
        events.append(json.loads(decoded))
        payload_bytes += length
        head, offset = stored, offset+32+length
    return dict(bytes=len(data), events=len(events), a8_changes=sum(len(e.get('mask_changes', [])) for e in events),
                mask_events=sum(e.get('mask_version') == 1 for e in events),
                compressed_payload_bytes=payload_bytes, framing_bytes=len(data)-payload_bytes,
                head_hash=head.hex())


def request(number, action, args):
    return dict(jsonrpc='2.0', id=number, method='tools/call', params=dict(name=action, arguments=args))


def run_mcp(cli, flags, plan):
    requests = INITIALIZE+[request(10+i, row['action'], row['args']) for i, row in enumerate(plan.rows)]
    run = subprocess.run([str(cli), 'mcp', *flags], input='\n'.join(json.dumps(r) for r in requests)+'\n',
                         capture_output=True, text=True, encoding='utf-8', timeout=180)
    assert run.returncode == 0, run.stderr+run.stdout[-3000:]
    replies = {reply['id']: reply for reply in map(json.loads, run.stdout.splitlines()) if 'id' in reply}
    values = []
    by_index = {plan.positions[key]: error for key, error in plan.errors.items()}
    for i, row in enumerate(plan.rows):
        reply = replies[10+i]
        assert 'error' not in reply, reply
        result = reply['result']
        structured = result['structuredContent']
        if i in by_index:
            assert result.get('isError') and structured['error']['code'] == by_index[i], (row, reply)
            values.append(structured['error'])
        else:
            assert not result.get('isError'), (row, reply)
            values.append(structured['result'])
    return values


def negative_commands(plan, output, fixture_path, invalid_paths, target_path, allocator_path):
    add = plan.add
    add('boundary-open', 'document.open', path=str(fixture_path))
    add('boundary-before', 'canvas.state')
    add('boundary-graph-before', 'graph.read')
    add('boundary-png-before', 'canvas.snapshot')
    for index, (action, args) in enumerate([
        ('canvas.resize', dict(width=48, height=32)),
        ('canvas.image_size.plan', dict(width=48)),
        ('layer.merge_selected', dict(layer_ids=[1, 2])),
        ('layer.duplicate', {}), ('layer.translate', dict(dx=1, dy=0)),
        ('layer.batch', dict(action='layer.set', layer_ids=[1, 2], args=dict(opacity=.5))),
        ('group.create', dict(members=[dict(kind='layer', id=1)])), ('clipboard.paste', {}),
    ]):
        add(f'pending-{index}', action, error='mask_editing_pending', **args)
    for index, (path, error) in enumerate(invalid_paths):
        add(f'invalid-project-{index}', 'document.open', error=error, path=str(path))
    add('boundary-after', 'canvas.state')
    add('boundary-graph-after', 'graph.read')
    add('boundary-png-after', 'canvas.snapshot')
    add('boundary-save', 'document.save', path=str(output/'unchanged.mcp'))
    add('contact-brush', 'brush.set', radius=4, color='#ABCD91', stabilizer=.55,
        pressure_size=True, pressure_opacity=True)
    add('contact-down', 'canvas.pointer', phase='down', x=30, y=40, pressure=.25, pen=True, time=0)
    add('contact-before', 'canvas.state')
    add('contact-blocked', 'canvas.resize', error='mask_editing_pending', width=48, height=32)
    add('contact-after', 'canvas.state')
    add('contact-move', 'canvas.pointer', phase='move', x=65, y=24, pressure=.9, pen=True, time=40)
    add('contact-up', 'canvas.pointer', phase='up', x=78, y=38, pressure=.4, pen=True, time=80)
    add('contact-done', 'canvas.state')
    add('contact-undo', 'history.undo')
    add('contact-restored', 'canvas.snapshot')
    add('boundary-verify', 'replay.verify')
    add('target-open', 'document.open', path=str(target_path))
    add('target-before', 'canvas.state')
    add('target-before-png', 'canvas.snapshot')
    add('target-fill', 'layer.fill_color', error='mask_target_pending', color='#FFFFFF')
    add('target-pointer', 'canvas.pointer', error='mask_target_pending', phase='down', x=30, y=40,
        pressure=.5, pen=True, time=0)
    add('target-after', 'canvas.state')
    add('target-after-png', 'canvas.snapshot')
    add('target-rejected-save', 'document.save', path=str(output/'rejected-target.mcp'))
    add('target-multi-select', 'layer.selection.set', layer_ids=[2])
    add('target-still-mask', 'layer.fill_color', error='mask_target_pending', color='#FFFFFF')
    add('target-explicit-layer', 'layer.selection.set', layer_ids=[2], active_layer_id=2)
    add('target-fill-allowed', 'layer.fill_color', color='#FFFFFF')
    add('target-save', 'document.save', path=str(output/'reset-target.mcp'))
    add('target-verify', 'replay.verify')
    add('allocator-open', 'document.open', path=str(allocator_path))
    add('allocator-export', 'replay.export', path=str(output/'allocator-only.mcreplay'))
    add('allocator-verify', 'replay.verify')


def validate_negatives(values, plan, output, planes, expected, source):
    at = lambda key: plan.at(values, key)
    assert at('boundary-before') == at('boundary-after'), 'Rejected operations changed live state'
    assert at('boundary-graph-before') == at('boundary-graph-after')
    assert image(at('boundary-png-before')) == image(at('boundary-png-after')) == (WIDTH, HEIGHT, expected)
    assert read_project(output/'unchanged.mcp')['planes'] == planes
    assert at('contact-before')['drawing'] and at('contact-after') == at('contact-before')
    assert not at('contact-done')['drawing']
    assert image(at('contact-restored')) == (WIDTH, HEIGHT, expected)
    assert at('boundary-verify')['valid']
    assert at('target-after') == at('target-before') and not at('target-after')['drawing']
    assert image(at('target-before-png')) == image(at('target-after-png')) == (WIDTH, HEIGHT, expected)
    rejected = read_project(output/'rejected-target.mcp')
    original = read_project(source)
    assert rejected['planes'] == planes and rejected['rasters'] == original['rasters']
    assert rejected['composition']['raster_masks']['target'] == dict(kind='mask', id=1)
    target = read_project(output/'reset-target.mcp')
    assert target['planes'] == planes and 'target' not in target['composition']['raster_masks']
    assert target['rasters'][1][1] == packed(0xffffffff)*AREA
    assert at('target-verify')['valid'] and at('allocator-verify')['valid']
    metrics = archive_metrics(output/'allocator-only.mcreplay')
    assert metrics['mask_events'] and metrics['a8_changes'] == 0


def fixtures(output):
    composition, layers, planes = fixture()
    source = output/'independent-a8.mcp'
    source.write_bytes(project_bytes(composition, layers, planes))
    target = copy.deepcopy(composition)
    target['raster_masks']['target'] = dict(kind='mask', id=1)
    target_path = output/'mask-target.mcp'
    target_path.write_bytes(project_bytes(target, layers, planes))
    allocator = copy.deepcopy(composition)
    allocator.pop('groups')
    allocator.pop('next_group_id')
    allocator['raster_masks'] = dict(version=1, items=[], next_id=7)
    allocator_path = output/'allocator-only.mcp'
    allocator_path.write_bytes(project_bytes(allocator, layers, {}))
    invalid = []
    for version in [1, 2, 3]:
        path = output/f'masks-illegal-v{version}.mcp'
        # Version 1 has no metadata; retaining the suffix must fail exact-size checks.
        data = project_bytes(composition, layers, planes, version=version)
        if version == 1:
            data += source.read_bytes()[24+len(layers)*(2+AREA*4):]
        path.write_bytes(data)
        invalid.append((path, 'io'))
    wrong_id = bytearray(source.read_bytes())
    metadata_length = struct.unpack_from('<I', wrong_id, 24+len(layers)*(2+AREA*4))[0]
    first_id = 24+len(layers)*(2+AREA*4)+4+metadata_length+4
    struct.pack_into('<Q', wrong_id, first_id, 999)
    path = output/'wrong-mask-id.mcp'
    path.write_bytes(wrong_id)
    invalid.append((path, 'io'))
    for label, data in [('truncated-a8', source.read_bytes()[:-9]), ('trailing-a8', source.read_bytes()+b'x')]:
        path = output/f'{label}.mcp'
        path.write_bytes(data)
        invalid.append((path, 'io'))
    for label, field, value in [('hidden-density', 'density', 128), ('hidden-feather', 'feather_radius_px', 3)]:
        unsupported = copy.deepcopy(composition)
        unsupported['raster_masks']['items'][0].update(enabled=False, **{field: value})
        path = output/f'{label}.mcp'
        path.write_bytes(project_bytes(unsupported, layers, planes))
        invalid.append((path, 'renderer_missing'))
    return composition, layers, planes, source, target_path, allocator_path, invalid


def legacy_checks(cli, flags, output, composition, layers):
    plain = copy.deepcopy(composition)
    for key in ['raster_masks', 'groups', 'next_group_id']:
        plain.pop(key)
    plan = Rows()
    for version in [1, 2]:
        source = output/f'legacy-v{version}.mcp'
        source.write_bytes(project_bytes(plain, layers, version=version))
        plan.add(f'legacy-{version}', 'document.open', path=str(source))
        plan.add(f'legacy-png-{version}', 'canvas.snapshot')
    # Version 3 embeds a verified journal. Obtain it through the public writer,
    # rather than manufacturing an empty or inconsistent embedded archive.
    third = output/'legacy-v3.mcp'
    plan.add('legacy-save-3', 'document.save', path=str(third))
    plan.add('legacy-3', 'document.open', path=str(third))
    plan.add('legacy-png-3', 'canvas.snapshot')
    archive = ROOT/'tests/fixtures/replay/v1.mcreplay'
    plan.add('legacy-archive', 'replay.load', path=str(archive))
    plan.add('legacy-verify', 'replay.verify', source='loaded')
    plan.add('legacy-png', 'replay.snapshot', source='loaded')
    values = run_mcp(cli, flags, plan)
    for version in [1, 2, 3]:
        assert image(plan.at(values, f'legacy-png-{version}'))[:2] == (WIDTH, HEIGHT)
    assert read_project(third)['magic'] == 'MICOPI03'
    assert image(plan.at(values, 'legacy-png-2')) == image(plan.at(values, 'legacy-png-3'))
    assert plan.at(values, 'legacy-verify')['valid']
    info = json.loads((archive.parent/'v1.json').read_text())
    assert hashlib.sha256(base64.b64decode(plan.at(values, 'legacy-png')['data'])).hexdigest() == info['png_sha256']


def replay_without_dlls(cli, profile, output, expected):
    minimal = output/'replay-only-plugins'
    minimal.mkdir()
    for name in ['core', 'replay']:
        shutil.copytree(ROOT/'plugins'/name, minimal/name)
    assert not list(minimal.rglob('*.dll'))
    plan = Rows()
    plan.add('new', 'canvas.new', width=17, height=13)
    plan.add('before', 'canvas.state')
    plan.add('before-png', 'canvas.snapshot')
    plan.add('open-without-compositor', 'document.open', error='renderer_missing',
             path=str(output/'independent-a8.mcp'))
    plan.add('after', 'canvas.state')
    plan.add('after-png', 'canvas.snapshot')
    plan.add('load', 'replay.load', path=str(output/'mcp.mcreplay'))
    plan.add('verify', 'replay.verify', source='loaded', expected_head=expected['archive']['head_hash'])
    plan.add('seek', 'replay.seek', source='loaded')
    plan.add('snapshot', 'replay.snapshot', source='loaded')
    values = run_mcp(cli, ['--plugins', str(minimal), '--config-dir', str(profile)], plan)
    verify, seek = plan.at(values, 'verify'), plan.at(values, 'seek')
    assert plan.at(values, 'before') == plan.at(values, 'after')
    assert image(plan.at(values, 'before-png')) == image(plan.at(values, 'after-png'))
    assert verify['valid'] and seek['state_hash'] == expected['state_hash']
    assert seek['surface_hash'] == expected['surface_hash']
    assert base64.b64decode(plan.at(values, 'snapshot')['data']) == expected['png']


def baseline_checks(cli, flags, output, source, composition, layers):
    legacy_checks(cli, flags, output, composition, layers)
    plan = Rows()
    plan.add('new', 'canvas.new', width=WIDTH, height=HEIGHT)
    plan.add('before', 'canvas.state')
    plan.add('before-png', 'canvas.snapshot')
    plan.add('project', 'document.open', error='io', path=str(source))
    plan.add('archive', 'replay.load', error='replay_integrity', path=str(output/'mcp.mcreplay'))
    plan.add('marker-only', 'replay.load', error='replay_integrity', path=str(output/'allocator-only.mcreplay'))
    plan.add('after', 'canvas.state')
    plan.add('after-png', 'canvas.snapshot')
    values = run_mcp(cli, flags, plan)
    assert plan.at(values, 'before') == plan.at(values, 'after')
    assert image(plan.at(values, 'before-png')) == image(plan.at(values, 'after-png'))
    assert 'unknown field' in plan.at(values, 'archive')['message']
    assert 'mask_version' in plan.at(values, 'marker-only')['message'], 'Old strict event readers must reject the marker alone'


def gui_checks(cli, gui, flags, profile, output, source, composition, layers, planes, expected,
               target, allocator, invalid):
    startup = subprocess.STARTUPINFO()
    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = 0
    process = subprocess.Popen([str(gui), *flags], startupinfo=startup)
    endpoint = profile/'session.json'

    def call(action_name, /, **args):
        plan = Rows()
        plan.add('call', action_name, **args)
        return run_mcp(cli, ['--attach', str(endpoint)], plan)[0]

    try:
        deadline = time.monotonic()+30
        while not endpoint.exists():
            assert process.poll() is None and time.monotonic() < deadline, 'GUI startup failed'
            time.sleep(.05)
        plan = commands(output, 'gui', source, composition)
        boundaries = output/'gui-boundaries'
        boundaries.mkdir()
        negative_commands(plan, boundaries, source, invalid, target, allocator)
        values = run_mcp(cli, ['--attach', str(endpoint)], plan)
        actual = validate(values, plan, composition, layers, planes, output, 'gui')
        assert actual['rgba'] == expected['rgba']
        validate_negatives(values, plan, boundaries, planes, expected['rgba'], source)
        call('document.open', path=str(output/'gui.mcp'))
        call('window.command', panel='layers', operation='show')
        call('graph.open')
        graph = call('graph.read')
        before = call('canvas.snapshot')
        for language in LANGUAGES:
            call('settings.update', path=str(profile/'settings.json'), ui=dict(language=language))
            # External catalogs load asynchronously; wait for a quiet UI interval.
            deadline, previous, quiet = time.monotonic()+8, None, time.monotonic()
            while time.monotonic() < deadline:
                revision = call('canvas.state')['revision']
                if revision != previous:
                    previous, quiet = revision, time.monotonic()
                if time.monotonic()-quiet >= .75:
                    break
                time.sleep(.1)
            path = output/f'{language}-actual-mask-composition.png'
            call('ui.capture', path=str(path))
            deadline = time.monotonic()+10
            while not path.exists():
                assert process.poll() is None and time.monotonic() < deadline, 'GUI capture timeout'
                time.sleep(.05)
            assert path.stat().st_size > 1000
            current = call('graph.read')
            assert current['graph'] == graph['graph'] and current['effective_graph'] == graph['effective_graph']
            assert call('canvas.snapshot')['data'] == before['data']
            assert json.loads((profile/'settings.json').read_text(encoding='utf-8-sig'))['custom']['keep'] == 42
        assert call('replay.verify')['valid']
        call('ui.dispatch', id='close', value=True)
        assert process.wait(timeout=15) == 0
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=15)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli', default='dist/micopi-cli.exe')
    parser.add_argument('--gui', default='dist/MiCoPi.exe')
    parser.add_argument('--baseline-cli', default='target/mask-core-baseline/micopi-cli.exe',
                        help='Previous host executable; explicitly skipped when absent (fresh CI).')
    parser.add_argument('--skip-gui', action='store_true')
    args = parser.parse_args()
    cli, gui, baseline = [(ROOT/value).resolve() for value in [args.cli, args.gui, args.baseline_cli]]
    assert cli.is_file(), f'Build/stage the matching host and DLLs first: {cli}'
    assert args.skip_gui or gui.is_file(), gui
    output = ROOT/'target/test-output/raster-masks-smoke'/str(time.time_ns())
    output.mkdir(parents=True)
    plugins, profile = output/'plugins', output/'profile'
    shutil.copytree(ROOT/'plugins', plugins)
    (profile/'locales').mkdir(parents=True)
    for language in LANGUAGES[1:]:
        shutil.copy2(ROOT/'locales'/f'{language}.json', profile/'locales'/f'{language}.json')
    (profile/'settings.json').write_text(json.dumps(dict(version=1, custom=dict(keep=42))), encoding='utf-8')
    flags = ['--plugins', str(plugins), '--config-dir', str(profile)]
    composition, layers, planes, source, target, allocator, invalid = fixtures(output)
    legacy_checks(cli, flags, output, composition, layers)
    plan = commands(output, 'jsonl', source, composition)
    script = output/'raster-masks.jsonl'
    script.write_text('\n'.join(json.dumps(row) for row in plan.rows)+'\n', encoding='utf-8')
    run = subprocess.run([str(cli), 'run', str(script), *flags], capture_output=True,
                         text=True, encoding='utf-8', timeout=180)
    assert run.returncode == 0, run.stderr+run.stdout[-3000:]
    records = [json.loads(line) for line in run.stdout.splitlines()]
    assert len(records) == len(plan.rows) and all(record['ok'] for record in records), records[-3:]
    expected = validate([r['data']['result'] for r in records], plan, composition, layers, planes, output, 'jsonl')
    plan = commands(output, 'mcp', source, composition)
    negative_commands(plan, output, source, invalid, target, allocator)
    values = run_mcp(cli, flags, plan)
    actual = validate(values, plan, composition, layers, planes, output, 'mcp')
    assert actual['rgba'] == expected['rgba']
    validate_negatives(values, plan, output, planes, expected['rgba'], source)
    replay_without_dlls(cli, profile, output, actual)
    if baseline.is_file():
        baseline_plugins = baseline.parent/'plugins'
        baseline_flags = ['--plugins', str(baseline_plugins if baseline_plugins.is_dir() else plugins),
                          '--config-dir', str(profile)]
        baseline_checks(baseline, baseline_flags, output, source, composition, layers)
        print('Previous host: legacy 01/02/03 retained; MICOPI04 and mask-bearing/marker-only archives rejected without mutation.')
    else:
        print(f'Previous-host compatibility check skipped: executable absent at {baseline}')
    saved, archive = actual['saved'], actual['archive']
    metrics = dict(width=WIDTH, height=HEIGHT, masks=len(planes), logical_a8_bytes=AREA*len(planes),
                   a8_payload_bytes=saved['a8_payload_bytes'],
                   a8_suffix_without_archive_bytes=saved['suffix_bytes']-len(saved['replay']),
                   embedded_archive_bytes=len(saved['replay']), archive=archive)
    (output/'metrics.json').write_text(json.dumps(metrics, indent=2)+'\n', encoding='utf-8')
    print('JSONL/MCP: exact layer/filter, isolated/Pass Through mask coverage, compact persistence, safe rejection, contact continuity and DLL-free replay passed.')
    print(json.dumps(metrics, sort_keys=True))
    if not args.skip_gui:
        gui_checks(cli, gui, flags, profile, output, source, composition, layers, planes, expected,
                   target, allocator, invalid)
        print('GUI parity and four-language Actual Composition captures passed.')
    print(output)


if __name__ == '__main__':
    main()
