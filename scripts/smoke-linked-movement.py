"""Verify persistent raster links and explicit multi-owner Move on tiny images.

The independent inverse-coordinate oracle checks raw RGBA/A8, unchanged pixel
selections and metadata separately from composition. JSONL and MCP share the
same references; optional desktop attachment runs one owned hidden process.
Synthetic contacts do not certify physical tablets or measure throughput.
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
from capture_checks import CaptureChecks

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('owner_move_smoke', ROOT/'scripts/smoke-owner-movement.py')
movement = importlib.util.module_from_spec(spec)
spec.loader.exec_module(movement)
owners, geometry, editing, foundation, Plan = movement.owners, movement.geometry, movement.editing, movement.foundation, movement.Plan
WIDTH, HEIGHT, AREA = movement.WIDTH, movement.HEIGHT, movement.AREA
LANGUAGES = movement.LANGUAGES


def memberships(model, sets):
    """Reference only: each retained member stores the same full sorted list."""
    for row in model['composition']['layer_metadata']:
        row.pop('links', None)
        for members in sets:
            if row['id'] in members:
                row['links'] = dict(version=1, layer_ids=sorted(members))


def linked_model(model):
    result = copy.deepcopy(model)
    memberships(result, [[1, 2, 3]])
    return result


def shifted(data, dx, dy, exterior):
    # Each destination samples its source; no native helper is used by the oracle.
    return [data[(y-dy)*WIDTH+x-dx] if 0 <= x-dx < WIDTH and 0 <= y-dy < HEIGHT else exterior
            for y in range(HEIGHT) for x in range(WIDTH)]


def reference(case):
    result = linked_model(case['model'])
    ids = owners.ids(result)
    masks = []
    for identity in case['ids']:
        index = ids.index(identity)
        result['rasters'][index] = shifted(result['rasters'][index], case['dx'], case['dy'], 0)
        owned = owners.mask_for(result, identity)
        if owned and owned.get('linked', True):
            masks.append(owned['id'])
            result['masks'][owned['id']] = bytes(shifted(result['masks'][owned['id']], case['dx'], case['dy'], 255))
    result['movement_context'] = dict(target='layers', layer_ids=case['ids'], linked_mask_ids=masks,
                                      dx=case['dx'], dy=case['dy'])
    return result


def cases(base):
    rows = [
        dict(name='cross-parent-exact-subset', ids=[3, 1], dx=2, dy=-1,
             action='canvas.move', args=dict(layer_ids=[3.0, 1], dx=2, dy=-1), preference='selection'),
        dict(name='disabled-dense-and-uniform', ids=[2, 3], dx=-1, dy=1,
             action='canvas.move', args=dict(layer_ids=[2, 3], target='auto', dx=-1, dy=1), preference='selection_mask'),
        dict(name='single-explicit-nudge', ids=[2], dx=2, dy=0,
             action='canvas.nudge', args=dict(layer_ids=[2.0], direction='right'), preference='selection'),
        dict(name='all-clipped', ids=[1, 3, 2], dx=4096, dy=-4096,
             action='canvas.move', args=dict(layer_ids=[1, 3, 2], target='layer', dx=4096, dy=-4096), preference='selection_mask'),
    ]
    for case in rows:
        case['model'] = copy.deepcopy(base)
        if case['name'] == 'disabled-dense-and-uniform':
            case['model']['masks'][3] = bytes([91])*AREA
    return rows


def metadata_commands(plan, output, prefix, source):
    add = plan.add
    add('links-open', 'document.open', path=str(source['path']))
    add('links-before', 'canvas.state')
    add('links-png-before', 'canvas.snapshot')
    add('links-first', 'layer.links.set', layer_ids=[2.0, 1], linked=True)
    add('links-pair', 'layer.links.query')
    add('links-union', 'layer.links.set', layer_ids=[2, 3], linked=True)
    add('links-all', 'layer.links.query')
    add('links-save', 'document.save', path=str(output/f'{prefix}-links-all.mcp'))
    add('links-unlink', 'layer.links.set', layer_ids=[2], linked=False)
    add('links-residual', 'layer.links.query')
    add('links-png-after', 'canvas.snapshot')
    add('links-unlink-save', 'document.save', path=str(output/f'{prefix}-links-residual.mcp'))
    add('links-undo', 'history.undo')
    add('links-undo-state', 'layer.links.query')
    add('links-undo-save', 'document.save', path=str(output/f'{prefix}-links-undo.mcp'))
    add('links-redo', 'history.redo')
    add('links-redo-state', 'layer.links.query')
    add('links-redo-save', 'document.save', path=str(output/f'{prefix}-links-redo.mcp'))
    add('links-reopen', 'document.open', path=str(output/f'{prefix}-links-all.mcp'))
    add('links-reopened', 'layer.links.query')


def commands(output, prefix, scenarios, profile):
    plan = Plan()
    add = plan.add
    add('catalog', 'node.catalog')
    metadata_commands(plan, output, prefix, scenarios[0])
    for case in scenarios:
        key = case['name']
        add(key+'-open', 'document.open', path=str(case['path']))
        add(key+'-links', 'layer.links.set', layer_ids=[3, 1, 2], linked=True)
        add(key+'-preferences', 'settings.update', path=str(profile/'settings.json'),
            movement=dict(target=case['preference'], step=2, large_step=3))
        add(key+'-before', 'canvas.state')
        add(key+'-before-png', 'canvas.snapshot')
        add(key+'-events-before', 'replay.events', limit=256)
        add(key+'-operation', case['action'], **case['args'])
        add(key+'-after', 'canvas.state')
        add(key+'-events-after', 'replay.events', limit=256)
        for identity in sorted(case['model']['masks']):
            add(f'{key}-raw-{identity}', 'mask.snapshot', mask_id=identity, width=WIDTH, height=HEIGHT)
        add(key+'-png', 'canvas.snapshot')
        add(key+'-save', 'document.save', path=str(output/f'{prefix}-{key}.mcp'))
        add(key+'-undo', 'history.undo')
        add(key+'-undo-png', 'canvas.snapshot')
        add(key+'-undo-save', 'document.save', path=str(output/f'{prefix}-{key}-undo.mcp'))
        add(key+'-redo', 'history.redo')
        add(key+'-redo-png', 'canvas.snapshot')
        add(key+'-redo-save', 'document.save', path=str(output/f'{prefix}-{key}-redo.mcp'))
        add(key+'-reopen', 'document.open', path=str(output/f'{prefix}-{key}-redo.mcp'))
        add(key+'-reopen-state', 'layer.links.query')
        add(key+'-reopen-png', 'canvas.snapshot')
        add(key+'-verify', 'replay.verify')
        add(key+'-seek', 'replay.seek')
        add(key+'-archive', 'replay.export', path=str(output/f'{prefix}-{key}.mcreplay'))
    return plan


def assert_links(value, sets, active, selected):
    assert value['version'] == 1 and value['layer_links'] == sets, value
    assert value['active_layer_id'] == active and value['selected_layer_ids'] == selected, value
    assert value['active_linked_layer_ids'] == next((members for members in sets if active in members), [active]), value


def validate_metadata(values, plan, output, prefix, source):
    at = lambda key: plan.at(values, key)
    original = source['model']
    active = owners.ids(original)[original['active']]
    selected = original['composition']['selected_layer_ids']
    for key, sets in [('links-pair', [[1, 2]]), ('links-all', [[1, 2, 3]]),
                      ('links-residual', [[1, 3]]), ('links-undo-state', [[1, 2, 3]]),
                      ('links-redo-state', [[1, 3]]), ('links-reopened', [[1, 2, 3]])]:
        assert_links(at(key), sets, active, selected)
    assert at('links-png-before')['data'] == at('links-png-after')['data']
    for suffix, sets in [('all', [[1, 2, 3]]), ('residual', [[1, 3]]),
                         ('undo', [[1, 2, 3]]), ('redo', [[1, 3]])]:
        expected = copy.deepcopy(original)
        memberships(expected, sets)
        owners.project(output/f'{prefix}-links-{suffix}.mcp', expected, original, exact_allocators=True)


def validate(values, plan, output, prefix, scenarios):
    at = lambda key: plan.at(values, key)
    catalog = at('catalog')['nodes']
    assert {'action.layer.links.set', 'action.layer.links.query', 'state.layers.links'} <= {row['id'] for row in catalog}
    for action in ['canvas.move', 'canvas.nudge']:
        node = next(row for row in catalog if row['id'] == 'action.'+action)
        schema = node['parameters_schema']['properties']['layer_ids']
        assert schema['type'] == 'array' and schema['minItems'] == 1 and schema['maxItems'] == 16
        port = next(row for row in node['inputs'] if row['id'] == 'layer_ids')
        assert not port['required']
    validate_metadata(values, plan, output, prefix, scenarios[0])
    results = {}
    for case in scenarios:
        key, original = case['name'], linked_model(case['model'])
        expected = reference(case)
        before_png, expected_png = owners.composed(original), owners.composed(expected)
        before, after = at(key+'-before'), at(key+'-after')
        assert at(key+'-operation')['revision'] == after['revision'] == before['revision']+1, key
        assert after['active_layer'] == original['active']
        assert after['selected_layer_ids'] == original['composition']['selected_layer_ids']
        assert after['raster_masks']['target'] == owners.target(original)
        assert after['layer_links'] == [[1, 2, 3]]
        for row in after['layers']:
            assert row['linked_layer_ids'] == [1, 2, 3]
        for identity, wanted in expected['masks'].items():
            raw = at(f'{key}-raw-{identity}')
            assert raw['encoding'] == 'a8' and (raw['width'], raw['height']) == (WIDTH, HEIGHT)
            geometry.assert_bytes(bytes(raw['samples']), wanted, key+': raw A8 '+str(identity))
        events_before, events_after = at(key+'-events-before'), at(key+'-events-after')
        assert events_after['total'] == events_before['total']+1
        event = events_after['events'][-1]
        assert event['success'] and event['source']['action'] == case['action']
        assert event['source']['context']['movement'] == expected['movement_context'], (key, event)
        raster_damage = sum(a != b for a, b in zip(original['rasters'], expected['rasters']))
        mask_damage = sum(data != expected['masks'][identity] for identity, data in original['masks'].items())
        assert event['changed_tiles'] == raster_damage+int(before_png != expected_png), (key, event)
        assert event['changed_mask_tiles'] == mask_damage, (key, event)
        for suffix, wanted in [('', expected), ('-undo', original), ('-redo', expected)]:
            saved = owners.project(output/f'{prefix}-{key}{suffix}.mcp', wanted, original, exact_allocators=True)
            assert saved['composition']['graph'] == original['composition']['graph']
        png = base64.b64decode(at(key+'-png')['data'])
        assert pixels(png) == (WIDTH, HEIGHT, expected_png), key
        assert pixels(base64.b64decode(at(key+'-before-png')['data'])) == (WIDTH, HEIGHT, before_png), key
        assert at(key+'-undo-png')['data'] == at(key+'-before-png')['data']
        assert at(key+'-redo-png')['data'] == at(key+'-reopen-png')['data'] == at(key+'-png')['data']
        assert_links(at(key+'-reopen-state'), [[1, 2, 3]], owners.ids(original)[original['active']],
                     original['composition']['selected_layer_ids'])
        assert at(key+'-verify')['valid']
        results[key] = dict(png=png, undone_png=base64.b64decode(at(key+'-undo-png')['data']),
                            context=event['source']['context']['movement'],
                            damage=[event['changed_tiles'], event['changed_mask_tiles']], seek=at(key+'-seek'),
                            archive=foundation.archive_metrics(output/f'{prefix}-{key}.mcreplay'))
    return results


def negatives(plan, output, source):
    add = plan.add
    add('negative-open', 'document.open', path=str(source['path']))
    add('negative-before', 'canvas.state')
    for index, args in enumerate([
        dict(layer_ids=[1, 1.0], dx=1, dy=0), dict(layer_ids=[], dx=1, dy=0),
        dict(layer_ids=[1, 2], mask_id=2, dx=1, dy=0),
        dict(layer_ids=[1, 2], target='selection', dx=1, dy=0),
    ]):
        add(f'invalid-{index}', 'canvas.move', error='invalid_arguments', **args)
    add('invalid-prefix', 'pipeline.run', error='invalid_arguments', steps=[
        dict(id='prefix', action='layer.rename', args=dict(layer_id=1, name='Must not run')),
        dict(id='bad', action='canvas.move', args=dict(layer_ids=[2, 2.0], dx=1, dy=0)),
    ])
    add('invalid-links', 'layer.links.set', error='invalid_layer_links', layer_ids=[1], linked=True)
    add('negative-after', 'canvas.state')
    plan.failed_pipeline('atomic', atomic=True, steps=[
        dict(id='link', action='layer.links.set', args=dict(layer_ids=[1, 2, 3], linked=True)),
        dict(id='move', action='canvas.move', args=dict(layer_ids=[1, 2], dx=1, dy=-1)),
        dict(id='fail', action='layer.select', args=dict(layer_id=4294967295)),
    ])
    add('atomic-after', 'canvas.state')
    add('atomic-save', 'document.save', path=str(output/'atomic-restored.mcp'))
    for lock in ['pixel_locked', 'position_locked', 'alpha_locked']:
        add(lock+'-set', 'layer.set', layer_id=2, **{lock: True})
        add(lock+'-before', 'canvas.state')
        add(lock+'-move', 'canvas.move', error='movement', layer_ids=[1, 2], dx=1, dy=0)
        add(lock+'-after', 'canvas.state')
        add(lock+'-clear', 'layer.set', layer_id=2, **{lock: False})
    for tool in ['brush', 'hand']:
        add(tool+'-open', 'document.open', path=str(source['path']))
        add(tool+'-parameters', 'brush.set', radius=1, opacity=.8, stabilizer=.55,
            pressure_size=True, pressure_opacity=True)
        add(tool+'-down', 'canvas.pointer', phase='down', x=2, y=2, pressure=.3, time=100, pen=True, tool_override=tool)
        add(tool+'-chunk', 'pipeline.run', steps=[dict(id='observe', action='layer.links.query', args={})])
        add(tool+'-move-before', 'canvas.pointer', phase='move', x=3, y=2, pressure=.5, time=120, pen=True, tool_override=tool)
        add(tool+'-before', 'canvas.state')
        add(tool+'-link', 'layer.links.set', error='stroke_active', layer_ids=[1, 2], linked=True)
        add(tool+'-move', 'canvas.move', error='stroke_active', layer_ids=[2, 3], dx=1, dy=0)
        add(tool+'-nudge', 'canvas.nudge', error='stroke_active', layer_ids=[1, 2], direction='right')
        add(tool+'-query', 'layer.links.query')
        add(tool+'-after', 'canvas.state')
        add(tool+'-move-after', 'canvas.pointer', phase='move', x=4, y=2, pressure=.8, time=140, pen=True, tool_override=tool)
        add(tool+'-up', 'canvas.pointer', phase='up', x=4, y=2, pressure=.4, time=160, pen=True, tool_override=tool)
        add(tool+'-events', 'replay.events', limit=256)
        if tool == 'brush':
            add(tool+'-undo', 'history.undo')
        add(tool+'-save', 'document.save', path=str(output/f'{tool}-restored.mcp'))
        add(tool+'-verify', 'replay.verify')


def validate_negatives(values, plan, output, source):
    at = lambda key: plan.at(values, key)
    before = editing.document_state(at('negative-before'))
    assert editing.document_state(at('negative-after')) == before
    assert editing.document_state(at('atomic-after')) == before
    assert at('atomic')['rolled_back'] and not at('atomic')['ok']
    owners.project(output/'atomic-restored.mcp', source['model'], source['model'], exact_allocators=True)
    for lock in ['pixel_locked', 'position_locked', 'alpha_locked']:
        assert editing.document_state(at(lock+'-after')) == editing.document_state(at(lock+'-before'))
    for tool in ['brush', 'hand']:
        assert editing.document_state(at(tool+'-before')) == editing.document_state(at(tool+'-after'))
        assert at(tool+'-before')['view'] == at(tool+'-after')['view']
        assert at(tool+'-query')['layer_links'] == []
        assert at(tool+'-after')['drawing'] == (tool == 'brush')
        samples = [sample for event in at(tool+'-events')['events'] for sample in event['source'].get('samples', [])]
        assert [sample['time'] for sample in samples] == [100, 120, 140, 160], tool
        assert [sample['phase'] for sample in samples] == ['down', 'move', 'move', 'up']
        for sample, pressure in zip(samples, [.3, .5, .8, .4]):
            assert sample['pen'] and sample['tool_override'] == tool and abs(sample['pressure']-pressure) < 1e-6
        owners.project(output/f'{tool}-restored.mcp', source['model'], source['model'], exact_allocators=True)
        assert at(tool+'-verify')['valid']


def gui_checks(cli, gui, flags, profile, output, scenarios, expected):
    startup = subprocess.STARTUPINFO()
    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = 0
    process = subprocess.Popen([str(gui), *flags], startupinfo=startup)
    endpoint = profile/'session.json'

    def call(action, args):
        run = subprocess.run([str(cli), 'call', action, json.dumps(args), '--attach', str(endpoint)],
                             capture_output=True, text=True, encoding='utf-8', timeout=30)
        assert run.returncode == 0, (action, run.stdout, run.stderr)
        return json.loads(run.stdout)['result']

    try:
        deadline = time.monotonic()+30
        while not endpoint.exists():
            assert process.poll() is None and time.monotonic() < deadline, 'Isolated GUI startup failed'
            time.sleep(.05)
        CaptureChecks(call, process).settle_layout()
        plan = commands(output, 'gui', scenarios, profile)
        values = editing.run_mcp(cli, ['--attach', str(endpoint)], plan)
        movement.parity(validate(values, plan, output, 'gui', scenarios), expected)
        close = subprocess.run([str(cli), 'call', 'ui.dispatch', json.dumps(dict(id='close', value=True)),
                                '--attach', str(endpoint)], capture_output=True, text=True, encoding='utf-8', timeout=30)
        assert close.returncode == 0, (close.stdout, close.stderr)
        assert process.wait(timeout=15) == 0
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=15)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    build = 'target/build/x86_64-pc-windows-gnu/release/'
    parser.add_argument('--cli', default=build+'micopi-cli.exe')
    parser.add_argument('--gui', default=build+'micopi.exe')
    parser.add_argument('--skip-gui', action='store_true')
    args = parser.parse_args()
    cli, gui = [(ROOT/value).resolve() for value in [args.cli, args.gui]]
    assert cli.is_file() and (args.skip_gui or gui.is_file()), 'Build/stage matching hosts and DLLs first'
    output = ROOT/'target/test-output/linked-movement-smoke'/str(time.time_ns())
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
    base['active'] = 0
    base['composition']['selected_layer_ids'] = [1, 2]
    for row in base['composition']['layer_metadata']:
        for field in ['alpha_locked', 'pixel_locked', 'position_locked']:
            row.pop(field, None)
    base['composition']['layers'][1] = [True, 137, 'normal']
    base['composition']['raster_masks']['target'] = dict(kind='mask', id=4)
    base['masks'][2] = bytes((17*x+31*y+27) % 256 for y in range(HEIGHT) for x in range(WIDTH))
    base['masks'][3] = bytes((11*x+7*y+19) % 256 for y in range(HEIGHT) for x in range(WIDTH))
    base['rasters'][1][0] = 0x00123456
    base['origins'] = {identity: identity for identity in owners.ids(base)}
    scenarios = cases(base)
    for case in scenarios:
        case['path'] = output/(case['name']+'.mcp')
        geometry.write_model(case['path'], case['model'])
    plan = commands(output, 'jsonl', scenarios, profile)
    values = geometry.run_jsonl(cli, flags, plan, output/'linked-movement.jsonl')
    expected = validate(values, plan, output, 'jsonl', scenarios)
    print(f'JSONL: {len(scenarios)} tiny cases, metadata link/unlink and raw/composition/Undo references passed.', flush=True)
    plan = commands(output, 'mcp', scenarios, profile)
    negative = output/'mcp-boundaries'
    negative.mkdir()
    negatives(plan, negative, scenarios[0])
    (output/'linked-movement-mcp.jsonl').write_text('\n'.join(json.dumps(row) for row in plan.rows)+'\n', encoding='utf-8')
    values = editing.run_mcp(cli, flags, plan)
    actual = validate(values, plan, output, 'mcp', scenarios)
    movement.parity(actual, expected)
    validate_negatives(values, plan, negative, scenarios[0])
    print('MCP: exact parity, literal/lock/contact rejection and atomic metadata/raw rollback passed.', flush=True)
    movement.replay_without_dlls(cli, profile, output, scenarios, actual)
    if not args.skip_gui:
        gui_checks(cli, gui, flags, profile, output, scenarios, actual)
        print('One isolated GUI attachment: metadata and movement reference parity passed; no screenshots requested.', flush=True)
    metrics = dict(cases=len(scenarios), width=WIDTH, height=HEIGHT, masks=5,
                   synthetic_input=True, physical_tablet_tested=False, staging_limit_bytes=192*1024*1024,
                   staging_limit_tested='Scalar Rust regressions; this smoke allocates only 7x5 images',
                   outcomes={key: dict(context=value['context'], damage=value['damage'], archive=value['archive'])
                             for key, value in actual.items()})
    (output/'metrics.json').write_text(json.dumps(metrics, indent=2)+'\n', encoding='utf-8')
    print('All linked-movement archives verified without DLLs. Synthetic input only.', flush=True)
    print(output)


if __name__ == '__main__':
    main()
