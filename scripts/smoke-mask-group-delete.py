"""Verify subtree mask deletion, compound Undo and compact DLL-free outcomes.

Uses independently written tiny projects and tree/raw/composition references.
Synthetic contacts are not hardware tablet or throughput measurements.
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
spec = importlib.util.spec_from_file_location('mask_group_smoke', ROOT/'scripts/smoke-mask-groups.py')
groups = importlib.util.module_from_spec(spec)
spec.loader.exec_module(groups)
owners, geometry, editing, foundation, Plan = groups.owners, groups.geometry, groups.editing, groups.foundation, groups.Plan
WIDTH, HEIGHT, AREA = groups.WIDTH, groups.HEIGHT, groups.AREA
LANGUAGES = groups.LANGUAGES


def reference(case):
    original = case['model']
    result = copy.deepcopy(original)
    comp, before = result['composition'], original['composition']
    tree = comp['groups']
    consumed_groups, consumed_layers = set(), set()

    def consume(identity):
        consumed_groups.add(identity)
        for child in groups.group(tree, identity)['children']:
            if child['kind'] == 'group':
                consume(child['id'])
            else:
                consumed_layers.add(child['id'])

    consume(case['group'])
    for rows in [tree['roots'], *(row['children'] for row in tree['groups'])]:
        rows[:] = [row for row in rows if not (row['kind'] == 'group' and row['id'] in consumed_groups)]
    tree['groups'][:] = [row for row in tree['groups'] if row['id'] not in consumed_groups]
    if not tree['groups']:
        comp.pop('groups')
    before_ids = owners.ids(original)
    after_ids = [identity for identity in before_ids if identity not in consumed_layers]
    assert after_ids, 'Positive cases retain a raster'
    active = before_ids[original['active']]
    selected = before.get('selected_layer_ids') or [active]
    survivors = [identity for identity in selected if identity in after_ids]
    if active not in after_ids:
        if survivors:
            active = min(survivors, key=lambda identity: (abs(before_ids.index(identity)-original['active']), before_ids.index(identity)))
        else:
            active = next((identity for index, identity in enumerate(before_ids) if index >= original['active'] and identity in after_ids), after_ids[-1])
    if active not in survivors:
        survivors.append(active)
    comp.pop('selected_layer_ids', None)
    if len(survivors) > 1:
        comp['selected_layer_ids'] = [identity for identity in after_ids if identity in survivors]
    result['active'] = after_ids.index(active)
    comp['layer_metadata'] = [copy.deepcopy(before['layer_metadata'][before_ids.index(identity)]) for identity in after_ids]
    comp['layers'] = [copy.deepcopy(before['layers'][before_ids.index(identity)]) for identity in after_ids]
    result['rasters'] = [list(original['rasters'][before_ids.index(identity)]) for identity in after_ids]
    masks = comp['raster_masks']
    removed_masks = [row['id'] for row in masks['items'] if row['owner']['id'] in (consumed_groups if row['owner']['kind'] == 'group' else consumed_layers)]
    masks['items'][:] = [row for row in masks['items'] if row['id'] not in removed_masks]
    if masks.get('target', {}).get('id') in removed_masks:
        masks.pop('target')
    result['masks'] = {identity: raw for identity, raw in original['masks'].items() if identity not in removed_masks}
    result['result'] = dict(group_id=case['group'],
                            deleted_group_ids=[row['id'] for row in before['groups']['groups'] if row['id'] in consumed_groups],
                            deleted_layer_ids=[identity for identity in before_ids if identity in consumed_layers],
                            deleted_mask_ids=removed_masks)
    return result


def composed(model):
    value = copy.deepcopy(model)
    if 'groups' not in value['composition']:
        value['composition']['groups'] = dict(version=1, groups=[], roots=[groups.item('layer', identity) for identity in owners.ids(value)])
    return owners.composed(value)


def empty_group(model, masked=True):
    comp = model['composition']
    comp['groups']['roots'].append(groups.item('group', 3))
    comp['groups']['groups'].append(dict(id=3, name='Empty masked folder', visible=False, opacity=137, blend_mode='normal', children=[]))
    comp['next_group_id'] = 4
    if masked:
        comp['raster_masks']['items'].append(dict(id=6, owner=groups.item('group', 3), linked=False, density=177, feather_radius_px=2))
        comp['raster_masks']['next_id'] = 7
        model['masks'][6] = bytes((37*x+19*y+9) % 256 for y in range(HEIGHT) for x in range(WIDTH))


def cases(base):
    rows = []

    def add(name, identity, target, edit=None):
        model = copy.deepcopy(base)
        if edit:
            edit(model)
        masks = model['composition']['raster_masks']
        masks.pop('target', None)
        if target is not None:
            masks['target'] = groups.item('mask', target)
        rows.append(dict(name=name, group=identity, model=model))

    for identity in [1, 2]:
        for name, target in [('pixels', None), ('raster', 3), ('group', 4), ('unrelated', 1)]:
            add(f'delete-{identity}-{name}', identity, target)
    add('delete-inner-retain-parent-mask', 1, 5)
    for name, target in [('owned', 6), ('unrelated', 1), ('pixels', None)]:
        add('empty-'+name, 3, target, empty_group)

    def interleave(model):
        empty_group(model)
        rename = {1: 1, 2: 4, 3: 6, 4: 2, 5: 5, 6: 3}
        for row in model['composition']['raster_masks']['items']:
            row['id'] = rename[row['id']]
        model['composition']['raster_masks']['items'].sort(key=lambda row: row['id'])
        model['masks'] = {rename[identity]: raw for identity, raw in model['masks'].items()}

    add('noncontiguous-owned', 1, 2, interleave)
    add('noncontiguous-unrelated', 1, 3, interleave)

    def remove_paper_mask(model):
        model['composition']['raster_masks']['items'] = [row for row in model['composition']['raster_masks']['items'] if row['id'] != 1]
        model['masks'].pop(1)

    add('last-owner-masks', 2, 4, remove_paper_mask)
    add('empty-unmasked', 3, 1, lambda model: empty_group(model, False))
    return rows


def project(path, expected, original):
    saved = owners.project(path, expected, original, exact_allocators=True)
    assert saved['composition']['next_group_id'] == expected['composition']['next_group_id']
    if owners.ids(expected) == owners.ids(original):
        positions = {node['id']: node['position'] for node in saved['composition']['graph']['nodes']}
        assert all(positions[node['id']] == node['position'] for node in original['composition']['graph']['nodes'])
    return saved


def workflow():
    return dict(version=1, nodes=[
        dict(id=1, type='action.group.delete', position=[0, 0], parameters=dict(group_id=1)),
        dict(id=2, type='action.history.undo', position=[360, 0], parameters={}),
        dict(id=3, type='action.layer.selection.set', position=[720, 0], parameters={}),
    ], edges=[
        dict(**{'from': 1}, output='$exec', to=2, input='$exec'),
        dict(**{'from': 2}, output='$exec', to=3, input='$exec'),
        dict(**{'from': 1}, output='deleted_layer_ids', to=3, input='layer_ids'),
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
        add(key+'-operation', 'group.delete', group_id=float(case['group']) if key == 'noncontiguous-owned' else case['group'])
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
        dict(id='delete', action='group.delete', args=dict(group_id=1)),
        dict(id='undo', action='history.undo', args={}),
        dict(id='name', action='layer.rename', args=dict(layer_id={'$ref': 'delete#/result/deleted_layer_ids/0'}, name='Restored bound cel')),
        dict(id='mask', action='mask.fill', args=dict(mask_id={'$ref': 'delete#/result/deleted_mask_ids/0'}, value=64)),
    ])
    add('pipeline-save', 'document.save', path=str(output/f'{prefix}-pipeline.mcp'))
    add('pipeline-verify', 'replay.verify')
    add('workflow-open', 'document.open', path=str(chosen['path']))
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
    assert 'action.group.delete' in json.dumps(at('catalog')) and 'deleted_mask_ids' in json.dumps(at('catalog'))
    outcomes = {}
    for case in scenarios:
        key, source = case['name'], case['model']
        expected = reference(case)
        result = at(key+'-operation')
        for field, value in expected['result'].items():
            assert result[field] == value, (key, field, result[field], value)
        assert result['layers'][result['active_layer']]['id'] == owners.ids(expected)[expected['active']]
        assert at(key+'-after')['revision'] == at(key+'-before')['revision']+1, key
        before, after = at(key+'-events-before'), at(key+'-events-after')
        assert after['total'] == before['total']+1
        event = after['events'][-1]
        assert event['success'] and event['source']['action'] == 'group.delete'
        assert event['changed_mask_tiles'] == 0, 'Removal re-recorded unrelated raw A8'
        project(output/f'{prefix}-{key}.mcp', expected, source)
        project(output/f'{prefix}-{key}-undo.mcp', source, source)
        project(output/f'{prefix}-{key}-redo.mcp', expected, source)
        png, undo = (base64.b64decode(at(key+suffix)['data']) for suffix in ['-png', '-undo-png'])
        assert pixels(png) == (WIDTH, HEIGHT, composed(expected)), key
        assert pixels(undo) == (WIDTH, HEIGHT, composed(source)), key+': Undo'
        assert at(key+'-before-png')['data'] == at(key+'-undo-png')['data']
        assert at(key+'-redo-png')['data'] == at(key+'-reopen-png')['data'] == at(key+'-png')['data']
        assert at(key+'-verify')['valid']
        outcomes[key] = dict(png=png, undone_png=undo, seek=at(key+'-seek'), archive=foundation.archive_metrics(output/f'{prefix}-{key}.mcreplay'))
    source = scenarios[0]['model']
    expected = copy.deepcopy(source)
    expected['composition']['layer_metadata'][1]['name'] = 'Restored bound cel'
    expected['masks'][2] = bytes([64])*AREA
    project(output/f'{prefix}-pipeline.mcp', expected, source)
    assert at('pipeline-run')['ok'] and at('pipeline-verify')['valid']
    assert at('workflow-read')['graph'] == workflow()
    assert editing.document_state(at('workflow-before')) == editing.document_state(at('workflow-inert'))
    assert at('workflow-dry')['ok'] and at('workflow-run')['ok'] and at('workflow-verify')['valid']
    expected = copy.deepcopy(source)
    expected['composition']['workflow'] = workflow()
    expected['composition']['selected_layer_ids'] = [2, 3]
    project(output/f'{prefix}-workflow.mcp', expected, source)
    return outcomes


def negatives(plan, output, chosen, final_raster):
    add = plan.add
    add('negative-open', 'document.open', path=str(chosen['path']))
    add('negative-before', 'canvas.state')
    add('unknown', 'group.delete', error='group_edit', group_id=99999)
    add('negative-after', 'canvas.state')
    add('atomic-before', 'canvas.state')
    plan.failed_pipeline('atomic-failure', atomic=True, steps=[
        dict(id='delete', action='group.delete', args=dict(group_id=1)),
        dict(id='fail', action='mask.delete', args=dict(mask_id=99999)),
    ])
    add('atomic-after', 'canvas.state')
    add('atomic-save', 'document.save', path=str(output/'atomic-restored.mcp'))
    add('final-open', 'document.open', path=str(final_raster))
    add('final-before', 'canvas.state')
    add('last-raster', 'group.delete', error='group_edit', group_id=2)
    add('final-after', 'canvas.state')
    for tool in ['brush', 'hand']:
        add(tool+'-open', 'document.open', path=str(chosen['path']))
        add(tool+'-options', 'brush.set', radius=1, color='#000000', opacity=.8, stabilizer=.55, pressure_size=True, pressure_opacity=True)
        add(tool+'-down', 'canvas.pointer', phase='down', x=3, y=2, pressure=.4, time=100, pen=True, tool_override=tool)
        add(tool+'-before', 'canvas.state')
        add(tool+'-delete', 'group.delete', error='stroke_active', group_id=1)
        add(tool+'-after', 'canvas.state')
        add(tool+'-move', 'canvas.pointer', phase='move', x=5, y=3, pressure=.8, time=120, pen=True, tool_override=tool)
        add(tool+'-up', 'canvas.pointer', phase='up', x=5, y=3, pressure=0, time=130, pen=True, tool_override=tool)
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


def validate_negatives(values, plan, output, chosen):
    at = lambda key: plan.at(values, key)
    for key in ['negative', 'atomic', 'final', 'pending']:
        assert editing.document_state(at(key+'-before')) == editing.document_state(at(key+'-after')), key
    assert at('atomic-failure')['rolled_back'] and not at('atomic-failure')['ok']
    project(output/'atomic-restored.mcp', chosen['model'], chosen['model'])
    for tool in ['brush', 'hand']:
        assert editing.document_state(at(tool+'-before')) == editing.document_state(at(tool+'-after'))
        assert at(tool+'-after')['drawing'] == (tool == 'brush')
        project(output/f'{tool}-restored.mcp', chosen['model'], chosen['model'])
    assert at('negative-verify')['valid']


def gui_checks(cli, gui, flags, profile, output, scenarios, expected, final_raster):
    from capture_checks import CaptureChecks
    startup = subprocess.STARTUPINFO()
    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = 0
    process = subprocess.Popen([str(gui), *flags], startupinfo=startup)
    endpoint = profile/'session.json'

    def call(action, args):
        run = subprocess.run([str(cli), 'call', action, json.dumps(args), '--attach', str(endpoint)], capture_output=True, text=True, encoding='utf-8', timeout=30)
        assert run.returncode == 0, (action, run.stderr, run.stdout)
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
        negatives(plan, negative, scenarios[0], final_raster)
        values = editing.run_mcp(cli, ['--attach', str(endpoint)], plan)
        owners.parity(validate(values, plan, output, 'gui', scenarios), expected)
        validate_negatives(values, plan, negative, scenarios[0])
        call('document.open', dict(path=str(output/'gui-delete-1-unrelated-undo.mcp')))
        call('group.inspect', dict(group_id=1))
        artwork = call('canvas.snapshot', {})['data']
        checks.window('layers', 'float', x=70, y=70, width=440, height=820)
        _, size = checks.ready('layers', (440, 820))
        for language in LANGUAGES:
            call('settings.update', dict(path=str(profile/'settings.json'), ui=dict(language=language)))
            checks.settle_layout()
            assert call('canvas.state', {})['raster_masks']['target'] == dict(kind='mask', id=1)
            captures.append(dict(language=language, **checks.capture(output/f'{language}-restored-mask-subtree.png', 'layers', size=size)))
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
    output = ROOT/'target/test-output/mask-group-delete-smoke'/str(time.time_ns())
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
    base['origins'] = {identity: identity for identity in owners.ids(base)}
    scenarios = cases(base)
    for case in scenarios:
        case['path'] = output/(case['name']+'.mcp')
        geometry.write_model(case['path'], case['model'])
    final = copy.deepcopy(base)
    final['composition']['groups']['roots'] = [groups.item('group', 2)]
    groups.group(final['composition']['groups'], 2)['children'].insert(0, groups.item('layer', 1))
    final_raster = output/'final-raster.mcp'
    geometry.write_model(final_raster, final)
    plan = commands(output, 'jsonl', scenarios)
    values = geometry.run_jsonl(cli, flags, plan, output/'delete.jsonl')
    expected = validate(values, plan, output, 'jsonl', scenarios)
    print(f'JSONL: {len(scenarios)} subtree cases / {len(plan.rows)} commands passed independent raw/composition references.', flush=True)
    plan = commands(output, 'mcp', scenarios)
    negative = output/'mcp-boundaries'
    negative.mkdir()
    negatives(plan, negative, scenarios[0], final_raster)
    (output/'delete-mcp.jsonl').write_text('\n'.join(json.dumps(row) for row in plan.rows)+'\n', encoding='utf-8')
    values = editing.run_mcp(cli, flags, plan)
    actual = validate(values, plan, output, 'mcp', scenarios)
    owners.parity(actual, expected)
    validate_negatives(values, plan, negative, scenarios[0])
    print(f'MCP: {len(plan.rows)} commands, contacts, final-raster guards and atomic rollback passed.', flush=True)
    owners.replay_without_dlls(cli, profile, output, scenarios, actual)
    metrics = dict(cases=len(scenarios), width=WIDTH, height=HEIGHT,
                   changed_a8_tiles_per_delete=0, physical_tablet_tested=False, synthetic_input=True,
                   journal_fault_injection='covered by Rust tests; no public transport fault hook',
                   archives={key: value['archive'] for key, value in actual.items()})
    if not args.skip_gui:
        captures = gui_checks(cli, gui, flags, profile, output, scenarios, actual, final_raster)
        metrics['captures'] = dict(count=len(captures), png_bytes=sum(row['bytes'] for row in captures))
        print('GUI parity and four real multilingual restored-subtree captures passed.', flush=True)
    (output/'metrics.json').write_text(json.dumps(metrics, indent=2)+'\n', encoding='utf-8')
    print('All subtree outcomes verified without DLLs. Unrelated raw A8 is not repeated. Synthetic input only.', flush=True)
    print(output)


if __name__ == '__main__':
    main()
