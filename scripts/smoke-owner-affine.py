"""Verify whole-owner affine transforms using independent tiny pixel references.

Raw RGBA, raw A8, selections, captured repeat scope and composed outcomes are
checked separately through JSONL/MCP and optional quiet desktop attachment.
No physical tablet, Photoshop arithmetic or throughput compatibility is claimed.
"""
import argparse
import base64
import copy
from fractions import Fraction
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import time

from capture_checks import CaptureChecks
from png_checks import pixels

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('affine_owner_movement', ROOT/'scripts/smoke-owner-movement.py')
movement = importlib.util.module_from_spec(spec)
spec.loader.exec_module(movement)
owners, geometry, editing, foundation, Plan = movement.owners, movement.geometry, movement.editing, movement.foundation, movement.Plan
WIDTH, HEIGHT, AREA = movement.WIDTH, movement.HEIGHT, movement.AREA


def rounded(value):
    return (2*value.numerator+value.denominator)//(2*value.denominator)


def sampled(source, matrix, bilinear, raw_mask=False):
    """Rational inverse coordinates; byte scalars and premultiplied colors differ."""
    a, b, c, d, e, f = map(Fraction, matrix)
    determinant = a*e-b*d
    fill = 255 if raw_mask else 0

    def at(x, y):
        return source[y*WIDTH+x] if 0 <= x < WIDTH and 0 <= y < HEIGHT else fill

    output = []
    for y in range(HEIGHT):
        for x in range(WIDTH):
            px, py = Fraction(2*x+1, 2)-c, Fraction(2*y+1, 2)-f
            sx, sy = (e*px-b*py)/determinant, (a*py-d*px)/determinant
            if not bilinear:
                output.append(at(sx.numerator//sx.denominator, sy.numerator//sy.denominator))
                continue
            sx, sy = sx-Fraction(1, 2), sy-Fraction(1, 2)
            ix, iy = sx.numerator//sx.denominator, sy.numerator//sy.denominator
            wx, wy = rounded((sx-ix)*65536), rounded((sy-iy)*65536)
            weights = [(65536-wx)*(65536-wy), wx*(65536-wy), (65536-wx)*wy, wx*wy]
            values = [at(ix, iy), at(ix+1, iy), at(ix, iy+1), at(ix+1, iy+1)]
            if raw_mask:
                output.append((sum(v*w for v, w in zip(values, weights))+(1 << 31))//(1 << 32))
                continue
            if weights[0] == 1 << 32:
                output.append(values[0])  # Preserve aligned invisible RGB, as legacy sampling does.
                continue
            alpha = [(v >> 24)*w for v, w in zip(values, weights)]
            total = sum(alpha)
            amount = (total+(1 << 31))//(1 << 32)
            if amount == 0:
                output.append(0)
                continue
            word = amount << 24
            for shift in [0, 8, 16]:
                word |= (sum(((v >> shift) & 255)*w for v, w in zip(values, alpha))+total//2)//total << shift
            output.append(word)
    return output


def links(model, members):
    for row in model['composition']['layer_metadata']:
        row.pop('links', None)
        if row['id'] in members:
            row['links'] = dict(version=1, layer_ids=sorted(members))


def cases(base):
    rows = [
        dict(name='cross-parent-shared-pivot', ids=[2, 1], matrix=[0, -1, 6, 1, 0, -1], method='nearest', args=dict(layer_ids=[2.0, 1], angle=90, method='nearest')),
        dict(name='linked-bilinear', ids=[2, 3], matrix=[1, 0, .5, 0, 1, 0], method='bilinear', args=dict(layer_ids=[2, 3], translate_x=.5)),
        dict(name='explicit-mask-other-owner', mask=2, matrix=[1, 0, .5, 0, 1, 0], method='bilinear', args=dict(mask_id=2.0, translate_x=.5)),
        dict(name='auto-mask-over-selection', mask=2, matrix=[1, 0, -1, 0, 1, 1], method='nearest', args=dict(target='auto', translate_x=-1, translate_y=1, method='nearest')),
        dict(name='captured-repeat', ids=[2, 1], matrix=[1, 0, 1, 0, 1, 0], method='nearest', args=dict(layer_ids=[2, 1], translate_x=1, method='nearest'), repeat=True),
        dict(name='recipe-only-neutral', ids=[1], matrix=[1, 0, 1, 0, 1, 0], method='nearest', args=dict(layer_ids=[1], translate_x=1, method='nearest'), neutral=True),
    ]
    for case in rows:
        model = copy.deepcopy(base)
        if case.get('mask'):
            model['composition']['layer_metadata'][1].update(pixel_locked=True, alpha_locked=True)
        if case['name'] == 'auto-mask-over-selection':
            model['composition']['raster_masks']['target'] = dict(kind='mask', id=2)
        if case['name'] == 'linked-bilinear':
            item = next(m for m in model['composition']['raster_masks']['items'] if m['id'] == 2)
            item.pop('enabled', None)
            item.update(density=173, feather_radius_px=1)
        if case.get('neutral'):
            model['rasters'] = [[0]*AREA for _ in model['rasters']]
            model['masks'][1] = bytes([255])*AREA
        case['model'] = model
        ids = case.get('ids', [])
        mask_ids = ([case['mask']] if case.get('mask') else
                    [owners.mask_for(model, identity)['id'] for identity in ids
                     if owners.mask_for(model, identity) and owners.mask_for(model, identity).get('linked', True)])
        case['scope'] = dict(version=1, layer_ids=ids, mask_ids=mask_ids)
        if case.get('mask'):
            case['scope']['mask_owner_layer_id'] = 2
    return rows


def reference(case, count=None):
    model = copy.deepcopy(case['model'])
    count = (2 if case.get('repeat') else 1) if count is None else count
    for _ in range(count):
        for identity in case['scope']['layer_ids']:
            index = owners.ids(model).index(identity)
            model['rasters'][index] = sampled(model['rasters'][index], case['matrix'], case['method'] == 'bilinear')
        for identity in case['scope']['mask_ids']:
            model['masks'][identity] = bytes(sampled(model['masks'][identity], case['matrix'], case['method'] == 'bilinear', True))
    if count:
        model['composition']['last_transform'] = dict(matrix=case['matrix'], target='owners', method=case['method'], owners=case['scope'])
    if case.get('repeat') and count == 2:
        links(model, [1, 2, 3])
    return model


def commands(output, prefix, scenarios):
    plan = Plan()
    add = plan.add
    add('catalog', 'node.catalog')
    for case in scenarios:
        key = case['name']
        add(key+'-open', 'document.open', path=str(case['path']))
        add(key+'-before', 'canvas.state')
        add(key+'-before-png', 'canvas.snapshot')
        add(key+'-plan', 'canvas.transform.plan', **case['args'])
        add(key+'-after-plan', 'canvas.state')
        add(key+'-events-before', 'replay.events', limit=256)
        add(key+'-operation', 'canvas.transform', **case['args'])
        if case.get('repeat'):
            add(key+'-join-links', 'layer.links.set', layer_ids=[2, 3], linked=True)
            add(key+'-repeat', 'canvas.transform.repeat')
        add(key+'-after', 'canvas.state')
        add(key+'-recipe', 'canvas.transform.state')
        add(key+'-events', 'replay.events', limit=256)
        add(key+'-png', 'canvas.snapshot')
        for identity in sorted(case['model']['masks']):
            add(f'{key}-raw-{identity}', 'mask.snapshot', mask_id=identity, width=WIDTH, height=HEIGHT)
        add(key+'-save', 'document.save', path=str(output/f'{prefix}-{key}.mcp'))
        steps = 3 if case.get('repeat') else 1
        for i in range(steps):
            add(f'{key}-undo-{i}', 'history.undo')
        add(key+'-undo-save', 'document.save', path=str(output/f'{prefix}-{key}-undo.mcp'))
        add(key+'-undo-png', 'canvas.snapshot')
        for i in range(steps):
            add(f'{key}-redo-{i}', 'history.redo')
        add(key+'-redo-save', 'document.save', path=str(output/f'{prefix}-{key}-redo.mcp'))
        add(key+'-reopen', 'document.open', path=str(output/f'{prefix}-{key}.mcp'))
        add(key+'-reopen-state', 'canvas.state')
        add(key+'-reopen-png', 'canvas.snapshot')
        add(key+'-verify', 'replay.verify')
        add(key+'-seek', 'replay.seek')
        add(key+'-archive', 'replay.export', path=str(output/f'{prefix}-{key}.mcreplay'))
    return plan


def validate(values, plan, output, prefix, scenarios):
    at = lambda key: plan.at(values, key)
    for identity in ['action.canvas.transform', 'action.canvas.transform.plan']:
        node = next(row for row in at('catalog')['nodes'] if row['id'] == identity)
        assert node['parameters_schema']['properties']['layer_ids']['maxItems'] == 16
        assert node['parameters_schema']['properties']['mask_id']['minimum'] == 1
    results = {}
    for case in scenarios:
        key, original, expected = case['name'], case['model'], reference(case)
        before, after = at(key+'-before'), at(key+'-after')
        assert editing.document_state(at(key+'-after-plan')) == editing.document_state(before), key
        p = at(key+'-plan')
        assert p['target'] == ('raster_mask' if case.get('mask') else 'layers'), key
        assert p['owner_scope'] == case['scope'] and p['matrix'] == case['matrix'], (key, p)
        assert p['source_bounds'] == [0, 0, WIDTH, HEIGHT], (key, p)
        assert p['staged_bytes'] == AREA*(4*len(case['scope']['layer_ids'])+len(case['scope']['mask_ids'])), key
        assert p['preview_available'] and p['preview_composite_bytes'] == AREA*4
        assert p['preview_extra_bytes'] >= p['preview_composite_bytes'] and not p['contact_active']
        expected_revision = before['revision']+(3 if case.get('repeat') else 1)
        assert after['revision'] == expected_revision, key
        assert after['active_layer'] == original['active'] and after['selected_layer_ids'] == original['composition']['selected_layer_ids']
        assert after['raster_masks']['target'] == owners.target(original), key
        assert at(key+'-recipe')['target'] == 'owners' and at(key+'-recipe')['owners'] == case['scope']
        for identity, wanted in expected['masks'].items():
            assert bytes(at(f'{key}-raw-{identity}')['samples']) == wanted, (key, identity)
        initial_png, expected_png = owners.composed(original), owners.composed(expected)
        png = base64.b64decode(at(key+'-png')['data'])
        assert pixels(png) == (WIDTH, HEIGHT, expected_png), key
        assert pixels(base64.b64decode(at(key+'-before-png')['data'])) == (WIDTH, HEIGHT, initial_png), key
        assert at(key+'-undo-png')['data'] == at(key+'-before-png')['data'], key
        assert pixels(base64.b64decode(at(key+'-reopen-png')['data'])) == (WIDTH, HEIGHT, expected_png), key
        for suffix, model in [('', expected), ('-undo', original), ('-redo', expected)]:
            owners.project(output/f'{prefix}-{key}{suffix}.mcp', model, original, exact_allocators=True)
        before_count = at(key+'-events-before')['total']
        events = at(key+'-events')['events'][before_count:]
        transforms = [row for row in events if row['source'].get('action') in ['canvas.transform', 'canvas.transform.repeat']]
        assert len(events) == (3 if case.get('repeat') else 1) and len(transforms) == (2 if case.get('repeat') else 1), key
        for i, event in enumerate(transforms):
            previous, current = reference(case, i), reference(case, i+1)
            context = event['source']['context']['transform']
            assert context['owner_scope'] == case['scope'] and context['matrix'] == case['matrix'], (key, context)
            assert context['layer_ids'] == case['scope']['layer_ids'] and context['mask_ids'] == case['scope']['mask_ids']
            assert event['success']
            rgba_damage = sum(a != b for a, b in zip(previous['rasters'], current['rasters']))
            a8_damage = sum(data != current['masks'][identity] for identity, data in previous['masks'].items())
            surface_damage = int(owners.composed(previous) != owners.composed(current))
            assert event['changed_tiles'] == rgba_damage+surface_damage and event['changed_mask_tiles'] == a8_damage, (key, event)
        assert at(key+'-verify')['valid']
        results[key] = dict(png=png, seek=at(key+'-seek'))
    return results


def negatives(output, source):
    plan = Plan()
    add = plan.add
    add('open', 'document.open', path=str(source['path']))
    add('before', 'canvas.state')
    for i, args in enumerate([dict(layer_ids=[1, 1.0]), dict(layer_ids=[1], mask_id=2),
                              dict(layer_ids=[1], target='selection'), dict(mask_id=2, target='layer'),
                              dict(layer_ids=[1], quad=[[0, 0], [7, 0], [7, 5], [0, 5]])]):
        add(f'invalid-{i}', 'canvas.transform', error='invalid_arguments', **args)
    plan.failed_pipeline('atomic', atomic=True, steps=[
        dict(id='transform', action='canvas.transform', args=dict(layer_ids=[2, 1], translate_x=.5)),
        dict(id='fail', action='layer.select', args=dict(index=15))])
    add('after', 'canvas.state')
    add('restored', 'document.save', path=str(output/'atomic-restored.mcp'))
    add('verify', 'replay.verify')
    for tool in ['brush', 'hand']:
        add(tool+'-open', 'document.open', path=str(source['path']))
        add(tool+'-parameters', 'brush.set', radius=1.5, hardness=1, stabilizer=.55, pressure_size=True, pressure_opacity=True)
        add(tool+'-down', 'canvas.pointer', phase='down', x=2.5, y=2.5, pressure=.3, time=100, pen=True, tool_override=tool)
        add(tool+'-move-before', 'canvas.pointer', phase='move', x=3.5, y=2.5, pressure=.6, time=120, pen=True, tool_override=tool)
        add(tool+'-before', 'canvas.state')
        add(tool+'-raw-before', 'mask.snapshot', mask_id=4, width=WIDTH, height=HEIGHT)
        add(tool+'-plan', 'canvas.transform.plan', layer_ids=[2, 1], angle=10)
        add(tool+'-rejected', 'canvas.transform', error='stroke_active', layer_ids=[2, 1], translate_x=1)
        add(tool+'-after', 'canvas.state')
        add(tool+'-raw-after', 'mask.snapshot', mask_id=4, width=WIDTH, height=HEIGHT)
        add(tool+'-move-after', 'canvas.pointer', phase='move', x=4.5, y=2.5, pressure=.8, time=140, pen=True, tool_override=tool)
        add(tool+'-up', 'canvas.pointer', phase='up', x=4.5, y=2.5, pressure=.2, time=160, pen=True, tool_override=tool)
        add(tool+'-events', 'replay.events', limit=256)
        if tool == 'brush':
            add(tool+'-undo', 'history.undo')
        add(tool+'-restored', 'document.save', path=str(output/f'{tool}-restored.mcp'))
        add(tool+'-verify', 'replay.verify')
    return plan


def validate_negatives(values, plan, output, source):
    at = lambda key: plan.at(values, key)
    assert editing.document_state(at('before')) == editing.document_state(at('after'))
    assert at('atomic')['rolled_back'] and at('verify')['valid']
    owners.project(output/'atomic-restored.mcp', source['model'], source['model'], exact_allocators=True)
    for tool in ['brush', 'hand']:
        assert editing.document_state(at(tool+'-before')) == editing.document_state(at(tool+'-after'))
        assert at(tool+'-before')['view'] == at(tool+'-after')['view']
        assert at(tool+'-raw-before')['samples'] == at(tool+'-raw-after')['samples']
        painted = bytes(at(tool+'-raw-before')['samples']) != source['model']['masks'][4]
        assert painted == (tool == 'brush'), 'Contact fixture must exercise actual A8 painting'
        assert at(tool+'-plan')['contact_active']
        samples = [sample for event in at(tool+'-events')['events'] for sample in event['source'].get('samples', [])]
        assert [sample['time'] for sample in samples] == [100, 120, 140, 160], tool
        assert [sample['phase'] for sample in samples] == ['down', 'move', 'move', 'up']
        for sample, pressure in zip(samples, [.3, .6, .8, .2]):
            assert sample['pen'] and sample['tool_override'] == tool and abs(sample['pressure']-pressure) < 1e-6
        owners.project(output/f'{tool}-restored.mcp', source['model'], source['model'], exact_allocators=True)
        assert at(tool+'-verify')['valid']


def gui_checks(cli, gui, flags, profile, output, scenarios):
    startup = subprocess.STARTUPINFO()
    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = 0
    process = subprocess.Popen([str(gui), *flags], startupinfo=startup)
    endpoint = profile/'session.json'

    def call(action, args):
        run = subprocess.run([str(cli), 'call', action, json.dumps(args), '--attach', str(endpoint)], capture_output=True, text=True, encoding='utf-8', timeout=30)
        assert run.returncode == 0, (action, run.stdout, run.stderr)
        return json.loads(run.stdout)['result']

    try:
        deadline = time.monotonic()+30
        while not endpoint.exists():
            assert process.poll() is None and time.monotonic() < deadline, 'Isolated GUI startup failed'
            time.sleep(.05)
        CaptureChecks(call, process).settle_layout()
        plan = commands(output, 'gui', scenarios)
        validate(editing.run_mcp(cli, ['--attach', str(endpoint)], plan), plan, output, 'gui', scenarios)
        call('ui.dispatch', dict(id='close', value=True))
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
    cli, gui = [(ROOT/path).resolve() for path in [args.cli, args.gui]]
    assert cli.is_file() and (args.skip_gui or gui.is_file()), 'Build/stage matching hosts and DLLs first'
    output = ROOT/'target/test-output/owner-affine-smoke'/str(time.time_ns())
    output.mkdir(parents=True)
    print(output, flush=True)
    plugins, profile = output/'plugins', output/'profile'
    shutil.copytree(ROOT/'plugins', plugins)
    profile.mkdir()
    (profile/'settings.json').write_text(json.dumps(dict(version=1, custom=dict(keep=42))), encoding='utf-8')
    flags = ['--plugins', str(plugins), '--config-dir', str(profile)]
    geometry.run_jsonl(cli, flags, geometry.bootstrap(output), output/'bootstrap.jsonl')
    base = geometry.write_fixture(output/'initial.mcp', foundation.read_project(output/'seed.mcp'))
    base['active'] = 0
    base['composition']['selected_layer_ids'] = [1, 2]
    base['composition']['raster_masks']['target'] = dict(kind='mask', id=4)
    base['composition']['layers'][1] = [True, 137, 'normal']
    for row in base['composition']['layer_metadata']:
        for field in ['pixel_locked', 'position_locked', 'alpha_locked']:
            row.pop(field, None)
    for item in base['composition']['raster_masks']['items']:
        if item['id'] == 1:
            item.pop('linked', None)
        if item['id'] == 3:
            item['linked'] = False
    links(base, [1, 3])
    base['origins'] = {identity: identity for identity in owners.ids(base)}
    scenarios = cases(base)
    for case in scenarios:
        case['path'] = output/(case['name']+'.mcp')
        geometry.write_model(case['path'], case['model'])
    expected = None
    for prefix, runner in [('jsonl', lambda p: geometry.run_jsonl(cli, flags, p, output/'owner-affine.jsonl')),
                           ('mcp', lambda p: editing.run_mcp(cli, flags, p))]:
        plan = commands(output, prefix, scenarios)
        actual = validate(runner(plan), plan, output, prefix, scenarios)
        if expected is not None:
            assert {k: pixels(v['png']) for k, v in actual.items()} == {k: pixels(v['png']) for k, v in expected.items()}
        expected = actual
        print(f'{prefix.upper()}: {len(scenarios)} tiny cases / {len(plan.rows)} commands passed independent raw/context/Undo references.', flush=True)
    negative = negatives(output, scenarios[0])
    values = editing.run_mcp(cli, flags, negative)
    validate_negatives(values, negative, output, scenarios[0])
    movement.replay_without_dlls(cli, profile, output, scenarios, expected)
    if not args.skip_gui:
        gui_checks(cli, gui, flags, profile, output, scenarios)
    (output/'metrics.json').write_text(json.dumps(dict(cases=len(scenarios), width=WIDTH, height=HEIGHT,
        synthetic_input=True, physical_tablet_tested=False, performance_measured=False,
        fault_injection='Rust regressions only; no public hook'), indent=2)+'\n', encoding='utf-8')
    print('Owner affine raw sampling, captured repeat, atomic rollback and DLL-free outcomes passed.', flush=True)
    print(output)


if __name__ == '__main__':
    main()
