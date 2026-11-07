"""Verify mask-bearing subtree copies through shared clients and DLL-free replay.

Independent tiny tree/byte references distinguish DFS raster allocation, BFS
group allocation and original mask order. Synthetic contacts do not certify
tablet hardware or measure painting throughput.
"""
import argparse
import base64
import copy
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import time

from png_checks import pixels

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('mask_delete_smoke', ROOT/'scripts/smoke-mask-group-delete.py')
deleted = importlib.util.module_from_spec(spec)
spec.loader.exec_module(deleted)
groups, owners = deleted.groups, deleted.owners
geometry, editing, foundation, Plan = deleted.geometry, deleted.editing, deleted.foundation, deleted.Plan
WIDTH, HEIGHT, AREA, LANGUAGES = deleted.WIDTH, deleted.HEIGHT, deleted.AREA, deleted.LANGUAGES
SENTINEL = 4294967296


def reference(case):
    source = case['model']
    result = copy.deepcopy(source)
    before, comp = source['composition'], result['composition']
    old_tree, tree = before['groups'], comp['groups']
    root = case['group']
    raster_ids = [row['id'] for row in groups.walk(old_tree, [groups.item('group', root)]) if row['kind'] == 'layer']
    group_ids = [root]
    for identity in group_ids:
        group_ids.extend(row['id'] for row in groups.group(old_tree, identity)['children'] if row['kind'] == 'group')
    layer_map = {identity: comp['next_layer_id']+index for index, identity in enumerate(raster_ids)}
    group_map = {identity: comp['next_group_id']+index for index, identity in enumerate(group_ids)}
    comp['next_layer_id'] += len(layer_map)
    comp['next_group_id'] += len(group_map)
    for identity in group_ids:
        row = copy.deepcopy(groups.group(old_tree, identity))
        row['id'] = group_map[identity]
        row['children'] = [groups.item(child['kind'], (layer_map if child['kind'] == 'layer' else group_map)[child['id']])
                           for child in row['children']]
        tree['groups'].append(row)
    siblings = groups.children(tree, groups.parent_of(old_tree, groups.item('group', root)))
    siblings.insert(siblings.index(groups.item('group', root))+1, groups.item('group', group_map[root]))
    reverse = {fresh: old for old, fresh in layer_map.items()}
    order = [row['id'] for row in groups.walk(tree) if row['kind'] == 'layer']
    original_ids = owners.ids(source)
    comp['layer_metadata'], comp['layers'], result['rasters'] = [], [], []
    for identity in order:
        origin = reverse.get(identity, identity)
        index = original_ids.index(origin)
        metadata = copy.deepcopy(before['layer_metadata'][index])
        metadata['id'] = identity
        comp['layer_metadata'].append(metadata)
        comp['layers'].append(copy.deepcopy(before['layers'][index]))
        result['rasters'].append(list(source['rasters'][index]))
        result['origins'][identity] = source['origins'][origin]
    active = original_ids[source['active']]
    selected = before.get('selected_layer_ids') or [active]
    if raster_ids:
        active = layer_map.get(active, layer_map[raster_ids[-1]])
        selected = list(layer_map.values())
    result['active'] = order.index(active)
    comp.pop('selected_layer_ids', None)
    if len(selected) > 1:
        comp['selected_layer_ids'] = [identity for identity in order if identity in selected]
    mask_ids = []
    for metadata in before['raster_masks']['items']:
        mapping = layer_map if metadata['owner']['kind'] == 'layer' else group_map
        if metadata['owner']['id'] not in mapping:
            continue
        copied = copy.deepcopy(metadata)
        copied['id'] = max(1, comp['raster_masks']['next_id'])
        copied['owner']['id'] = mapping[metadata['owner']['id']]
        comp['raster_masks']['next_id'] = copied['id']+1
        comp['raster_masks']['items'].append(copied)
        result['masks'][copied['id']] = bytes(source['masks'][metadata['id']])
        if owners.target(source) == groups.item('mask', metadata['id']):
            comp['raster_masks']['target'] = groups.item('mask', copied['id'])
        mask_ids.append(copied['id'])
    result['result'] = dict(source_group_id=root, group_id=group_map[root],
                            group_ids=list(group_map.values()), layer_ids=list(layer_map.values()), mask_ids=mask_ids)
    return result


def cases(base):
    rows = []

    def add(name, identity, target, edit=None):
        model = copy.deepcopy(base)
        if edit:
            edit(model)
        model['composition']['raster_masks'].pop('target', None)
        if target is not None:
            model['composition']['raster_masks']['target'] = groups.item('mask', target)
        rows.append(dict(name=name, group=identity, model=model))

    for name, identity in [('inner', 1), ('outer', 2)]:
        for suffix, target in [('pixels', None), ('raster', 3), ('group', 4), ('unrelated', 1)]:
            add(name+'-'+suffix, identity, target)
    add('inner-parent-target', 1, 5)

    def empty_masked(model):
        deleted.empty_group(model)
        model['composition']['next_layer_id'] = SENTINEL
    add('empty-owned-exhausted-raster', 3, 6, empty_masked)
    add('empty-unrelated', 3, 1, deleted.empty_group)

    def empty_unmasked(model):
        deleted.empty_group(model, False)
        model['composition']['next_layer_id'] = SENTINEL
        model['composition']['raster_masks']['next_id'] = SENTINEL
    add('empty-unmasked-exhausted-no-copy', 3, 1, empty_unmasked)

    def outside(model):
        model['active'] = 0
    add('outer-active-outside', 2, 3, outside)

    def interleaved(model):
        deleted.empty_group(model)
        rename = {1: 1, 2: 4, 3: 6, 4: 2, 5: 5, 6: 3}
        for row in model['composition']['raster_masks']['items']:
            row['id'] = rename[row['id']]
        model['composition']['raster_masks']['items'].sort(key=lambda row: row['id'])
        model['masks'] = {rename[identity]: raw for identity, raw in model['masks'].items()}
    add('interleaved-owned', 1, 2, interleaved)

    def disabled(model):
        model['composition']['graph']['enabled'] = False
        model['composition']['layers'][1][0] = True
    add('disabled-source-filters', 1, 2, disabled)

    def nested_empty(model):
        deleted.empty_group(model)
        tree = model['composition']['groups']
        tree['roots'].remove(groups.item('group', 3))
        groups.group(tree, 1)['children'].append(groups.item('group', 3))
        # A sibling makes allocation BFS differ from both storage/public DFS.
        groups.group(tree, 2)['children'].append(groups.item('group', 4))
        tree['groups'].append(dict(id=4, name='Sibling empty folder', visible=True,
                                   opacity=255, blend_mode='normal', children=[]))
        model['composition']['next_group_id'] = 5
    add('nested-empty-breadth-first', 2, 6, nested_empty)
    return rows


def undo_reference(source, copied):
    result = copy.deepcopy(source)
    for key in ['next_layer_id', 'next_group_id']:
        result['composition'][key] = copied['composition'][key]
    result['composition']['raster_masks']['next_id'] = copied['composition']['raster_masks']['next_id']
    return result


def project(path, expected, original):
    saved = owners.project(path, expected, original, exact_allocators=True)
    assert saved['composition']['next_group_id'] == expected['composition']['next_group_id']
    if owners.ids(expected) == owners.ids(original):
        positions = {node['id']: node['position'] for node in saved['composition']['graph']['nodes']}
        assert all(positions[node['id']] == node['position'] for node in original['composition']['graph']['nodes'])
    return saved


def workflow():
    return dict(version=1, nodes=[
        dict(id=1, type='action.group.duplicate', position=[0, 0], parameters=dict(group_id=1)),
        dict(id=2, type='action.group.set', position=[360, 0], parameters=dict(name='Schema copied folder')),
        dict(id=3, type='action.layer.selection.set', position=[720, 0], parameters={}),
    ], edges=[
        dict(**{'from': 1}, output='group_id', to=2, input='group_id'),
        dict(**{'from': 1}, output='layer_ids', to=3, input='layer_ids'),
        dict(**{'from': 2}, output='$exec', to=3, input='$exec'),
    ])


def commands(output, prefix, scenarios):
    plan = Plan()
    add = plan.add
    add('catalog', 'node.catalog')
    for case in scenarios:
        key = case['name']
        add(key+'-open', 'document.open', path=str(case['path']))
        add(key+'-before', 'canvas.state')
        add(key+'-before-png', 'canvas.snapshot')
        add(key+'-events-before', 'replay.events', limit=256)
        add(key+'-operation', 'group.duplicate', group_id=float(case['group']) if key == 'interleaved-owned' else case['group'])
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
    chosen = scenarios[0]
    copied = reference(chosen)
    add('independent-open', 'document.open', path=str(output/f'{prefix}-{chosen["name"]}.mcp'))
    add('independent-fill', 'mask.fill', mask_id=copied['result']['mask_ids'][-1], value=17)
    add('independent-save', 'document.save', path=str(output/f'{prefix}-independent.mcp'))
    add('independent-undo', 'history.undo')
    add('independent-png', 'canvas.snapshot')
    add('pipeline-open', 'document.open', path=str(chosen['path']))
    add('pipeline-run', 'pipeline.run', atomic=True, steps=[
        dict(id='copy', action='group.duplicate', args=dict(group_id=1)),
        dict(id='folder', action='group.set', args=dict(group_id={'$ref':'copy#/result/group_id'}, name='Bound copied folder')),
        dict(id='cel', action='layer.rename', args=dict(layer_id={'$ref':'copy#/result/layer_ids/0'}, name='Bound copied cel')),
        dict(id='mask', action='mask.fill', args=dict(mask_id={'$ref':'copy#/result/mask_ids/0'}, value=64)),
    ])
    add('pipeline-save', 'document.save', path=str(output/f'{prefix}-pipeline.mcp'))
    add('pipeline-verify', 'replay.verify')
    workflow_case = next(case for case in scenarios if case['name'] == 'inner-group')
    add('workflow-open', 'document.open', path=str(workflow_case['path']))
    add('workflow-set', 'workflow.set', graph=workflow())
    add('workflow-before', 'canvas.state')
    add('workflow-read', 'workflow.read')
    add('workflow-dry', 'workflow.run', atomic=True, dry_run=True)
    add('workflow-inert', 'canvas.state')
    add('workflow-run', 'workflow.run', atomic=True)
    add('workflow-save', 'document.save', path=str(output/f'{prefix}-workflow.mcp'))
    add('workflow-verify', 'replay.verify')
    return plan


def validate(values, plan, output, prefix, scenarios):
    at = lambda key: plan.at(values, key)
    catalog = json.dumps(at('catalog'))
    assert all(key in catalog for key in ['action.group.duplicate', 'source_group_id', 'group_ids', 'layer_ids', 'mask_ids'])
    outcomes = {}
    for case in scenarios:
        key, source = case['name'], case['model']
        expected = reference(case)
        operation, after = at(key+'-operation'), at(key+'-after')
        for field, wanted in expected['result'].items():
            assert operation[field] == wanted, (key, field, operation[field], wanted)
        assert operation['revision'] == after['revision'] == at(key+'-before')['revision']+1, key
        assert after['layers'][after['active_layer']]['id'] == owners.ids(expected)[expected['active']]
        selected = expected['composition'].get('selected_layer_ids') or [owners.ids(expected)[expected['active']]]
        assert after['selected_layer_ids'] == selected
        assert after['raster_masks']['target'] == owners.target(expected)
        before_events, after_events = at(key+'-events-before'), at(key+'-events-after')
        assert after_events['total'] == before_events['total']+1
        event = after_events['events'][-1]
        assert event['success'] and event['source']['action'] == 'group.duplicate'
        assert event['changed_mask_tiles'] == len(expected['result']['mask_ids']), (key, event)
        project(output/f'{prefix}-{key}.mcp', expected, source)
        project(output/f'{prefix}-{key}-undo.mcp', undo_reference(source, expected), source)
        project(output/f'{prefix}-{key}-redo.mcp', expected, source)
        png, undone = (base64.b64decode(at(key+suffix)['data']) for suffix in ['-png', '-undo-png'])
        assert pixels(png) == (WIDTH, HEIGHT, owners.composed(expected)), key
        assert pixels(undone) == (WIDTH, HEIGHT, owners.composed(source)), key+': Undo'
        assert at(key+'-before-png')['data'] == at(key+'-undo-png')['data']
        assert at(key+'-redo-png')['data'] == at(key+'-reopen-png')['data'] == at(key+'-png')['data']
        assert at(key+'-verify')['valid']
        outcomes[key] = dict(png=png, undone_png=undone, seek=at(key+'-seek'),
                             archive=foundation.archive_metrics(output/f'{prefix}-{key}.mcreplay'))
    source = scenarios[0]['model']
    expected = reference(scenarios[0])
    expected['masks'][expected['result']['mask_ids'][-1]] = bytes([17])*AREA
    project(output/f'{prefix}-independent.mcp', expected, source)
    assert at('independent-png')['data'] == at(scenarios[0]['name']+'-png')['data']
    expected = reference(scenarios[0])
    groups.group(expected['composition']['groups'], expected['result']['group_id'])['name'] = 'Bound copied folder'
    next(row for row in expected['composition']['layer_metadata'] if row['id'] == expected['result']['layer_ids'][0])['name'] = 'Bound copied cel'
    expected['masks'][expected['result']['mask_ids'][0]] = bytes([64])*AREA
    project(output/f'{prefix}-pipeline.mcp', expected, source)
    assert at('pipeline-run')['ok'] and at('pipeline-verify')['valid']
    assert at('workflow-read')['graph'] == workflow()
    assert editing.document_state(at('workflow-before')) == editing.document_state(at('workflow-inert'))
    assert at('workflow-dry')['ok'] and at('workflow-run')['ok'] and at('workflow-verify')['valid']
    workflow_case = next(case for case in scenarios if case['name'] == 'inner-group')
    expected = reference(workflow_case)
    expected['composition']['workflow'] = workflow()
    groups.group(expected['composition']['groups'], expected['result']['group_id'])['name'] = 'Schema copied folder'
    # Array selection without an explicit active ID preserves the copied mask target.
    project(output/f'{prefix}-workflow.mcp', expected, workflow_case['model'])
    return outcomes


def negatives(plan, output, chosen, exhausted):
    add = plan.add
    add('negative-open', 'document.open', path=str(chosen['path']))
    add('negative-before', 'canvas.state')
    add('unknown', 'group.duplicate', error='group_edit', group_id=99999)
    add('negative-after', 'canvas.state')
    add('atomic-before', 'canvas.state')
    plan.failed_pipeline('atomic-failure', atomic=True, steps=[
        dict(id='copy', action='group.duplicate', args=dict(group_id=1)),
        dict(id='fail', action='mask.delete', args=dict(mask_id=99999)),
    ])
    add('atomic-after', 'canvas.state')
    add('atomic-save', 'document.save', path=str(output/'atomic-restored.mcp'))
    for kind, path in exhausted.items():
        add(kind+'-open', 'document.open', path=str(path))
        add(kind+'-before', 'canvas.state')
        add(kind+'-reject', 'group.duplicate', error='group_edit', group_id=1)
        add(kind+'-after', 'canvas.state')
    for tool in ['brush', 'hand']:
        add(tool+'-open', 'document.open', path=str(chosen['path']))
        add(tool+'-options', 'brush.set', radius=1, color='#000000', opacity=.8, stabilizer=.55, pressure_size=True, pressure_opacity=True)
        add(tool+'-down', 'canvas.pointer', phase='down', x=3, y=2, pressure=.4, time=100, pen=True, tool_override=tool)
        add(tool+'-before', 'canvas.state')
        add(tool+'-raw-before', 'mask.snapshot', mask_id=1, width=WIDTH, height=HEIGHT)
        add(tool+'-copy', 'group.duplicate', error='stroke_active', group_id=1)
        add(tool+'-after', 'canvas.state')
        add(tool+'-raw-after', 'mask.snapshot', mask_id=1, width=WIDTH, height=HEIGHT)
        add(tool+'-move', 'canvas.pointer', phase='move', x=5, y=3, pressure=.8, time=120, pen=True, tool_override=tool)
        add(tool+'-up', 'canvas.pointer', phase='up', x=5, y=3, pressure=0, time=130, pen=True, tool_override=tool)
        add(tool+'-events', 'replay.events', limit=256)
        if tool == 'brush':
            add(tool+'-undo', 'history.undo')
        add(tool+'-save', 'document.save', path=str(output/f'{tool}-restored.mcp'))
    add('pending-open', 'document.open', path=str(chosen['path']))
    add('pending-pixels', 'mask.target', target=dict(kind='layer_pixels'))
    add('pending-before', 'canvas.state')
    for index, (action, args) in enumerate(editing.boundary_cases()):
        add(f'pending-{index}', action, error='mask_editing_pending', **args)
    add('pending-after', 'canvas.state')
    add('negative-verify', 'replay.verify')


def validate_negatives(values, plan, output, chosen, exhausted):
    at = lambda key: plan.at(values, key)
    for key in ['negative', 'atomic', 'pending', *exhausted]:
        assert editing.document_state(at(key+'-before')) == editing.document_state(at(key+'-after')), key
    assert at('atomic-failure')['rolled_back'] and not at('atomic-failure')['ok']
    project(output/'atomic-restored.mcp', chosen['model'], chosen['model'])
    for tool in ['brush', 'hand']:
        assert editing.document_state(at(tool+'-before')) == editing.document_state(at(tool+'-after'))
        assert at(tool+'-raw-before') == at(tool+'-raw-after')
        assert at(tool+'-after')['drawing'] == (tool == 'brush')
        samples = [sample for event in at(tool+'-events')['events'] if event['source']['kind'] == 'pointer'
                   for sample in event['source']['samples']]
        assert [sample['time'] for sample in samples] == [100, 120, 130]
        assert all(sample['pen'] for sample in samples)
        assert all(abs(sample['pressure']-pressure) < 1e-6 for sample, pressure in zip(samples, [.4, .8, 0]))
        project(output/f'{tool}-restored.mcp', chosen['model'], chosen['model'])
    assert at('negative-verify')['valid']


def gui_checks(cli, gui, flags, profile, output, scenarios, expected, exhausted):
    from capture_checks import CaptureChecks
    startup = subprocess.STARTUPINFO()
    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = 0
    process = subprocess.Popen([str(gui), *flags], startupinfo=startup)
    endpoint = profile/'session.json'

    def call(action_name, args):
        run = subprocess.run([str(cli), 'call', action_name, json.dumps(args), '--attach', str(endpoint)],
                             capture_output=True, text=True, encoding='utf-8', timeout=30)
        assert run.returncode == 0, (action_name, args, run.stderr, run.stdout)
        return json.loads(run.stdout)['result']

    checks, captures = CaptureChecks(call, process), []
    try:
        deadline = time.monotonic()+30
        while not endpoint.exists():
            assert process.poll() is None and time.monotonic() < deadline, 'GUI startup failed'
            time.sleep(.05)
        checks.settle_layout()
        plan = commands(output, 'gui', scenarios)
        negative = output/'gui-boundaries'
        negative.mkdir()
        negatives(plan, negative, scenarios[0], exhausted)
        values = editing.run_mcp(cli, ['--attach', str(endpoint)], plan)
        owners.parity(validate(values, plan, output, 'gui', scenarios), expected)
        validate_negatives(values, plan, negative, scenarios[0], exhausted)
        case = next(row for row in scenarios if row['name'] == 'inner-raster')
        copied = reference(case)
        call('document.open', dict(path=str(output/'gui-inner-raster.mcp')))
        call('group.inspect', dict(group_id=copied['result']['group_id']))
        artwork = call('canvas.snapshot', {})['data']
        checks.window('layers', 'float', x=70, y=70, width=440, height=820)
        _, size = checks.ready('layers', (440, 820))
        for language in LANGUAGES:
            call('settings.update', dict(path=str(profile/'settings.json'), ui=dict(language=language)))
            stored = json.loads((profile/'settings.json').read_text(encoding='utf-8-sig'))
            assert stored['ui']['language'] == language and stored['custom']['keep'] == 42
            checks.settle_layout()
            assert call('canvas.state', {})['raster_masks']['target'] == owners.target(copied)
            captures.append(dict(language=language, **checks.capture(output/f'{language}-copied-mask-subtree.png', 'layers', size=size)))
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
    assert cli.is_file() and (args.skip_gui or gui.is_file()), 'Build/stage matching host and DLLs first'
    output = ROOT/'target/test-output/mask-group-duplicate-smoke'/str(time.time_ns())
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
    next(row for row in base['composition']['raster_masks']['items'] if row['id'] == 3).update(
        density=177, feather_radius_px=2, linked=False)
    base['origins'] = {identity: identity for identity in owners.ids(base)}
    scenarios = cases(base)
    for case in scenarios:
        case['path'] = output/(case['name']+'.mcp')
        geometry.write_model(case['path'], case['model'])
    exhausted = {}
    for key in ['group', 'layer', 'mask']:
        model = copy.deepcopy(base)
        if key == 'mask':
            model['composition']['raster_masks']['next_id'] = SENTINEL
        else:
            model['composition']['next_'+key+'_id'] = SENTINEL
        path = output/('exhausted-'+key+'.mcp')
        geometry.write_model(path, model)
        exhausted['exhausted-'+key] = path
    plan = commands(output, 'jsonl', scenarios)
    values = geometry.run_jsonl(cli, flags, plan, output/'duplicate.jsonl')
    expected = validate(values, plan, output, 'jsonl', scenarios)
    print(f'JSONL: {len(scenarios)} subtree-copy cases / {len(plan.rows)} commands passed independent references.', flush=True)
    plan = commands(output, 'mcp', scenarios)
    negative = output/'mcp-boundaries'
    negative.mkdir()
    negatives(plan, negative, scenarios[0], exhausted)
    (output/'duplicate-mcp.jsonl').write_text('\n'.join(json.dumps(row) for row in plan.rows)+'\n', encoding='utf-8')
    values = editing.run_mcp(cli, flags, plan)
    actual = validate(values, plan, output, 'mcp', scenarios)
    owners.parity(actual, expected)
    validate_negatives(values, plan, negative, scenarios[0], exhausted)
    print(f'MCP: {len(plan.rows)} commands, allocator/contact guards and atomic rollback passed.', flush=True)
    owners.replay_without_dlls(cli, profile, output, scenarios, actual)
    metrics = dict(cases=len(scenarios), width=WIDTH, height=HEIGHT,
                   changed_a8_tiles='Only fresh copied mask IDs; one tile per tiny plane.',
                   physical_tablet_tested=False, synthetic_input=True,
                   journal_fault_injection='covered by Rust tests; no public transport fault hook',
                   archives={key: value['archive'] for key, value in actual.items()})
    if not args.skip_gui:
        captures = gui_checks(cli, gui, flags, profile, output, scenarios, actual, exhausted)
        metrics['captures'] = dict(count=len(captures), png_bytes=sum(row['bytes'] for row in captures))
        print('GUI parity and four real multilingual copied-mask captures passed.', flush=True)
    (output/'metrics.json').write_text(json.dumps(metrics, indent=2)+'\n', encoding='utf-8')
    print('All copied outcomes verified without DLLs. Unchanged raw A8 is not repeated. Synthetic input only.', flush=True)
    print(output)


if __name__ == '__main__':
    main()
