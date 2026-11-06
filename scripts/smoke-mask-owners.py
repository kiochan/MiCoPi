"""Verify single-layer mask ownership edits across transports and outcome replay.

Small independent MICOPI04, byte-copy and integer-composition references exercise
structural contracts without a throughput or physical-tablet compatibility claim.
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

from png_checks import pixels

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('mask_geometry_smoke', ROOT/'scripts/smoke-mask-geometry.py')
geometry = importlib.util.module_from_spec(spec)
spec.loader.exec_module(geometry)
editing, foundation, Plan = geometry.editing, geometry.foundation, geometry.Plan
WIDTH, HEIGHT = geometry.WIDTH, geometry.HEIGHT
AREA = WIDTH*HEIGHT
LANGUAGES = ['en', 'zh-CN', 'ja', 'ko']


def ids(model):
    return [row['id'] for row in model['composition']['layer_metadata']]


def target(model):
    return model['composition']['raster_masks'].get('target', dict(kind='layer_pixels'))


def mask_for(model, layer):
    return next((item for item in model['composition']['raster_masks']['items']
                 if item['owner'] == dict(kind='layer', id=layer)), None)


def model_cases(base):
    result = []
    for action in ['duplicate', 'delete']:
        for layer in [1, 2, 3]:
            for selected in ['pixels', 'owned', 'unrelated']:
                model = copy.deepcopy(base)
                masks = model['composition']['raster_masks']
                if selected == 'pixels':
                    masks.pop('target', None)
                else:
                    masks['target'] = dict(kind='mask', id=mask_for(model, layer)['id'] if selected == 'owned' else 4)
                result.append(dict(name=f'{action}-{layer}-{selected}', action=action, layer=layer, model=model))
    for action in ['duplicate', 'delete']:
        model = copy.deepcopy(base)
        masks = model['composition']['raster_masks']
        masks['items'] = [item for item in masks['items'] if item['id'] != 3]
        masks['target'] = dict(kind='mask', id=4)
        del model['masks'][3]
        result.append(dict(name=f'{action}-unmasked', action=action, layer=3, model=model))
    uniform = copy.deepcopy(base)
    uniform['masks'][3] = bytes([77])*AREA
    uniform['composition']['raster_masks']['target'] = dict(kind='mask', id=3)
    result.append(dict(name='duplicate-uniform', action='duplicate', layer=3, model=uniform))
    filtered = copy.deepcopy(base)
    filtered['composition']['layers'][1][0] = True
    filtered['composition']['layers'][1][1] = 137
    filtered['composition']['raster_masks']['target'] = dict(kind='mask', id=2)
    result.append(dict(name='duplicate-filtered-visible', action='duplicate', layer=2, model=filtered))
    return result


def rewrite_children(tree, source, replacement):
    for children in [tree['roots'], *(group['children'] for group in tree['groups'])]:
        for index, child in enumerate(children):
            if child == dict(kind='layer', id=source):
                children[index:index+1] = replacement
                return
    raise AssertionError('Missing raster in independent group tree')


def reference(case):
    before = case['model']
    model = copy.deepcopy(before)
    comp = model['composition']
    index, source = ids(model).index(case['layer']), case['layer']
    old_active = ids(model)[model['active']]
    selected = comp.get('selected_layer_ids', []) or [old_active]
    owned = mask_for(model, source)
    affected = []
    if case['action'] == 'duplicate':
        fresh = comp['next_layer_id']
        comp['next_layer_id'] += 1
        metadata = dict(comp['layer_metadata'][index], id=fresh)
        comp['layer_metadata'].insert(index+1, metadata)
        comp['layers'].insert(index+1, copy.deepcopy(comp['layers'][index]))
        model['rasters'].insert(index+1, list(model['rasters'][index]))
        model['origins'][fresh] = model['origins'][source]
        rewrite_children(comp['groups'], source, [dict(kind='layer', id=source), dict(kind='layer', id=fresh)])
        model['active'] = index+1
        comp.pop('selected_layer_ids', None)
        if owned:
            fresh_mask = comp['raster_masks']['next_id']
            comp['raster_masks']['next_id'] += 1
            metadata = copy.deepcopy(owned)
            metadata.update(id=fresh_mask, owner=dict(kind='layer', id=fresh))
            comp['raster_masks']['items'].append(metadata)
            model['masks'][fresh_mask] = bytes(model['masks'][owned['id']])
            affected = [fresh_mask]
            if target(before) == dict(kind='mask', id=owned['id']):
                comp['raster_masks']['target'] = dict(kind='mask', id=fresh_mask)
        model['result'] = dict(source_layer_id=source, layer_id=fresh, mask_ids=affected)
    else:
        rewrite_children(comp['groups'], source, [])
        del comp['layer_metadata'][index], comp['layers'][index], model['rasters'][index]
        if owned:
            affected = [owned['id']]
            comp['raster_masks']['items'] = [item for item in comp['raster_masks']['items'] if item['id'] != owned['id']]
            del model['masks'][owned['id']]
            if target(before) == dict(kind='mask', id=owned['id']):
                comp['raster_masks'].pop('target', None)
        candidates = [identity for identity in selected if identity != source]
        active = old_active if old_active != source else min(candidates,
            key=lambda identity: (abs(ids(before).index(identity)-before['active']), ids(before).index(identity)),
            default=ids(model)[min(index, len(ids(model))-1)])
        model['active'] = ids(model).index(active)
        selected = [identity for identity in ids(model) if identity in candidates or identity == active]
        comp.pop('selected_layer_ids', None)
        if len(selected) > 1:
            comp['selected_layer_ids'] = selected
        model['result'] = dict(deleted_layer_id=source, deleted_mask_ids=affected)
    return model


def composed(model):
    comp = model['composition']
    layers = {identity: (comp['layers'][index], model['rasters'][index]) for index, identity in enumerate(ids(model))}
    groups = {group['id']: group for group in comp['groups']['groups']}
    masks = {}
    for item in comp['raster_masks']['items']:
        if item.get('enabled', True):
            owner = item['owner']
            masks[(owner['kind'], owner['id'])] = editing.feather_density(
                model['masks'][item['id']], WIDTH, HEIGHT,
                item.get('feather_radius_px', 0), item.get('density', 255))

    def weight(kind, identity, opacity, pixel):
        coverage = masks.get((kind, identity))
        return opacity if coverage is None else (opacity*coverage[pixel]+127)//255

    def children(rows, initial, pixel):
        output = initial
        for row in rows:
            identity = row['id']
            if row['kind'] == 'layer':
                (visible, opacity, mode), data = layers[identity]
                if visible:
                    color = data[pixel]
                    if model['origins'][identity] == 2 and comp['graph']['enabled']:
                        color ^= 0xffffff
                    output = editing.over(output, editing.attenuate(color, weight('layer', identity, opacity, pixel)), mode)
            else:
                group = groups[identity]
                if not group['visible']:
                    continue
                gate = weight('group', identity, group['opacity'], pixel)
                if group.get('compositing', 'isolated') == 'pass_through':
                    output = editing.mix(output, children(group['children'], output, pixel), gate)
                else:
                    inner = children(group['children'], 0, pixel)
                    output = editing.over(output, editing.attenuate(inner, gate), group['blend_mode'])
        return output

    return b''.join(editing.rgba(children(comp['groups']['roots'], 0, pixel)) for pixel in range(AREA))


def filters(comp, identity):
    index = [row['id'] for row in comp['layer_metadata']].index(identity)
    nodes = {node['id']: node for node in comp['graph']['nodes']}
    parents = {edge['to']: edge['from'] for edge in comp['graph']['edges'] if edge['input'] == 0}
    result = []
    for node in nodes.values():
        if node['kind'] != 'filter':
            continue
        parent = node
        while parent['kind'] == 'filter':
            parent = nodes[parents[parent['id']]]
        if parent['kind'] == 'layer' and parent['index'] == index:
            properties = {key: value for key, value in node.items() if key not in ['id', 'position', 'node_type']}
            result.append((node['id'], properties))
    return result


def project(path, expected, original, exact_allocators=False):
    saved = foundation.read_project(path)
    comp = copy.deepcopy(saved['composition'])
    desired = copy.deepcopy(expected['composition'])
    assert saved['magic'] == 'MICOPI04' and (saved['width'], saved['height'], saved['active']) == (WIDTH, HEIGHT, expected['active'])
    assert geometry.selection_bytes(comp['selection']) == expected['selections']
    for value in [comp, desired]:
        value.pop('graph')
        value.pop('selection')
        if not value.get('document_info', {}).get('resolution_explicit'):
            value.get('document_info', {}).pop('resolution_explicit', None)
        if not value.get('selected_layer_ids'):
            value.pop('selected_layer_ids', None)
    if not exact_allocators:
        assert comp['next_layer_id'] >= desired['next_layer_id']
        assert comp['raster_masks']['next_id'] >= desired['raster_masks']['next_id']
        comp['next_layer_id'] = desired['next_layer_id']
        comp['raster_masks']['next_id'] = desired['raster_masks']['next_id']
    assert comp == desired, (path, comp, desired)
    assert set(saved['planes']) == set(expected['masks'])
    for identity, raw in expected['masks'].items():
        actual = saved['planes'][identity]
        actual = actual*AREA if len(actual) == 1 else actual
        geometry.assert_bytes(actual, raw, f'{path}: raw mask {identity}')
    assert [data for _, data in saved['rasters']] == [struct.pack('<'+'I'*AREA, *data) for data in expected['rasters']]
    assert saved['composition']['graph']['enabled'] == original['composition']['graph']['enabled']
    for identity in ids(expected):
        source = expected['origins'][identity]
        before = filters(original['composition'], source)
        after = filters(saved['composition'], identity)
        assert [props for _, props in after] == [props for _, props in before]
        if identity in ids(original):
            assert after == before, (path, identity, 'Retained filter identities changed')
        else:
            assert not set(key for key, _ in after) & set(key for key, _ in before)
    return saved


def commands(output, prefix, cases):
    plan = Plan()
    add = plan.add
    add('catalog', 'node.catalog')
    for case in cases:
        key = case['name']
        add(key+'-open', 'document.open', path=str(case['path']))
        add(key+'-before', 'canvas.state')
        add(key+'-before-png', 'canvas.snapshot')
        add(key+'-events-before', 'replay.events', limit=256)
        selectors = dict(layer_id=case['layer'])
        if key in ['duplicate-2-owned', 'delete-1-pixels']:
            selectors = dict(layer_id=float(case['layer']), index=float(ids(case['model']).index(case['layer'])))
        add(key+'-operation', 'layer.'+case['action'], **selectors)
        add(key+'-after', 'canvas.state')
        add(key+'-events-after', 'replay.events', limit=256)
        add(key+'-png', 'canvas.snapshot')
        add(key+'-save', 'document.save', path=str(output/f'{prefix}-{key}.mcp'))
        add(key+'-undo', 'history.undo')
        add(key+'-undo-png', 'canvas.snapshot')
        add(key+'-undo-save', 'document.save', path=str(output/f'{prefix}-{key}-undo.mcp'))
        add(key+'-redo', 'history.redo')
        add(key+'-redo-png', 'canvas.snapshot')
        add(key+'-redo-save', 'document.save', path=str(output/f'{prefix}-{key}-redo.mcp'))
        add(key+'-reopen', 'document.open', path=str(output/f'{prefix}-{key}-redo.mcp'))
        add(key+'-reopen-png', 'canvas.snapshot')
        add(key+'-verify', 'replay.verify')
        add(key+'-seek', 'replay.seek')
        add(key+'-archive', 'replay.export', path=str(output/f'{prefix}-{key}.mcreplay'))
    case = next(case for case in cases if case['name'] == 'duplicate-3-owned')
    add('independence-open', 'document.open', path=str(output/f'{prefix}-{case["name"]}.mcp'))
    add('independence-fill', 'mask.fill', mask_id=6, value=17)
    add('independence-save', 'document.save', path=str(output/f'{prefix}-independence.mcp'))
    add('independence-undo', 'history.undo')
    add('independence-png', 'canvas.snapshot')
    add('pipeline-open', 'document.open', path=str(case['path']))
    add('pipeline-run', 'pipeline.run', atomic=True, steps=[
        dict(id='copy', action='layer.duplicate', args=dict(layer_id=3)),
        dict(id='name', action='layer.rename', args=dict(layer_id={'$ref': 'copy#/result/layer_id'}, name='Bound copy')),
        dict(id='mask', action='mask.fill', args=dict(mask_id={'$ref': 'copy#/result/mask_ids/0'}, value=64)),
    ])
    add('pipeline-save', 'document.save', path=str(output/f'{prefix}-pipeline.mcp'))
    add('pipeline-verify', 'replay.verify')
    return plan


def validate(values, plan, output, prefix, cases):
    at = lambda key: plan.at(values, key)
    assert 'action.layer.duplicate' in json.dumps(at('catalog')) and 'mask_ids' in json.dumps(at('catalog'))
    outcomes = {}
    for case in cases:
        key, original = case['name'], case['model']
        expected = reference(case)
        result = at(key+'-operation')
        assert all(result[field] == value for field, value in expected['result'].items()), (key, result)
        assert result['layers'][result['active_layer']]['id'] == ids(expected)[expected['active']]
        if case['action'] == 'duplicate':
            assert result['selected_layer_ids'] == [result['layer_id']]
        assert at(key+'-after')['revision'] == at(key+'-before')['revision']+1
        before, after = at(key+'-events-before'), at(key+'-events-after')
        assert after['total'] == before['total']+1
        event = after['events'][-1]
        assert event['success'] and event['source']['action'] == 'layer.'+case['action']
        # Raster-count changes must not re-record unrelated raw mask planes.
        assert event['changed_mask_tiles'] == (len(expected['result']['mask_ids']) if case['action'] == 'duplicate' else 0), (key, event)
        project(output/f'{prefix}-{key}.mcp', expected, original)
        project(output/f'{prefix}-{key}-undo.mcp', original, original)
        project(output/f'{prefix}-{key}-redo.mcp', expected, original)
        png = base64.b64decode(at(key+'-png')['data'])
        assert pixels(png) == (WIDTH, HEIGHT, composed(expected)), key
        undone = base64.b64decode(at(key+'-undo-png')['data'])
        assert pixels(undone) == (WIDTH, HEIGHT, composed(original)), key+': Undo'
        assert at(key+'-before-png')['data'] == at(key+'-undo-png')['data']
        assert at(key+'-redo-png')['data'] == at(key+'-reopen-png')['data'] == at(key+'-png')['data']
        assert at(key+'-verify')['valid']
        outcomes[key] = dict(png=png, undone_png=undone, seek=at(key+'-seek'),
                             archive=foundation.archive_metrics(output/f'{prefix}-{key}.mcreplay'))
    case = next(case for case in cases if case['name'] == 'duplicate-3-owned')
    expected = reference(case)
    expected['masks'][6] = bytes([17])*AREA
    project(output/f'{prefix}-independence.mcp', expected, case['model'])
    assert at('independence-png')['data'] == at(case['name']+'-png')['data']
    expected['masks'][6] = bytes([64])*AREA
    expected['composition']['layer_metadata'][expected['active']]['name'] = 'Bound copy'
    project(output/f'{prefix}-pipeline.mcp', expected, case['model'])
    assert at('pipeline-run')['ok'] and at('pipeline-verify')['valid']
    return outcomes


def negatives(plan, output, case):
    add = plan.add
    add('numeric-open', 'document.open', path=str(case['path']))
    add('numeric-selection', 'layer.selection.set', layer_ids=[1.0, 2.0, 3.0], active_layer_id=2.0)
    add('numeric-after', 'canvas.state')
    add('negative-open', 'document.open', path=str(case['path']))
    add('atomic-before', 'canvas.state')
    plan.failed_pipeline('atomic-failure', atomic=True, steps=[
        dict(id='copy', action='layer.duplicate', args=dict(layer_id=3)),
        dict(id='fail', action='mask.delete', args=dict(mask_id=99999)),
    ])
    add('atomic-after', 'canvas.state')
    add('atomic-save', 'document.save', path=str(output/'atomic-restored.mcp'))
    for tool in ['brush', 'hand']:
        add(tool+'-open', 'document.open', path=str(case['path']))
        add(tool+'-options', 'brush.set', radius=1, color='#000000', opacity=.8, stabilizer=.55,
            pressure_size=True, pressure_opacity=True)
        add(tool+'-down', 'canvas.pointer', phase='down', x=3, y=2, pressure=.4, time=100,
            pen=True, tool_override=tool)
        add(tool+'-before', 'canvas.state')
        add(tool+'-raw-before', 'mask.snapshot', mask_id=3, width=WIDTH, height=HEIGHT)
        add(tool+'-duplicate', 'layer.duplicate', error='stroke_active', layer_id=3)
        add(tool+'-delete', 'layer.delete', error='stroke_active', layer_id=2)
        add(tool+'-after', 'canvas.state')
        add(tool+'-raw-after', 'mask.snapshot', mask_id=3, width=WIDTH, height=HEIGHT)
        add(tool+'-move', 'canvas.pointer', phase='move', x=5, y=3, pressure=.8, time=120,
            pen=True, tool_override=tool)
        add(tool+'-up', 'canvas.pointer', phase='up', x=5, y=3, pressure=0, time=130,
            pen=True, tool_override=tool)
        if tool == 'brush':
            add(tool+'-undo', 'history.undo')
        add(tool+'-save', 'document.save', path=str(output/f'{tool}-restored.mcp'))
    add('pending-open', 'document.open', path=str(case['path']))
    add('pending-pixels', 'mask.target', target=dict(kind='layer_pixels'))
    add('pending-before', 'canvas.state')
    for index, (action, args) in enumerate(editing.boundary_cases()):
        if action not in ['layer.duplicate', 'layer.delete']:
            add(f'pending-{index}', action, error='mask_editing_pending', **args)
    add('pending-after', 'canvas.state')
    add('negative-verify', 'replay.verify')


def validate_negatives(values, plan, output, case):
    at = lambda key: plan.at(values, key)
    numeric = at('numeric-after')
    assert numeric['layers'][numeric['active_layer']]['id'] == 2 and numeric['selected_layer_ids'] == [1, 2, 3]
    assert at('atomic-failure')['rolled_back'] and not at('atomic-failure')['ok']
    assert editing.document_state(at('atomic-before')) == editing.document_state(at('atomic-after'))
    project(output/'atomic-restored.mcp', case['model'], case['model'], exact_allocators=True)
    for tool in ['brush', 'hand']:
        assert editing.document_state(at(tool+'-before')) == editing.document_state(at(tool+'-after'))
        assert at(tool+'-raw-before') == at(tool+'-raw-after')
        assert at(tool+'-after')['drawing'] == (tool == 'brush')
        project(output/f'{tool}-restored.mcp', case['model'], case['model'])
    assert editing.document_state(at('pending-before')) == editing.document_state(at('pending-after'))
    assert at('negative-verify')['valid']


def replay_without_dlls(cli, profile, output, cases, outcomes):
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
        assert base64.b64decode(plan.at(values, key+'-png')['data']) == outcomes[key]['png']
        for field in ['state_hash', 'surface_hash']:
            assert plan.at(values, key+'-seek')[field] == outcomes[key]['seek'][field]


def parity(actual, expected):
    assert actual.keys() == expected.keys()
    for key in actual:
        assert actual[key]['png'] == expected[key]['png'] and actual[key]['undone_png'] == expected[key]['undone_png'], key


def gui_checks(cli, gui, flags, profile, output, cases, expected):
    from capture_checks import CaptureChecks
    startup = subprocess.STARTUPINFO()
    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = 0
    process = subprocess.Popen([str(gui), *flags], startupinfo=startup)
    endpoint = profile/'session.json'

    def call(action, args):
        run = subprocess.run([str(cli), 'call', action, json.dumps(args), '--attach', str(endpoint)],
                             capture_output=True, text=True, encoding='utf-8', timeout=30)
        assert run.returncode == 0, (action, args, run.stderr, run.stdout)
        return json.loads(run.stdout)['result']

    checks, captures = CaptureChecks(call, process), []
    try:
        deadline = time.monotonic()+30
        while not endpoint.exists():
            assert process.poll() is None and time.monotonic() < deadline, 'GUI startup failed'
            time.sleep(.05)
        checks.settle_layout()
        plan = commands(output, 'gui', cases)
        negative = output/'gui-boundaries'
        negative.mkdir()
        chosen = next(case for case in cases if case['name'] == 'duplicate-3-owned')
        negatives(plan, negative, chosen)
        values = editing.run_mcp(cli, ['--attach', str(endpoint)], plan)
        parity(validate(values, plan, output, 'gui', cases), expected)
        validate_negatives(values, plan, negative, chosen)
        call('document.open', dict(path=str(output/'gui-duplicate-3-owned.mcp')))
        call('layer.rename', dict(layer_id=4, name='Copied cel'))
        artwork = call('canvas.snapshot', {})['data']
        checks.window('layers', 'float', x=70, y=70, width=420, height=820)
        _, size = checks.ready('layers', (420, 820))
        for language in LANGUAGES:
            call('settings.update', dict(path=str(profile/'settings.json'), ui=dict(language=language)))
            stored = json.loads((profile/'settings.json').read_text(encoding='utf-8-sig'))
            assert stored['ui']['language'] == language and stored['custom']['keep'] == 42
            checks.settle_layout()
            state = call('canvas.state', {})
            assert state['layers'][state['active_layer']]['id'] == 4 and state['raster_masks']['target'] == dict(kind='mask', id=6)
            captures.append(dict(language=language, **checks.capture(output/f'{language}-copied-mask-owner.png', 'layers', size=size)))
            assert call('canvas.snapshot', {})['data'] == artwork
        assert call('replay.verify', {})['valid']
        call('ui.dispatch', dict(id='close', value=True))
        assert process.wait(timeout=15) == 0
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=15)
    (output/'captures.json').write_text(json.dumps(captures, indent=2)+'\n', encoding='utf-8')
    return captures


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    build = 'target/build/x86_64-pc-windows-gnu/release/'
    parser.add_argument('--cli', default=build+'micopi-cli.exe')
    parser.add_argument('--gui', default=build+'micopi.exe')
    parser.add_argument('--skip-gui', action='store_true')
    args = parser.parse_args()
    cli, gui = [(ROOT/value).resolve() for value in [args.cli, args.gui]]
    assert cli.is_file() and (args.skip_gui or gui.is_file()), 'Build/stage the matched host and DLLs first'
    output = ROOT/'target/test-output/mask-owners-smoke'/str(time.time_ns())
    output.mkdir(parents=True)
    print(output, flush=True)
    plugins, profile = output/'plugins', output/'profile'
    shutil.copytree(ROOT/'plugins', plugins)
    (profile/'locales').mkdir(parents=True)
    for language in LANGUAGES[1:]:
        shutil.copy2(ROOT/'locales'/f'{language}.json', profile/'locales'/f'{language}.json')
    (profile/'settings.json').write_text(json.dumps(dict(version=1, custom=dict(keep=42))), encoding='utf-8')
    flags = ['--plugins', str(plugins), '--config-dir', str(profile)]
    geometry.run_jsonl(cli, flags, geometry.bootstrap(output), output/'bootstrap.jsonl')
    base = geometry.write_fixture(output/'initial.mcp', foundation.read_project(output/'seed.mcp'))
    base['composition']['layers'][2][1] = 137
    base['masks'][3] = bytes((17*x+31*y+27) % 256 for y in range(HEIGHT) for x in range(WIDTH))
    metadata = next(item for item in base['composition']['raster_masks']['items'] if item['id'] == 3)
    metadata.update(density=177, feather_radius_px=2, linked=False)
    base['origins'] = {identity: identity for identity in ids(base)}
    cases = model_cases(base)
    for case in cases:
        case['path'] = output/(case['name']+'.mcp')
        geometry.write_model(case['path'], case['model'])
    plan = commands(output, 'jsonl', cases)
    values = geometry.run_jsonl(cli, flags, plan, output/'owners.jsonl')
    expected = validate(values, plan, output, 'jsonl', cases)
    print(f'JSONL: {len(cases)} owner cases / {len(plan.rows)} commands passed independent raw/composition references.', flush=True)
    plan = commands(output, 'mcp', cases)
    negative = output/'mcp-boundaries'
    negative.mkdir()
    chosen = next(case for case in cases if case['name'] == 'duplicate-3-owned')
    negatives(plan, negative, chosen)
    (output/'owners-mcp.jsonl').write_text('\n'.join(json.dumps(row) for row in plan.rows)+'\n', encoding='utf-8')
    values = editing.run_mcp(cli, flags, plan)
    actual = validate(values, plan, output, 'mcp', cases)
    parity(actual, expected)
    validate_negatives(values, plan, negative, chosen)
    print(f'MCP: {len(plan.rows)} commands, remaining guards, Brush/Hand continuity and atomic rollback passed.', flush=True)
    replay_without_dlls(cli, profile, output, cases, actual)
    metrics = dict(cases=len(cases), width=WIDTH, height=HEIGHT, masks=5, logical_a8_bytes=AREA*5,
                   synthetic_input=True, physical_tablet_tested=False,
                   journal_fault_injection='covered by Rust tests; no public transport fault hook',
                   archives={key: value['archive'] for key, value in actual.items()})
    if not args.skip_gui:
        captures = gui_checks(cli, gui, flags, profile, output, cases, actual)
        metrics['captures'] = dict(count=len(captures), png_bytes=sum(row['bytes'] for row in captures))
        print('GUI parity and four real multilingual copied-owner target captures passed.', flush=True)
    (output/'metrics.json').write_text(json.dumps(metrics, indent=2)+'\n', encoding='utf-8')
    print('All owner outcome archives verified without DLLs. Synthetic input only.', flush=True)
    print(output)


if __name__ == '__main__':
    main()
