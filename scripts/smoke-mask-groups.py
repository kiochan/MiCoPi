"""Verify mask-preserving group wrapping and reparenting through shared clients.

Independent tiny tree/byte/integer references distinguish preserved raw ownership
from changed composition. Synthetic contacts do not certify tablet hardware.
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
spec = importlib.util.spec_from_file_location('mask_owner_smoke', ROOT/'scripts/smoke-mask-owners.py')
owners = importlib.util.module_from_spec(spec)
spec.loader.exec_module(owners)
geometry, editing, foundation, Plan = owners.geometry, owners.editing, owners.foundation, owners.Plan
WIDTH, HEIGHT, AREA = owners.WIDTH, owners.HEIGHT, owners.AREA
LANGUAGES = ['en', 'zh-CN', 'ja', 'ko']
SUPPORTED = {'group.create', 'group.create_selected', 'group.move'}


def item(kind, identity):
    return dict(kind=kind, id=identity)


def group(tree, identity):
    return next(row for row in tree['groups'] if row['id'] == identity)


def children(tree, parent):
    return tree['roots'] if parent == 0 else group(tree, parent)['children']


def walk(tree, rows=None):
    for row in tree['roots'] if rows is None else rows:
        yield row
        if row['kind'] == 'group':
            yield from walk(tree, group(tree, row['id'])['children'])


def normalized(rows):
    return [item(row['kind'], int(row['id'])) for row in rows]


def parent_of(tree, wanted):
    for parent in [0, *(row['id'] for row in tree['groups'])]:
        if wanted in children(tree, parent):
            return parent
    raise AssertionError('Missing independent tree item')


def reference(case):
    original = case['model']
    result = copy.deepcopy(original)
    comp = result['composition']
    tree, args = comp['groups'], case['args']
    active = owners.ids(original)[original['active']]
    selected = comp.get('selected_layer_ids') or [active]
    if case['action'] in ['group.create', 'group.create_selected']:
        members = ([item('layer', identity) for identity in selected]
                   if case['action'] == 'group.create_selected' else normalized(args['members']))
        parent = int(args.get('parent_id', parent_of(tree, members[0]) if members else 0))
        siblings = children(tree, parent)
        contained = [row for row in siblings if row in members]
        position = next((index for index, row in enumerate(siblings) if row in members), len(siblings))
        siblings[:] = [row for row in siblings if row not in members]
        identity = comp['next_group_id']
        siblings.insert(position, item('group', identity))
        tree['groups'].append(dict(id=identity, name=args.get('name', f'Group {identity:02}'),
                                   visible=True, opacity=255, blend_mode='normal', children=contained))
        comp['next_group_id'] += 1
        result['group_id'] = identity
    else:
        wanted = normalized(args['items'])
        ordered = [row for row in walk(tree) if row in wanted]
        for values in [tree['roots'], *(row['children'] for row in tree['groups'])]:
            values[:] = [row for row in values if row not in wanted]
        destination = children(tree, int(args['parent_id']))
        index = int(args['index'])
        destination[index:index] = ordered
    order = [row['id'] for row in walk(tree) if row['kind'] == 'layer']
    before_ids = owners.ids(original)
    comp['layer_metadata'] = [copy.deepcopy(original['composition']['layer_metadata'][before_ids.index(identity)]) for identity in order]
    comp['layers'] = [copy.deepcopy(original['composition']['layers'][before_ids.index(identity)]) for identity in order]
    result['rasters'] = [list(original['rasters'][before_ids.index(identity)]) for identity in order]
    result['active'] = order.index(active)
    selected = [identity for identity in order if identity in selected]
    comp.pop('selected_layer_ids', None)
    if len(selected) > 1:
        comp['selected_layer_ids'] = selected
    return result


def cases(base):
    values = []

    def add(name, action, args, edit=None):
        model = copy.deepcopy(base)
        if edit:
            edit(model)
        values.append(dict(name=name, action=action, args=args, model=model))

    def selected_pair(model):
        model['composition']['selected_layer_ids'] = [2, 3]

    def pixels_target(model):
        model['composition']['raster_masks'].pop('target', None)

    def group_target(model):
        model['composition']['raster_masks']['target'] = item('mask', 4)

    def hidden_target(model):
        model['composition']['raster_masks']['target'] = item('mask', 2)

    def backdrop(model):
        group(model['composition']['groups'], 2)['compositing'] = 'pass_through'

    add('wrap-root', 'group.create', dict(members=[item('layer', 1), item('group', 2)], parent_id=0, name='Masked folder'))
    add('empty-root', 'group.create', dict(members=[], parent_id=0, name='Empty folder'), group_target)
    add('empty-inner', 'group.create', dict(members=[], parent_id=1, name='Empty child'), pixels_target)
    add('wrap-inner-floats', 'group.create', dict(members=[item('layer', 2.0), item('layer', 3.0)], parent_id=1.0, name='Float selectors'))
    add('wrap-backdrop', 'group.create', dict(members=[item('layer', 3)], name='Isolated wrapper'), backdrop)
    add('wrap-selected', 'group.create_selected', dict(name='Selected folder'), selected_pair)

    def disabled_filters(model):
        model['composition']['graph']['enabled'] = False
        model['composition']['layers'][1][0] = True
    add('wrap-disabled-filters', 'group.create', dict(members=[item('layer', 2), item('layer', 3)], parent_id=1, name='Retained hierarchy'), disabled_filters)
    add('paper-into-inner', 'group.move', dict(items=[item('layer', 1)], parent_id=1, index=0))
    add('visible-to-root-floats', 'group.move', dict(items=[item('layer', 3.0)], parent_id=0.0, index=1.0), group_target)
    add('hidden-to-root', 'group.move', dict(items=[item('layer', 2)], parent_id=0, index=0), hidden_target)
    add('masked-group-to-root', 'group.move', dict(items=[item('group', 1)], parent_id=0, index=1), group_target)
    add('folder-before-paper', 'group.move', dict(items=[item('group', 2)], parent_id=0, index=0))
    add('mixed-source-order', 'group.move', dict(items=[item('layer', 3), item('layer', 1)], parent_id=0, index=0))

    def hidden_parent(model):
        group(model['composition']['groups'], 2)['visible'] = False
    add('exit-hidden-parent', 'group.move', dict(items=[item('layer', 3)], parent_id=0, index=1), hidden_parent)

    def exhausted(model):
        model['composition']['next_group_id'] = 4294967296
    add('move-exhausted-allocator', 'group.move', dict(items=[item('group', 1)], parent_id=0, index=1), exhausted)
    return values


def project(path, expected, original, exact=False):
    saved = foundation.read_project(path)
    desired = copy.deepcopy(expected)
    actual_next = saved['composition']['next_group_id']
    expected_next = expected['composition']['next_group_id']
    assert actual_next == expected_next if exact else actual_next >= expected_next
    assert saved['composition']['next_layer_id'] == expected['composition']['next_layer_id']
    assert saved['composition']['raster_masks']['next_id'] == expected['composition']['raster_masks']['next_id']
    if owners.ids(expected) == owners.ids(original):
        positions = {node['id']: node['position'] for node in saved['composition']['graph']['nodes']}
        assert all(positions[node['id']] == node['position'] for node in original['composition']['graph']['nodes'])
    desired['composition']['next_group_id'] = actual_next
    return owners.project(path, desired, original, exact_allocators=True)


def workflow():
    return dict(version=1, nodes=[
        dict(id=1, type='action.group.create', position=[0, 0], parameters=dict(members=[], parent_id=0, name='Schema folder')),
        dict(id=2, type='action.group.move', position=[380, 0], parameters=dict(items=[item('layer', 3)], index=0)),
    ], edges=[dict(**{'from': 1}, output='group_id', to=2, input='parent_id')])


def bound_reference(model):
    created = reference(dict(model=model, action='group.create', args=dict(members=[], parent_id=0, name='Schema folder')))
    moved = reference(dict(model=created, action='group.move', args=dict(items=[item('layer', 3)], parent_id=created['group_id'], index=0)))
    return moved


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
        add(key+'-operation', case['action'], **case['args'])
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
    add('pipeline-open', 'document.open', path=str(chosen['path']))
    add('pipeline-run', 'pipeline.run', atomic=True, steps=[
        dict(id='folder', action='group.create', args=dict(members=[], parent_id=0, name='Schema folder')),
        dict(id='move', action='group.move', args=dict(items=[item('layer', 3)], parent_id={'$ref': 'folder#/result/group_id'}, index=0)),
    ])
    add('pipeline-save', 'document.save', path=str(output/f'{prefix}-pipeline.mcp'))
    add('pipeline-verify', 'replay.verify')
    add('workflow-open', 'document.open', path=str(chosen['path']))
    add('workflow-before-png', 'canvas.snapshot')
    add('workflow-set', 'workflow.set', graph=workflow())
    add('workflow-read', 'workflow.read')
    add('workflow-before', 'canvas.state')
    add('workflow-dry', 'workflow.run', atomic=True, dry_run=True)
    add('workflow-after', 'canvas.state')
    add('workflow-inert-png', 'canvas.snapshot')
    add('workflow-run', 'workflow.run', atomic=True)
    add('workflow-save', 'document.save', path=str(output/f'{prefix}-workflow.mcp'))
    add('workflow-verify', 'replay.verify')
    return plan


def validate(values, plan, output, prefix, scenarios):
    at = lambda key: plan.at(values, key)
    catalog = json.dumps(at('catalog'))
    assert all('action.'+action in catalog for action in SUPPORTED)
    results = {}
    for case in scenarios:
        key, source = case['name'], case['model']
        expected = reference(case)
        operation, after = at(key+'-operation'), at(key+'-after')
        if case['action'] != 'group.move':
            assert operation['group_id'] == expected['group_id']
        assert after['layers'][after['active_layer']]['id'] == owners.ids(source)[source['active']]
        selected = source['composition'].get('selected_layer_ids') or [owners.ids(source)[source['active']]]
        assert set(after['selected_layer_ids']) == set(selected)
        assert after['raster_masks']['items'] == at(key+'-before')['raster_masks']['items']
        assert after['raster_masks']['target'] == at(key+'-before')['raster_masks']['target']
        assert after['revision'] == at(key+'-before')['revision']+1
        before_events, after_events = at(key+'-events-before'), at(key+'-events-after')
        assert after_events['total'] == before_events['total']+1
        event = after_events['events'][-1]
        assert event['success'] and event['source']['action'] == case['action']
        assert event['changed_mask_tiles'] == 0, 'Owner-preserving tree edits re-recorded unchanged raw A8'
        project(output/f'{prefix}-{key}.mcp', expected, source)
        project(output/f'{prefix}-{key}-undo.mcp', source, source)
        project(output/f'{prefix}-{key}-redo.mcp', expected, source)
        png = base64.b64decode(at(key+'-png')['data'])
        assert pixels(png) == (WIDTH, HEIGHT, owners.composed(expected)), key
        undone = base64.b64decode(at(key+'-undo-png')['data'])
        assert pixels(undone) == (WIDTH, HEIGHT, owners.composed(source)), key+': Undo'
        assert at(key+'-before-png')['data'] == at(key+'-undo-png')['data']
        assert at(key+'-redo-png')['data'] == at(key+'-reopen-png')['data'] == at(key+'-png')['data']
        assert at(key+'-verify')['valid']
        results[key] = dict(png=png, undone_png=undone, seek=at(key+'-seek'),
                            archive=foundation.archive_metrics(output/f'{prefix}-{key}.mcreplay'))
    chosen = scenarios[0]
    expected = bound_reference(chosen['model'])
    project(output/f'{prefix}-pipeline.mcp', expected, chosen['model'])
    assert at('pipeline-run')['ok'] and at('pipeline-verify')['valid']
    assert at('workflow-read')['graph'] == workflow()
    assert editing.document_state(at('workflow-before')) == editing.document_state(at('workflow-after'))
    assert at('workflow-before-png')['data'] == at('workflow-inert-png')['data']
    assert at('workflow-dry')['ok'] and at('workflow-run')['ok']
    expected['composition']['workflow'] = workflow()
    project(output/f'{prefix}-workflow.mcp', expected, chosen['model'])
    assert at('workflow-verify')['valid']
    return results


def negatives(plan, output, chosen, exhausted):
    add = plan.add
    add('negative-open', 'document.open', path=str(chosen['path']))
    add('negative-before', 'canvas.state')
    for key, action, args in [
        ('duplicate-member', 'group.create', dict(members=[item('layer', 1), item('layer', 1.0)], parent_id=0)),
        ('duplicate-move', 'group.move', dict(items=[item('layer', 1), item('layer', 1.0)], parent_id=0, index=0)),
        ('cross-parent', 'group.create_selected', {}),
        ('cycle', 'group.move', dict(items=[item('group', 2)], parent_id=1, index=0)),
        ('ancestor', 'group.move', dict(items=[item('group', 2), item('layer', 3)], parent_id=0, index=0)),
        ('destination', 'group.move', dict(items=[item('layer', 3)], parent_id=0, index=3)),
        ('wrong-parent', 'group.create', dict(members=[item('layer', 3)], parent_id=0.0)),
    ]:
        add(key, action, error='group_edit', **args)
    add('negative-after', 'canvas.state')
    add('atomic-before', 'canvas.state')
    plan.failed_pipeline('atomic-failure', atomic=True, steps=[
        dict(id='folder', action='group.create', args=dict(members=[item('layer', 1)], parent_id=0)),
        dict(id='fail', action='group.move', args=dict(items=[item('layer', 3)], parent_id=99999, index=0)),
    ])
    add('atomic-after', 'canvas.state')
    add('atomic-save', 'document.save', path=str(output/'atomic-restored.mcp'))
    add('exhausted-open', 'document.open', path=str(exhausted['path']))
    add('exhausted-before', 'canvas.state')
    add('exhausted-create', 'group.create', error='group_edit', members=[], parent_id=0)
    add('exhausted-after', 'canvas.state')
    for tool in ['brush', 'hand']:
        add(tool+'-open', 'document.open', path=str(chosen['path']))
        add(tool+'-options', 'brush.set', radius=1, color='#000000', opacity=.8, stabilizer=.55,
            pressure_size=True, pressure_opacity=True)
        add(tool+'-down', 'canvas.pointer', phase='down', x=3, y=2, pressure=.4, time=100, pen=True, tool_override=tool)
        add(tool+'-before', 'canvas.state')
        add(tool+'-raw-before', 'mask.snapshot', mask_id=1, width=WIDTH, height=HEIGHT)
        add(tool+'-create', 'group.create', error='stroke_active', members=[item('layer', 1)], parent_id=0)
        add(tool+'-selected', 'group.create_selected', error='stroke_active')
        add(tool+'-move-rejected', 'group.move', error='stroke_active', items=[item('layer', 3)], parent_id=0, index=1)
        add(tool+'-after', 'canvas.state')
        add(tool+'-raw-after', 'mask.snapshot', mask_id=1, width=WIDTH, height=HEIGHT)
        add(tool+'-move', 'canvas.pointer', phase='move', x=5, y=3, pressure=.8, time=120, pen=True, tool_override=tool)
        add(tool+'-up', 'canvas.pointer', phase='up', x=5, y=3, pressure=0, time=130, pen=True, tool_override=tool)
        if tool == 'brush':
            add(tool+'-undo', 'history.undo')
        add(tool+'-save', 'document.save', path=str(output/f'{tool}-restored.mcp'))
    add('pending-open', 'document.open', path=str(chosen['path']))
    add('pending-pixels', 'mask.target', target=dict(kind='layer_pixels'))
    add('pending-before', 'canvas.state')
    for index, (action, args) in enumerate(editing.boundary_cases()):
        if action not in SUPPORTED:
            add(f'pending-{index}', action, error='mask_editing_pending', **args)
    add('pending-after', 'canvas.state')
    add('negative-verify', 'replay.verify')


def validate_negatives(values, plan, output, chosen):
    at = lambda key: plan.at(values, key)
    for first, second in [('negative-before', 'negative-after'), ('atomic-before', 'atomic-after'),
                          ('exhausted-before', 'exhausted-after'), ('pending-before', 'pending-after')]:
        assert editing.document_state(at(first)) == editing.document_state(at(second)), (first, second)
    assert at('atomic-failure')['rolled_back'] and not at('atomic-failure')['ok']
    project(output/'atomic-restored.mcp', chosen['model'], chosen['model'], exact=True)
    for tool in ['brush', 'hand']:
        assert editing.document_state(at(tool+'-before')) == editing.document_state(at(tool+'-after'))
        assert at(tool+'-raw-before') == at(tool+'-raw-after')
        assert at(tool+'-after')['drawing'] == (tool == 'brush')
        project(output/f'{tool}-restored.mcp', chosen['model'], chosen['model'])
    assert at('negative-verify')['valid']


def gui_checks(cli, gui, flags, profile, output, scenarios, expected):
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
        plan = commands(output, 'gui', scenarios)
        negative = output/'gui-boundaries'
        negative.mkdir()
        negatives(plan, negative, scenarios[0], scenarios[-1])
        values = editing.run_mcp(cli, ['--attach', str(endpoint)], plan)
        owners.parity(validate(values, plan, output, 'gui', scenarios), expected)
        validate_negatives(values, plan, negative, scenarios[0])
        call('document.open', dict(path=str(output/'gui-wrap-root.mcp')))
        call('group.inspect', dict(group_id=3))
        artwork = call('canvas.snapshot', {})['data']
        checks.window('layers', 'float', x=70, y=70, width=440, height=820)
        _, size = checks.ready('layers', (440, 820))
        for language in LANGUAGES:
            call('settings.update', dict(path=str(profile/'settings.json'), ui=dict(language=language)))
            stored = json.loads((profile/'settings.json').read_text(encoding='utf-8-sig'))
            assert stored['ui']['language'] == language and stored['custom']['keep'] == 42
            checks.settle_layout()
            state = call('canvas.state', {})
            assert state['layers'][state['active_layer']]['id'] == 3
            assert state['raster_masks']['target'] == dict(kind='mask', id=1)
            captures.append(dict(language=language, **checks.capture(output/f'{language}-masked-group-wrap.png', 'layers', size=size)))
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
    output = ROOT/'target/test-output/mask-groups-smoke'/str(time.time_ns())
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
    next(row for row in base['composition']['raster_masks']['items'] if row['id'] == 3).update(density=177, feather_radius_px=2, linked=False)
    base['origins'] = {identity: identity for identity in owners.ids(base)}
    scenarios = cases(base)
    for case in scenarios:
        case['path'] = output/(case['name']+'.mcp')
        geometry.write_model(case['path'], case['model'])
    plan = commands(output, 'jsonl', scenarios)
    values = geometry.run_jsonl(cli, flags, plan, output/'groups.jsonl')
    expected = validate(values, plan, output, 'jsonl', scenarios)
    print(f'JSONL: {len(scenarios)} tree cases / {len(plan.rows)} commands passed independent raw/composition references.', flush=True)
    plan = commands(output, 'mcp', scenarios)
    negative = output/'mcp-boundaries'
    negative.mkdir()
    negatives(plan, negative, scenarios[0], scenarios[-1])
    (output/'groups-mcp.jsonl').write_text('\n'.join(json.dumps(row) for row in plan.rows)+'\n', encoding='utf-8')
    values = editing.run_mcp(cli, flags, plan)
    actual = validate(values, plan, output, 'mcp', scenarios)
    owners.parity(actual, expected)
    validate_negatives(values, plan, negative, scenarios[0])
    print(f'MCP: {len(plan.rows)} commands, numeric/shape/contact guards and atomic rollback passed.', flush=True)
    owners.replay_without_dlls(cli, profile, output, scenarios, actual)
    metrics = dict(cases=len(scenarios), width=WIDTH, height=HEIGHT, masks=5, logical_a8_bytes=AREA*5,
                   synthetic_input=True, physical_tablet_tested=False, changed_a8_tiles_per_tree_action=0,
                   journal_fault_injection='covered by Rust tests; no public transport fault hook',
                   archives={key: value['archive'] for key, value in actual.items()})
    if not args.skip_gui:
        captures = gui_checks(cli, gui, flags, profile, output, scenarios, actual)
        metrics['captures'] = dict(count=len(captures), png_bytes=sum(row['bytes'] for row in captures))
        print('GUI parity and four real multilingual group/paint-target captures passed.', flush=True)
    (output/'metrics.json').write_text(json.dumps(metrics, indent=2)+'\n', encoding='utf-8')
    print('All tree outcome archives verified without DLLs; raw mask ownership retained. Synthetic input only.', flush=True)
    print(output)


if __name__ == '__main__':
    main()
