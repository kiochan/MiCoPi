"""Verify capped brush influence and independent Flow through real transports.

All images are 7x5. Rational full-interior dab references check raw RGBA/A8,
selection-once rounding, origin recomposition, Undo and DLL-free outcomes.
Optional attachment uses one owned desktop process; no tablet/throughput claim.
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

from png_checks import pixels
from capture_checks import CaptureChecks

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('opacity_geometry_smoke', ROOT/'scripts/smoke-mask-geometry.py')
geometry = importlib.util.module_from_spec(spec)
spec.loader.exec_module(geometry)
editing, foundation, Plan = geometry.editing, geometry.foundation, geometry.Plan
WIDTH, HEIGHT, AREA = 7, 5, 35
COLOR = 0x194f93


def rounded(value):
    return (2*value.numerator+value.denominator)//(2*value.denominator)


def alpha(caps, flow, selected):
    """Independent u16 recurrence, followed by a single coverage quantization."""
    weight, amount = 0, rounded(65535*flow)
    for cap in caps:
        deficit = max(0, rounded(65535*cap)-weight)
        weight += (deficit*amount+32767)//65535
    return (weight*selected+32767)//65535


def over(before, color, amount):
    if amount == 0:
        return before
    old_alpha = before >> 24
    out_alpha = amount+(old_alpha*(255-amount)+127)//255
    result = out_alpha << 24
    for shift in [0, 8, 16]:
        src, dst = (color >> shift) & 255, (before >> shift) & 255
        result |= min(255, (src*amount*255+dst*old_alpha*(255-amount)+out_alpha*127)//(out_alpha*255)) << shift
    return result


def atop(before, amount):
    if before >> 24 == 0:
        return before
    result = before & 0xff000000
    for shift in [0, 8, 16]:
        result |= (((COLOR >> shift) & 255)*amount+((before >> shift) & 255)*(255-amount)+127)//255 << shift
    return result


def selection(values):
    encoder = zlib.compressobj(level=1, wbits=-15)
    compressed = encoder.compress(bytes(values))+encoder.flush()
    return dict(width=WIDTH, height=HEIGHT, deflate_hex=compressed.hex())


def cases(seed):
    rows = [
        dict(name='one-contact', flow=Fraction(1), contacts=1, pressures=[1]*5),
        dict(name='two-contacts', flow=Fraction(1), contacts=2, pressures=[1]*3),
        dict(name='selection-low-flow', flow=Fraction(1, 4), contacts=1, pressures=[1]*3, selected=True),
        dict(name='pressure-floor', flow=Fraction(1, 4), contacts=1, pressures=[0, .2, .2], pressure_opacity=True, floor=.25),
        dict(name='rgba-erase', flow=Fraction(1, 4), contacts=1, pressures=[1]*3, erase=True),
        dict(name='alpha-locked', flow=Fraction(1, 4), contacts=1, pressures=[1]*3, locked=True),
        dict(name='a8-brush', flow=Fraction(1, 4), contacts=1, pressures=[1]*3, a8=True),
        dict(name='a8-reveal', flow=Fraction(1, 4), contacts=1, pressures=[1]*3, a8=True, erase=True, uniform=True),
        dict(name='dab-flow', flow=Fraction(1, 4), contacts=1, pressures=[1]*3, mode='dab'),
        dict(name='zero-flow', flow=Fraction(0), contacts=1, pressures=[1]*3),
    ]
    for case in rows:
        model = dict(width=WIDTH, height=HEIGHT, active=0, composition=copy.deepcopy(seed['composition']),
                     rasters=[[0]*AREA], masks={})
        if case.get('erase') or case.get('locked'):
            model['rasters'][0] = [ [0x00123456, 0x80335988, 0xff112233][i % 3] for i in range(AREA)]
        if case.get('locked'):
            model['composition']['layer_metadata'][0]['alpha_locked'] = True
        if case.get('selected'):
            values = [ [0, 64, 128, 255][i % 4] for i in range(AREA)]
            model['composition']['selection'] = dict(current=selection(values), previous=selection(values[::-1]))
            case['selection'] = values
        else:
            case['selection'] = [255]*AREA
        if case.get('a8'):
            model['rasters'][0] = [0xffe5c798]*AREA
            model['masks'] = {1: bytes([177]*AREA if case.get('uniform') else [i*7 for i in range(AREA)])}
            model['composition']['raster_masks'] = dict(version=1, next_id=2,
                target=dict(kind='mask', id=1), items=[dict(id=1, owner=dict(kind='layer', id=1),
                    density=173, feather_radius_px=1, linked=False)])
        case['model'] = model
    return rows


def caps(case):
    if not case.get('pressure_opacity'):
        return [Fraction(2, 5) if p > 0 else Fraction(0) for p in case['pressures']]
    floor = Fraction(str(case.get('floor', 0)))
    return [Fraction(2, 5)*(floor+(1-floor)*Fraction(str(p))) if p else Fraction(0) for p in case['pressures']]


def reference(case):
    rgba = list(case['model']['rasters'][0])
    raw = list(case['model']['masks'].get(1, []))
    influence = caps(case)
    for _ in range(case['contacts']):
        for i, selected in enumerate(case['selection']):
            amounts = [alpha(influence, case['flow'], selected)]
            if case.get('mode') == 'dab':
                amounts = [(rounded(255*cap*case['flow'])*selected+127)//255 for cap in influence]
            for amount in amounts:
                if case.get('a8'):
                    value = 255 if case.get('erase') else 0
                    raw[i] = (raw[i]*(255-amount)+value*amount+127)//255
                elif case.get('erase'):
                    rgba[i] = (rgba[i] & 0xffffff) | (((rgba[i] >> 24)*(255-amount)+127)//255 << 24)
                elif case.get('locked'):
                    rgba[i] = atop(rgba[i], amount)
                else:
                    rgba[i] = over(rgba[i], COLOR, amount)
    if case.get('a8'):
        coverage = editing.feather_density(raw, WIDTH, HEIGHT, 1, 173)
        surface = [editing.attenuate(p, m) for p, m in zip(rgba, coverage)]
    else:
        # Stored invisible RGB survives edits, while composition over transparency
        # canonicalizes a zero-alpha contribution to transparent black.
        surface = [editing.over(0, p) for p in rgba]
    return dict(rgba=rgba, mask=raw, png=b''.join(editing.rgba(p) for p in surface))


def commands(output, prefix, scenarios):
    plan = Plan()
    add = plan.add
    add('catalog', 'node.catalog')
    add('describe', 'app.describe')
    add('providers', 'context.read', resources=['plugins.registry'])
    for case in scenarios:
        key = case['name']
        add(key+'-open', 'document.open', path=str(case['path']))
        add(key+'-tool', 'tool.select', tool='eraser' if case.get('erase') else 'brush')
        add(key+'-options', 'brush.set', radius=20, hardness=1, opacity=.4, flow=float(case['flow']),
            opacity_mode=case.get('mode', 'stroke'), color='#000000' if case.get('a8') else '#194F93',
            spacing=.01, stabilizer=0, pressure_size=False, pressure_opacity=case.get('pressure_opacity', False),
            pressure_gamma=1, minimum_opacity=case.get('floor', 0))
        add(key+'-before', 'canvas.state')
        add(key+'-events-before', 'replay.events', limit=256)
        for contact in range(case['contacts']):
            points = [dict(x=2+(i % 2)*.5, y=2, pressure=p, time=100+i, pen=True)
                      for i, p in enumerate(case['pressures'])]
            add(f'{key}-stroke-{contact}', 'canvas.stroke', points=points)
        add(key+'-after', 'canvas.state')
        add(key+'-tool-context', 'context.read', resources=['painting.tool'])
        add(key+'-events', 'replay.events', limit=256)
        add(key+'-snapshot', 'canvas.snapshot')
        add(key+'-save', 'document.save', path=str(output/f'{prefix}-{key}.mcp'))
        if case.get('a8'):
            add(key+'-raw', 'mask.snapshot', mask_id=1, width=WIDTH, height=HEIGHT)
        # A contact with zero influence creates no undo record.
        if case['flow']:
            for contact in range(case['contacts']):
                add(f'{key}-undo-{contact}', 'history.undo')
            add(key+'-undo-save', 'document.save', path=str(output/f'{prefix}-{key}-undo.mcp'))
            for contact in range(case['contacts']):
                add(f'{key}-redo-{contact}', 'history.redo')
            add(key+'-redo-save', 'document.save', path=str(output/f'{prefix}-{key}-redo.mcp'))
        add(key+'-reopen', 'document.open', path=str(output/f'{prefix}-{key}.mcp'))
        add(key+'-reopen-state', 'canvas.state')
        add(key+'-reopen-png', 'canvas.snapshot')
        add(key+'-verify', 'replay.verify')
        add(key+'-seek', 'replay.seek')
        add(key+'-archive', 'replay.export', path=str(output/f'{prefix}-{key}.mcreplay'))
    return plan


def project(path, case, expected, original=False):
    saved = foundation.read_project(path)
    assert (saved['width'], saved['height'], saved['active']) == (WIDTH, HEIGHT, 0), path
    assert len(saved['rasters']) == 1, path
    rgba = case['model']['rasters'][0] if original else expected['rgba']
    assert saved['rasters'][0][1] == struct.pack('<'+'I'*AREA, *rgba), path
    assert saved['composition'] == case['model']['composition'], (path, saved['composition'], case['model']['composition'])
    if case.get('a8'):
        plane = saved['planes'][1]
        actual = plane*AREA if len(plane) == 1 else plane
        target = case['model']['masks'][1] if original else bytes(expected['mask'])
        assert actual == target, path
    else:
        assert not saved['planes'], path


def validate(values, plan, output, prefix, scenarios):
    at = lambda key: plan.at(values, key)
    nodes = at('catalog')['nodes']
    schema = next(n for n in nodes if n['id'] == 'action.brush.set')['parameters_schema']['properties']
    assert schema['flow']['minimum'] == 0 and schema['flow']['maximum'] == 1
    assert schema['opacity_mode']['enum'] == ['dab', 'stroke']
    resource = next(r for r in at('describe')['resources'] if r['id'] == 'painting.tool')['outputSchema']
    state_node = next(n for n in nodes if n['id'] == 'state.painting.tool')
    assert state_node['output_schema'] == resource
    parameters_port = next(p for p in state_node['outputs'] if p['id'] == 'parameters')
    assert parameters_port['data_type'] == 'object' and parameters_port['schema'] == resource['properties']['parameters']
    complete = resource['properties']['parameters']
    assert complete['additionalProperties'] is False
    assert set(complete['required']) == set(complete['properties'])
    assert {'flow', 'opacity_mode', 'spacing', 'minimum_opacity'} <= set(complete['required'])
    native = at('providers')['resources']['plugins.registry']['value']['native']
    assert next(p for p in native if p['id'] == 'painting')['tool_options'] == dict(flow=True, stroke_opacity=True)
    assert next(p for p in native if p['id'] == 'compositing')['tool_options'] == dict(flow=False, stroke_opacity=False)
    results = {}
    for case in scenarios:
        key, expected = case['name'], reference(case)
        before, after = at(key+'-before'), at(key+'-after')
        assert after['revision'] == before['revision']+case['contacts']*(len(case['pressures'])+2), key
        for state in [after, at(key+'-reopen-state')]:
            assert state['parameters']['flow'] == float(case['flow']), key
            assert state['parameters']['opacity_mode'] == case.get('mode', 'stroke'), key
            assert not state['drawing'], key
        tool = at(key+'-tool-context')['resources']['painting.tool']['value']
        assert tool['parameters'] == after['parameters'], key
        assert set(tool['parameters']) == set(complete['required']), key
        assert tool['capabilities'] == dict(flow=True, stroke_opacity=True), key
        png = base64.b64decode(at(key+'-snapshot')['data'])
        assert pixels(png) == (WIDTH, HEIGHT, expected['png']), key
        assert pixels(base64.b64decode(at(key+'-reopen-png')['data'])) == (WIDTH, HEIGHT, expected['png']), key
        project(output/f'{prefix}-{key}.mcp', case, expected)
        if case.get('a8'):
            assert at(key+'-raw')['samples'] == expected['mask'], key
        if case['flow']:
            project(output/f'{prefix}-{key}-undo.mcp', case, expected, original=True)
            project(output/f'{prefix}-{key}-redo.mcp', case, expected)
        before_count = len(at(key+'-events-before')['events'])
        events = at(key+'-events')['events'][before_count:]
        assert len(events) == case['contacts'], (key, events)
        for contact, event in enumerate(events):
            assert event['source']['action'] == 'canvas.stroke' and event['success'], (key, event)
            previous = reference(dict(case, contacts=contact))
            current = reference(dict(case, contacts=contact+1))
            # Replay counts the composed surface alongside raw RGBA tiles.
            rgba_damage = int(previous['rgba'] != current['rgba'])
            surface_damage = int(previous['png'] != current['png'])
            mask_damage = int(previous['mask'] != current['mask'])
            assert event['changed_tiles'] == rgba_damage+surface_damage, (key, event)
            assert event.get('changed_mask_tiles', 0) == mask_damage, (key, event)
        assert at(key+'-verify')['valid'], key
        results[key] = dict(png=png, seek=at(key+'-seek'))
    assert reference(scenarios[0])['rgba'][0] >> 24 == 102
    assert reference(scenarios[1])['rgba'][0] >> 24 == 163
    return results


def negative_commands(output, scenarios):
    plan = Plan()
    add = plan.add
    case = scenarios[0]
    add('open', 'document.open', path=str(case['path']))
    add('before', 'canvas.state')
    add('invalid-flow', 'brush.set', flow=1.1, error='invalid_arguments')
    add('invalid-mode', 'brush.set', opacity_mode='unknown', error='invalid_arguments')
    plan.failed_pipeline('atomic', atomic=True, steps=[
        dict(id='options', action='brush.set', args=dict(opacity_mode='stroke', flow=.25, opacity=.4, radius=20,
                                                       hardness=1, stabilizer=0, pressure_size=False, spacing=.01)),
        dict(id='paint', action='canvas.stroke', args=dict(points=[dict(x=2, y=2), dict(x=2.5, y=2)])),
        dict(id='fail', action='layer.select', args=dict(index=15))])
    add('after', 'canvas.state')
    add('restored-save', 'document.save', path=str(output/'atomic-restored.mcp'))
    add('verify', 'replay.verify')
    return plan


def validate_negatives(values, plan, output, case):
    assert editing.document_state(plan.at(values, 'before')) == editing.document_state(plan.at(values, 'after'))
    assert plan.at(values, 'atomic')['rolled_back']
    project(output/'atomic-restored.mcp', case, reference(case), original=True)
    assert plan.at(values, 'verify')['valid']


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
        assert pixels(base64.b64decode(plan.at(values, key+'-png')['data'])) == pixels(expected[key]['png'])
        for field in ['state_hash', 'surface_hash']:
            assert plan.at(values, key+'-seek')[field] == expected[key]['seek'][field]


def gui_checks(cli, gui, flags, profile, output, scenarios):
    startup = subprocess.STARTUPINFO()
    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = 0
    process = subprocess.Popen([str(gui), *flags], startupinfo=startup)
    endpoint = profile/'session.json'

    def call(action, args):
        result = subprocess.run([str(cli), 'call', action, json.dumps(args), '--attach', str(endpoint)],
                                capture_output=True, text=True, encoding='utf-8', timeout=30)
        assert result.returncode == 0, (action, result.stdout, result.stderr)
        return json.loads(result.stdout)['result']

    try:
        deadline = time.monotonic()+30
        while not endpoint.exists():
            assert process.poll() is None and time.monotonic() < deadline, 'Isolated desktop startup failed'
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
    output = ROOT/'target/test-output/stroke-opacity-smoke'/str(time.time_ns())
    output.mkdir(parents=True)
    print(output, flush=True)
    plugins, profile = output/'plugins', output/'profile'
    shutil.copytree(ROOT/'plugins', plugins)
    profile.mkdir()
    (profile/'settings.json').write_text(json.dumps(dict(version=1, custom=dict(keep=42))), encoding='utf-8')
    flags = ['--plugins', str(plugins), '--config-dir', str(profile)]
    bootstrap = Plan()
    bootstrap.add('new', 'canvas.new', width=WIDTH, height=HEIGHT)
    bootstrap.add('save', 'document.save', path=str(output/'seed.mcp'))
    geometry.run_jsonl(cli, flags, bootstrap, output/'bootstrap.jsonl')
    scenarios = cases(foundation.read_project(output/'seed.mcp'))
    for case in scenarios:
        case['path'] = output/(case['name']+'.mcp')
        geometry.write_model(case['path'], case['model'])
    baseline = None
    for prefix, runner in [('jsonl', lambda p: geometry.run_jsonl(cli, flags, p, output/'stroke-opacity.jsonl')),
                           ('mcp', lambda p: editing.run_mcp(cli, flags, p))]:
        plan = commands(output, prefix, scenarios)
        actual = validate(runner(plan), plan, output, prefix, scenarios)
        if baseline is not None:
            assert {k: pixels(v['png']) for k, v in actual.items()} == {k: pixels(v['png']) for k, v in baseline.items()}
        baseline = actual
        print(f'{prefix.upper()}: {len(scenarios)} tiny cases / {len(plan.rows)} commands passed independent raw/outcome references.', flush=True)
    negatives = negative_commands(output, scenarios)
    validate_negatives(editing.run_mcp(cli, flags, negatives), negatives, output, scenarios[0])
    replay_without_dlls(cli, profile, output, scenarios, baseline)
    if not args.skip_gui:
        gui_checks(cli, gui, flags, profile, output, scenarios)
    (output/'metrics.json').write_text(json.dumps(dict(cases=len(scenarios), width=WIDTH, height=HEIGHT,
        synthetic_input=True, physical_tablet_tested=False, performance_measured=False,
        fault_injection='Rust regressions; no public hook'), indent=2)+'\n', encoding='utf-8')
    print('Flow/capped opacity, raw A8/selection, grouped Undo, atomic rejection and DLL-free replay passed.', flush=True)
    print(output)


if __name__ == '__main__':
    main()
