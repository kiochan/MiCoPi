"""Verify bounded raster-owner Move through independent bytes and shared clients.

The 7x5 inverse-coordinate oracle checks raw RGBA, raw A8, selection, outcome
damage and resolved context separately. Composition references reuse the existing
independent integer group/filter/mask oracle. Synthetic contacts are not hardware
compatibility or throughput measurements. Run one isolated GUI at a time.
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
spec = importlib.util.spec_from_file_location('owner_structure_smoke', ROOT/'scripts/smoke-mask-owners.py')
owners = importlib.util.module_from_spec(spec)
spec.loader.exec_module(owners)
geometry, editing, foundation, Plan = owners.geometry, owners.editing, owners.foundation, owners.Plan
WIDTH, HEIGHT, AREA = owners.WIDTH, owners.HEIGHT, owners.AREA
LANGUAGES = ['en', 'zh-CN', 'ja', 'ko']


def shifted(data, dx, dy, exterior):
    """Destination-to-source lookup, independent from the native staging code."""
    return [data[sy*WIDTH+sx] if 0 <= sx < WIDTH and 0 <= sy < HEIGHT else exterior
            for y in range(HEIGHT) for x in range(WIDTH)
            for sx, sy in [(x-dx, y-dy)]]


def mask(model, identity=2):
    return next(row for row in model['composition']['raster_masks']['items'] if row['id'] == identity)


def cases(base):
    rows = []

    def add(name, domain, dx, dy, action='canvas.move', args=None, edit=None, preference='auto'):
        model = copy.deepcopy(base)
        if edit:
            edit(model)
        rows.append(dict(name=name, domain=domain, dx=dx, dy=dy, action=action,
                         args=args if args is not None else dict(target='layer', dx=dx, dy=dy),
                         preference=preference, model=model))

    add('layer-linked-dense', 'layer', 2, -1)

    def disabled_uniform(model):
        mask(model)['enabled'] = False
        model['masks'][2] = bytes([91])*AREA
    add('layer-linked-disabled-uniform', 'layer', -1, 1, edit=disabled_uniform)
    add('layer-unlinked-dense', 'layer', 1, 0, edit=lambda model: mask(model).update(linked=False))
    add('auto-mask-over-selection', 'mask', 1, -1, args=dict(dx=1, dy=-1),
        edit=lambda model: model['composition']['raster_masks'].update(target=dict(kind='mask', id=2)))

    def other_active(model):
        model['active'] = 0
        model['composition']['selected_layer_ids'] = [1, 3]
        model['composition']['layer_metadata'][1].update(pixel_locked=True, alpha_locked=True)
    add('explicit-mask-other-owner', 'mask', -2, 0, args=dict(mask_id=2.0, dx=-2, dy=0),
        edit=other_active, preference='selection_mask')
    add('nudge-mask-default-step', 'mask', 2, 0, action='canvas.nudge',
        args=dict(mask_id=2.0, target='auto', direction='right'), preference='selection_mask')
    add('nudge-layer-large-step', 'layer', 0, -3, action='canvas.nudge',
        args=dict(target='layer', direction='up', large=True))
    add('selection-outline-only', 'outline', 1, 0, args=dict(target='selection_mask', dx=1, dy=0))
    add('mask-complete-clipping', 'mask', 4096, -4096, args=dict(mask_id=2, dx=4096, dy=-4096))
    add('mask-equal-uniform', 'mask', 2, 0, args=dict(mask_id=2, dx=2, dy=0),
        edit=lambda model: model['masks'].update({2: bytes([255])*AREA}))
    return rows


def reference(case):
    result = copy.deepcopy(case['model'])
    dx, dy, domain = case['dx'], case['dy'], case['domain']
    if domain == 'layer':
        result['rasters'][1] = shifted(result['rasters'][1], dx, dy, 0)
        linked = mask(result).get('linked', True)
        if linked:
            result['masks'][2] = bytes(shifted(result['masks'][2], dx, dy, 255))
        context = dict(target='layer', layer_id=2, linked_mask_id=2 if linked else None, dx=dx, dy=dy)
    elif domain == 'mask':
        result['masks'][2] = bytes(shifted(result['masks'][2], dx, dy, 255))
        context = dict(target='raster_mask', mask_id=2, owner_layer_id=2, dx=dx, dy=dy)
    else:
        result['selections']['current'] = bytes(shifted(result['selections']['current'], dx, dy, 0))
        result['composition']['selection']['current'] = geometry.selection_wire(result['selections']['current'], WIDTH, HEIGHT)
        context = dict(target='selection_mask', dx=dx, dy=dy)
    result['movement_context'] = context
    return result


def commands(output, prefix, scenarios, profile):
    plan = Plan()
    add = plan.add
    add('catalog', 'node.catalog')
    for case in scenarios:
        key = case['name']
        add(key+'-open', 'document.open', path=str(case['path']))
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
        add(key+'-reopen-png', 'canvas.snapshot')
        add(key+'-verify', 'replay.verify')
        add(key+'-seek', 'replay.seek')
        add(key+'-archive', 'replay.export', path=str(output/f'{prefix}-{key}.mcreplay'))
    return plan


def validate(values, plan, output, prefix, scenarios):
    at = lambda key: plan.at(values, key)
    for action in ['canvas.move', 'canvas.nudge']:
        node = next(row for row in at('catalog')['nodes'] if row['id'] == 'action.'+action)
        schema = node['parameters_schema']['properties']['mask_id']
        assert schema['type'] == 'integer' and schema['minimum'] == 1 and schema['maximum'] == 4294967295
        port = next(row for row in node['inputs'] if row['id'] == 'mask_id')
        assert port['data_type'] == 'integer' and not port['required']
    outcomes = {}
    for case in scenarios:
        key, original = case['name'], case['model']
        expected = reference(case)
        before_png, expected_png = owners.composed(original), owners.composed(expected)
        after = at(key+'-after')
        assert at(key+'-operation')['revision'] == after['revision'] == at(key+'-before')['revision']+1, key
        assert after['active_layer'] == original['active'] and after['selected_layer_ids'] == original['composition']['selected_layer_ids']
        assert after['raster_masks']['target'] == owners.target(original), key
        for identity, wanted in expected['masks'].items():
            snapshot = at(f'{key}-raw-{identity}')
            assert snapshot['encoding'] == 'a8' and (snapshot['width'], snapshot['height']) == (WIDTH, HEIGHT)
            geometry.assert_bytes(bytes(snapshot['samples']), wanted, f'{key}: raw A8 {identity}')
        before, after_events = at(key+'-events-before'), at(key+'-events-after')
        assert after_events['total'] == before['total']+1
        event = after_events['events'][-1]
        assert event['success'] and event['source']['action'] == case['action']
        assert event['source']['context']['movement'] == expected['movement_context'], (key, event)
        raster_damage = sum(a != b for a, b in zip(original['rasters'], expected['rasters']))
        surface_damage = int(before_png != expected_png)
        mask_damage = sum(a != expected['masks'][identity] for identity, a in original['masks'].items())
        assert event['changed_tiles'] == raster_damage+surface_damage, (key, event)
        assert event['changed_mask_tiles'] == mask_damage, (key, event)
        for suffix, wanted in [('', expected), ('-undo', original), ('-redo', expected)]:
            saved = owners.project(output/f'{prefix}-{key}{suffix}.mcp', wanted, original, exact_allocators=True)
            assert saved['composition']['graph'] == original['composition']['graph'], key+suffix
        png = base64.b64decode(at(key+'-png')['data'])
        assert pixels(png) == (WIDTH, HEIGHT, expected_png), key
        assert pixels(base64.b64decode(at(key+'-before-png')['data'])) == (WIDTH, HEIGHT, before_png), key
        assert at(key+'-undo-png')['data'] == at(key+'-before-png')['data'], key
        assert at(key+'-redo-png')['data'] == at(key+'-reopen-png')['data'] == at(key+'-png')['data'], key
        assert at(key+'-verify')['valid']
        outcomes[key] = dict(png=png, undone_png=base64.b64decode(at(key+'-undo-png')['data']),
                             context=event['source']['context']['movement'],
                             damage=[event['changed_tiles'], event['changed_mask_tiles']],
                             seek=at(key+'-seek'), archive=foundation.archive_metrics(output/f'{prefix}-{key}.mcreplay'))
    return outcomes


def negatives(plan, output, case):
    add = plan.add
    add('negative-open', 'document.open', path=str(case['path']))
    add('negative-before', 'canvas.state')
    add('negative-png', 'canvas.snapshot')
    for index, (action, args) in enumerate([
        ('canvas.move', dict(target='selection', dx=1, dy=0)),
        ('canvas.move', dict(mask_id=4, dx=1, dy=0)),
        ('canvas.move', dict(dx=1, dy=0)),
        # Whole-layer affine is supported; selected-pixel affine on a masked
        # document remains outside the owner-transform contract.
        ('canvas.transform', dict(target='selection', translate_x=1)),
        ('canvas.transform.plan', dict(target='selection', translate_x=1)),
    ]):
        add(f'pending-{index}', action, error='mask_editing_pending', **args)
    add('supported-affine-plan', 'canvas.transform.plan', target='layer', translate_x=1, method='nearest')
    add('negative-after', 'canvas.state')
    add('negative-after-png', 'canvas.snapshot')
    add('invalid-prefix', 'pipeline.run', error='invalid_arguments', steps=[
        dict(id='prefix', action='layer.rename', args=dict(layer_id=1, name='Must not execute')),
        dict(id='bad', action='canvas.move', args=dict(mask_id=2, target='layer', dx=1, dy=0)),
    ])
    add('invalid-after', 'canvas.state')
    plan.failed_pipeline('atomic', atomic=True, steps=[
        dict(id='move', action='canvas.move', args=dict(target='layer', dx=1, dy=0)),
        dict(id='fail', action='layer.select', args=dict(layer_id=4294967295)),
    ])
    add('atomic-after', 'canvas.state')
    add('atomic-save', 'document.save', path=str(output/'atomic-restored.mcp'))
    for tool in ['brush', 'hand']:
        add(tool+'-open', 'document.open', path=str(case['path']))
        add(tool+'-parameters', 'brush.set', radius=1, opacity=.8, stabilizer=.55,
            pressure_size=True, pressure_opacity=True)
        add(tool+'-down', 'canvas.pointer', phase='down', x=2, y=2, pressure=.3, time=100, pen=True, tool_override=tool)
        add(tool+'-chunk', 'pipeline.run', steps=[dict(id='observe', action='canvas.state', args={})])
        add(tool+'-chunk-events', 'replay.events', limit=256)
        add(tool+'-move-before', 'canvas.pointer', phase='move', x=3, y=2, pressure=.5, time=120, pen=True, tool_override=tool)
        add(tool+'-before', 'canvas.state')
        add(tool+'-raw-before', 'mask.snapshot', mask_id=4, width=WIDTH, height=HEIGHT)
        for index, (action, args) in enumerate([
            ('canvas.move', dict(target='layer', dx=1, dy=0)),
            ('canvas.move', dict(mask_id=2, dx=0, dy=0)),
            ('canvas.nudge', dict(mask_id=2, direction='right')),
            ('canvas.move', dict(target='selection_mask', dx=1, dy=0)),
        ]):
            add(f'{tool}-idle-{index}', action, error='stroke_active', **args)
        add(tool+'-after', 'canvas.state')
        add(tool+'-raw-after', 'mask.snapshot', mask_id=4, width=WIDTH, height=HEIGHT)
        add(tool+'-move-after', 'canvas.pointer', phase='move', x=4, y=2, pressure=.8, time=140, pen=True, tool_override=tool)
        add(tool+'-up', 'canvas.pointer', phase='up', x=4, y=2, pressure=.4, time=160, pen=True, tool_override=tool)
        add(tool+'-events', 'replay.events', limit=256)
        if tool == 'brush':
            add(tool+'-undo', 'history.undo')
        add(tool+'-save', 'document.save', path=str(output/f'{tool}-restored.mcp'))
        add(tool+'-verify', 'replay.verify')


def validate_negatives(values, plan, output, case):
    at = lambda key: plan.at(values, key)
    baseline = editing.document_state(at('negative-before'))
    affine = at('supported-affine-plan')
    active_id = case['model']['composition']['layer_metadata'][case['model']['active']]['id']
    assert affine['target'] == 'layers' and affine['layer_ids'] == [active_id]
    assert affine['matrix'] == [1, 0, 1, 0, 1, 0]
    for key in ['negative-after', 'invalid-after', 'atomic-after']:
        assert editing.document_state(at(key)) == baseline, key
    assert at('negative-png')['data'] == at('negative-after-png')['data']
    assert at('atomic')['rolled_back'] and not at('atomic')['ok']
    owners.project(output/'atomic-restored.mcp', case['model'], case['model'], exact_allocators=True)
    for tool in ['brush', 'hand']:
        assert editing.document_state(at(tool+'-before')) == editing.document_state(at(tool+'-after'))
        assert at(tool+'-before')['view'] == at(tool+'-after')['view']
        assert at(tool+'-raw-before') == at(tool+'-raw-after')
        assert at(tool+'-after')['drawing'] == (tool == 'brush')
        recorded = [event['source'] for event in at(tool+'-chunk-events')['events']
                    if event['source'].get('kind') == 'pointer']
        assert any(source.get('complete') is False and [sample['time'] for sample in source['samples']] == [100]
                   for source in recorded), tool+': readonly pipeline did not flush an active chunk'
        events = at(tool+'-events')['events']
        samples = [sample for event in events for sample in event['source'].get('samples', [])]
        assert [sample['time'] for sample in samples] == [100, 120, 140, 160]
        assert [sample['phase'] for sample in samples] == ['down', 'move', 'move', 'up']
        for sample, pressure in zip(samples, [.3, .5, .8, .4]):
            assert sample['pen'] and sample['tool_override'] == tool and abs(sample['pressure']-pressure) < 1e-6
        owners.project(output/f'{tool}-restored.mcp', case['model'], case['model'], exact_allocators=True)
        assert at(tool+'-verify')['valid']


def parity(actual, expected):
    assert actual.keys() == expected.keys()
    for key in expected:
        for field in ['png', 'undone_png', 'context', 'damage']:
            assert actual[key][field] == expected[key][field], (key, field)


def jsonl_terminal_guards(cli, flags, profile, output, case):
    """CLI deliberately stops on errors; continuing-contact proofs use MCP below."""
    for name, args, code, contact in [
        ('selected-pixels', dict(target='selection', dx=1, dy=0), 'mask_editing_pending', False),
        ('group-mask', dict(mask_id=4, dx=1, dy=0), 'mask_editing_pending', False),
        ('active-owner', dict(target='layer', dx=1, dy=0), 'stroke_active', True),
    ]:
        plan = Plan()
        plan.add('open', 'document.open', path=str(case['path']))
        plan.add('preferences', 'settings.update', path=str(profile/'settings.json'),
                 movement=dict(target='auto', step=2, large_step=3))
        if contact:
            plan.add('down', 'canvas.pointer', phase='down', x=2, y=2, pressure=.3, time=100,
                     pen=True, tool_override='brush')
        plan.add('before', 'canvas.state')
        plan.add('guard', 'canvas.move', **args)
        plan.add('unreached', 'layer.rename', name='Must not execute after an error')
        path = output/f'jsonl-terminal-{name}.jsonl'
        path.write_text('\n'.join(json.dumps(row) for row in plan.rows)+'\n', encoding='utf-8')
        run = subprocess.run([str(cli), 'run', str(path), *flags], capture_output=True,
                             text=True, encoding='utf-8', timeout=60)
        path.with_suffix('.responses.jsonl').write_text(run.stdout, encoding='utf-8')
        rows = [json.loads(line) for line in run.stdout.splitlines()]
        assert run.returncode != 0 and len(rows) == len(plan.rows)-1, (name, run.stdout, run.stderr)
        assert all(row['ok'] for row in rows[:-1]) and rows[-1]['error']['code'] == code, (name, rows)


def replay_without_dlls(cli, profile, output, scenarios, expected):
    minimal = output/'replay-only-plugins'
    minimal.mkdir()
    for name in ['core', 'replay']:
        shutil.copytree(ROOT/'plugins'/name, minimal/name)
    assert not list(minimal.rglob('*.dll'))
    plan = Plan()
    for case in scenarios:
        key = case['name']
        plan.add(key+'-load', 'replay.load', path=str(output/f'mcp-{key}.mcreplay'))
        plan.add(key+'-verify', 'replay.verify', source='loaded')
        plan.add(key+'-png', 'replay.snapshot', source='loaded')
        plan.add(key+'-seek', 'replay.seek', source='loaded')
    values = editing.run_mcp(cli, ['--plugins', str(minimal), '--config-dir', str(profile)], plan, require_masks=False)
    for case in scenarios:
        key = case['name']
        assert plan.at(values, key+'-verify')['valid']
        assert base64.b64decode(plan.at(values, key+'-png')['data']) == expected[key]['png']
        for field in ['state_hash', 'surface_hash']:
            assert plan.at(values, key+'-seek')[field] == expected[key]['seek'][field]


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
            assert process.poll() is None and time.monotonic() < deadline, 'Isolated GUI startup failed'
            time.sleep(.05)
        checks.settle_layout()
        plan = commands(output, 'gui', scenarios, profile)
        negative = output/'gui-boundaries'
        negative.mkdir()
        negatives(plan, negative, scenarios[0])
        values = editing.run_mcp(cli, ['--attach', str(endpoint)], plan)
        parity(validate(values, plan, output, 'gui', scenarios), expected)
        validate_negatives(values, plan, negative, scenarios[0])
        call('document.open', dict(path=str(output/'gui-auto-mask-over-selection.mcp')))
        call('settings.update', dict(path=str(profile/'settings.json'), movement=dict(target='auto', step=2, large_step=3)))
        call('toolbox.mode.move', {})
        artwork = call('canvas.snapshot', {})['data']
        for language in LANGUAGES:
            call('settings.update', dict(path=str(profile/'settings.json'), ui=dict(language=language)))
            stored = json.loads((profile/'settings.json').read_text(encoding='utf-8-sig'))
            assert stored['ui']['language'] == language and stored['custom']['keep'] == 42
            for label, width in [('narrow', 340), ('wide', 680)]:
                checks.window('tool-options', 'float', x=70, y=70, width=width, height=480)
                _, size = checks.ready('tool-options', (width, 480))
                checks.settle_layout()
                assert call('canvas.state', {})['raster_masks']['target'] == dict(kind='mask', id=2)
                captures.append(dict(language=language, layout=label,
                                     **checks.capture(output/f'{language}-owner-move-{label}.png', 'tool-options', size=size)))
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
    assert cli.is_file() and (args.skip_gui or gui.is_file()), 'Build/stage matching hosts and DLLs first'
    output = ROOT/'target/test-output/owner-movement-smoke'/str(time.time_ns())
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
    base['active'] = 1
    base['composition']['selected_layer_ids'] = [1, 2]
    base['composition']['layers'][1] = [True, 137, 'normal']
    for flag in ['pixel_locked', 'alpha_locked', 'position_locked']:
        base['composition']['layer_metadata'][1].pop(flag, None)
    mask(base).pop('enabled', None)
    mask(base).update(density=173, feather_radius_px=1)
    base['composition']['raster_masks']['target'] = dict(kind='mask', id=4)
    base['masks'][2] = bytes((17*x+31*y+27) % 256 for y in range(HEIGHT) for x in range(WIDTH))
    base['masks'][3] = bytes((11*x+7*y+19) % 256 for y in range(HEIGHT) for x in range(WIDTH))
    base['origins'] = {identity: identity for identity in owners.ids(base)}
    scenarios = cases(base)
    for case in scenarios:
        case['path'] = output/(case['name']+'.mcp')
        geometry.write_model(case['path'], case['model'])
    plan = commands(output, 'jsonl', scenarios, profile)
    values = geometry.run_jsonl(cli, flags, plan, output/'movement.jsonl')
    expected = validate(values, plan, output, 'jsonl', scenarios)
    print(f'JSONL: {len(scenarios)} bounded cases / {len(plan.rows)} commands passed byte, composition, damage and context references.', flush=True)
    jsonl_terminal_guards(cli, flags, profile, output, scenarios[0])
    plan = commands(output, 'mcp', scenarios, profile)
    negative = output/'mcp-boundaries'
    negative.mkdir()
    negatives(plan, negative, scenarios[0])
    (output/'movement-mcp.jsonl').write_text('\n'.join(json.dumps(row) for row in plan.rows)+'\n', encoding='utf-8')
    values = editing.run_mcp(cli, flags, plan)
    actual = validate(values, plan, output, 'mcp', scenarios)
    parity(actual, expected)
    validate_negatives(values, plan, negative, scenarios[0])
    print('MCP: exact reference parity, pending boundaries, atomic rollback and Brush/Hand continuity passed.', flush=True)
    replay_without_dlls(cli, profile, output, scenarios, actual)
    metrics = dict(cases=len(scenarios), width=WIDTH, height=HEIGHT, masks=5,
                   logical_a8_bytes=AREA*5, synthetic_input=True, physical_tablet_tested=False,
                   fault_injection='Rust regressions only; no new public hook',
                   jsonl_errors='Three actual terminal rejection cases; CLI stops on first error, continuing contact/rollback proofs use MCP and GUI attachment',
                   outcomes={key: dict(context=value['context'], damage=value['damage'], archive=value['archive']) for key, value in actual.items()})
    if not args.skip_gui:
        captures = gui_checks(cli, gui, flags, profile, output, scenarios, actual)
        metrics['captures'] = dict(count=len(captures), png_bytes=sum(row['bytes'] for row in captures))
        print('One isolated GUI: reference parity and eight actual four-language narrow/wide Tool Options captures passed.', flush=True)
    (output/'metrics.json').write_text(json.dumps(metrics, indent=2)+'\n', encoding='utf-8')
    print('All owner-movement outcome archives verified without DLLs. Synthetic input only.', flush=True)
    print(output)


if __name__ == '__main__':
    main()
