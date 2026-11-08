"""Verify published built-in result revisions through JSONL, MCP and GUI attach.

Tiny fixtures exercise optimistic chaining without rewriting historical reports or
DLL-owned JSON. Synthetic pressure input is not a tablet or throughput benchmark.
JSONL batches stop at a failure; persistent MCP clients also verify restored state.
"""
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
INITIALIZE = [dict(jsonrpc='2.0', id=1, method='initialize',
                   params=dict(protocolVersion='2025-11-25')),
              dict(jsonrpc='2.0', method='notifications/initialized')]


def action(action_name, **args):
    return dict(action=action_name, args=args)


def step(key, action_name, **args):
    return dict(id=key, **action(action_name, **args))


def operations(output, prefix):
    steps, pairs = [step('before', 'canvas.state')], []

    def pair(key, action_name, revision=True, mutation=True, **args):
        steps.extend([step(key, action_name, **args), step(key+'-query', 'canvas.state')])
        pairs.append((key, revision, mutation))

    pair('new', 'canvas.new', width=8, height=6)
    pair('rename', 'layer.rename', layer_id=1, name='Published cel')
    steps.append(step('chain', 'pipeline.run',
                      expected_revision={'$ref': 'rename#/result/revision'},
                      steps=[step('accepted', 'canvas.state')]))
    pair('rename-noop', 'layer.rename', layer_id=1, name='Published cel')
    pair('add', 'layer.add', revision=False)
    pair('select', 'layer.selection.set', layer_ids=[1, 2], active_layer_id=1)
    pair('batch', 'layer.batch', action='layer.set', layer_ids=[1, 2], args=dict(color_label='violet'))
    pair('batch-undo', 'history.undo')
    pair('batch-redo', 'history.redo')
    pair('folder', 'group.create', members=[dict(kind='layer', id=2)], name='Color')
    pair('folder-set', 'group.set', group_id=1, opacity=.7)
    pair('folder-move', 'group.move', items=[dict(kind='layer', id=2)], parent_id=0, index=1)
    pair('folder-delete', 'group.delete', group_id=1)
    steps.append(step('source-graph', 'graph.read'))
    pair('graph', 'graph.set', revision=False, graph={'$ref': 'source-graph#/result/graph'})
    pair('density', 'canvas.resolution', revision=False, resolution_ppi=144)
    pair('shape', 'selection.shape', kind='rectangle', x=1, y=1, width=5, height=3)
    pair('selection-invert', 'selection.invert')
    pair('deselect', 'selection.deselect')
    pair('mask', 'mask.create', revision=False, owner=dict(kind='layer', id=1), select=False)
    pair('mask-fill', 'mask.fill', revision=False, mask_id=1, value=190)
    pair('mask-set', 'mask.set', revision=False, mask_id=1, density=177, feather_radius_px=1)
    pair('mask-target', 'mask.target', revision=False, target=dict(kind='mask', id=1))
    pair('frame-noop', 'canvas.resize', width=8, height=6)
    pair('mask-inspect', 'mask.query', mutation=False)
    pair('export-plan', 'canvas.export.plan', mutation=False)
    pair('export', 'canvas.export', path=str(output/f'{prefix}.png'))
    pair('save', 'document.save', revision=False, path=str(output/f'{prefix}.mcp'))
    pair('open', 'document.open', path=str(output/f'{prefix}.mcp'))
    assert len({row['id'] for row in steps}) == len(steps), 'Duplicate fixture step IDs'
    return action('pipeline.run', steps=steps), pairs


def historical():
    return action('pipeline.run', steps=[
        step('new', 'canvas.new', width=8, height=6),
        step('first', 'layer.rename', layer_id=1, name='First result'),
        step('nested', 'pipeline.run', steps=[
            step('second', 'layer.rename', layer_id=1, name='Second result'),
            step('observed', 'canvas.state'),
        ]),
        step('last', 'layer.rename', layer_id=1, name='Last result'),
        step('atomic', 'pipeline.run', atomic=True, steps=[
            step('committed', 'layer.rename', layer_id=1, name='Atomic result'),
            step('observed', 'canvas.state'),
        ]),
        step('workflow', 'workflow.run', graph=dict(version=1, nodes=[
            dict(id=1, type='action.layer.rename', position=[0, 0],
                 parameters=dict(layer_id=1, name='Workflow first')),
            dict(id=2, type='action.layer.rename', position=[360, 0],
                 parameters=dict(layer_id=1, name='Workflow last')),
        ], edges=[dict(**{'from': 1}, output='$exec', to=2, input='$exec')])),
        step('final', 'canvas.state'),
    ])


def native():
    return action('pipeline.run', steps=[
        step('new', 'canvas.new', width=8, height=6),
        step('private', 'assistant.checkerboard', cell=2),
        step('query', 'canvas.state'),
        step('context', 'assistant.context'),
    ])


def contacts():
    return action('pipeline.run', steps=[
        step('new', 'canvas.new', width=8, height=6),
        step('options', 'brush.set', radius=1, color='#5477CC', stabilizer=.55,
             pressure_size=True, pressure_opacity=True),
        step('down', 'canvas.pointer', phase='down', x=2, y=2, pressure=.3, pen=True, time=100, tool_override='brush'),
        step('during-down', 'canvas.state'),
        step('context', 'context.read', resources=['input.pointer', 'painting.tool']),
        step('move', 'canvas.pointer', phase='move', x=4, y=3, pressure=.9, pen=True, time=120, tool_override='brush'),
        step('during-move', 'canvas.state'),
        step('describe', 'app.describe'),
        step('up', 'canvas.pointer', phase='up', x=6, y=4, pressure=.4, pen=True, time=140, tool_override='brush'),
        step('after', 'canvas.state'),
        step('events', 'replay.events', limit=256),
        step('verify', 'replay.verify'),
    ])


def rollback():
    return action('pipeline.run', atomic=True, steps=[
        step('attempt', 'layer.rename', layer_id=1, name='Rolled back attempt'),
        step('reject', 'layer.select', layer_id=4294967295),
    ])


def rows(output, prefix):
    normal, pairs = operations(output, prefix)
    return [normal, historical(), native(), contacts(),
            action('settings.read', path=str(output/'foreign-settings.json')),
            action('canvas.state'), rollback()], pairs


def envelope(value, expected_revision_field=True):
    assert isinstance(value['revision'], int), value
    result = value['result']
    if expected_revision_field:
        assert result['revision'] == value['revision'], value
    else:
        assert 'revision' not in result, value
    return result


def validate(values, pairs):
    assert len(values) == 7
    normal = envelope(values[0])
    assert normal['ok']
    results = normal['results']
    previous = envelope(results['before'])['revision']
    for key, revision, mutation in pairs:
        value = results[key]
        envelope(value, revision)
        query = envelope(results[key+'-query'])
        assert value['revision'] == query['revision'], (key, value, query)
        assert value['revision'] == previous + int(mutation), (key, value['revision'], previous)
        previous = query['revision']
    assert normal['revision'] == previous
    chain = envelope(results['chain'])
    assert chain['ok'] and envelope(chain['results']['accepted'])['revision'] == results['rename']['revision']
    assert results['new']['result']['revision'] < normal['revision']
    assert results['batch']['revision'] == results['select']['revision']+1
    assert results['folder-delete']['result']['deleted_group_ids'] == [1]

    history = envelope(values[1])
    first = history['results']['first']
    nested = envelope(history['results']['nested'])
    second = nested['results']['second']
    last = history['results']['last']
    for value in [first, second, last, nested['results']['observed'], history['results']['final']]:
        envelope(value)
    assert first['revision'] < second['revision'] == nested['revision'] < last['revision']
    atomic = envelope(history['results']['atomic'])
    assert atomic['ok'] and atomic['transactional'] and not atomic['rolled_back']
    assert envelope(atomic['results']['committed'])['revision'] == atomic['revision']
    assert envelope(atomic['results']['observed'])['revision'] == atomic['revision']
    assert last['revision'] < atomic['revision'] < history['revision']
    workflow = envelope(history['results']['workflow'], False)
    first_node, last_node = [workflow['results'][key] for key in ['node_1', 'node_2']]
    envelope(first_node)
    envelope(last_node)
    assert workflow['ok'] and first_node['revision'] < last_node['revision'] == history['revision']
    assert workflow['execution']['revision'] == last_node['revision']

    private_report = envelope(values[2])
    private = private_report['results']['private']
    # Native-root publication folds provisional child increments into one
    # revision. The DLL's forwarded report remains its historical observation.
    native_start = private_report['results']['new']['revision']
    assert private['revision'] == native_start+1
    assert private['result']['results']['before']['revision'] == native_start
    assert private['result']['results']['paint']['revision'] == native_start+2
    assert private['result']['revision'] == native_start+2
    assert private['result']['revision'] == private['result']['results']['after']['revision']
    assert envelope(private_report['results']['query'])['revision'] == private['revision']
    assert private_report['results']['context']['result']['revision'] == private['revision']

    contact = envelope(values[3])['results']
    for key in ['options', 'down', 'move', 'up']:
        envelope(contact[key], False)
    down, moved, after = [envelope(contact[key]) for key in ['during-down', 'during-move', 'after']]
    assert down['drawing'] and moved['drawing'] and not after['drawing']
    assert contact['down']['revision'] == down['revision'] == contact['context']['revision']
    assert contact['move']['revision'] == moved['revision'] == contact['describe']['revision']
    assert contact['context']['result']['revision'] == down['revision']
    observed_pressure = contact['context']['result']['resources']['input.pointer']['value']['last']['pressure']
    assert abs(observed_pressure-.3) < 1e-6
    assert contact['describe']['result']['revision'] == moved['revision']
    samples = [sample for event in contact['events']['result']['events']
               if event['source']['kind'] == 'pointer' for sample in event['source']['samples']]
    assert [sample['time'] for sample in samples] == [100, 120, 140], samples
    assert all(sample['pen'] for sample in samples)
    assert all(abs(sample['pressure']-pressure) < 1e-6 for sample, pressure in zip(samples, [.3, .9, .4]))
    assert contact['verify']['result']['valid']
    # SettingsRead is opaque even if a supported unknown root key happens to
    # be called revision. A Session-owned result policy must not touch it.
    assert values[4]['result']['revision'] == 777
    before = envelope(values[5])
    failed = envelope(values[6])
    assert failed['ok'] is False and failed['rolled_back'] is True
    assert values[6]['revision'] == before['revision']
    attempt = failed['results']['attempt']
    envelope(attempt)
    assert attempt['revision'] == before['revision']+1
    assert attempt['result']['layers'][0]['name'] == 'Rolled back attempt'
    return before


def jsonl(cli, flags, commands, path):
    path.write_text('\n'.join(json.dumps(row) for row in commands)+'\n', encoding='utf-8')
    run = subprocess.run([str(cli), 'run', str(path), *flags], capture_output=True,
                         text=True, encoding='utf-8', timeout=90)
    # The final rollback report is a deliberate failed operation; JSONL stops.
    assert run.returncode != 0 and 'batch_failed' in run.stderr, run.stderr+run.stdout[-2000:]
    replies = [json.loads(line) for line in run.stdout.splitlines()]
    assert len(replies) == len(commands), run.stderr+run.stdout[-2000:]
    assert all(row['ok'] for row in replies[:-1]) and not replies[-1]['ok']
    return [row['data'] for row in replies]


def mcp(cli, flags, commands):
    requests = INITIALIZE + [dict(jsonrpc='2.0', id=10+index, method='tools/call',
                                 params=dict(name=row['action'], arguments=row['args']))
                              for index, row in enumerate(commands)]
    run = subprocess.run([str(cli), 'mcp', *flags],
                         input='\n'.join(json.dumps(row) for row in requests)+'\n',
                         capture_output=True, text=True, encoding='utf-8', timeout=90)
    assert run.returncode == 0, run.stderr+run.stdout[-2000:]
    replies = {row['id']: row for row in map(json.loads, run.stdout.splitlines()) if 'id' in row}
    values = []
    for index, command in enumerate(commands):
        reply = replies[10+index]
        assert 'error' not in reply, (command, reply)
        transport = reply['result']
        value = transport['structuredContent']
        assert 'result' in value, (command, transport)
        assert bool(transport.get('isError')) == (value['result'].get('ok') is False), (command, transport)
        values.append(value)
    return values


def persistent(cli, flags, output, prefix):
    commands, pairs = rows(output, prefix)
    commands.extend([action('canvas.state'), action('replay.verify')])
    values = mcp(cli, flags, commands)
    before = validate(values[:7], pairs)
    after = envelope(values[7])
    assert after == before, 'Atomic rollback did not restore the queried Session state'
    assert values[8]['result']['valid']


def gui_checks(cli, gui, plugins, profile, output):
    startup = subprocess.STARTUPINFO()
    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = 0
    process = subprocess.Popen([str(gui), '--plugins', str(plugins), '--config-dir', str(profile)], startupinfo=startup)
    endpoint = profile/'session.json'
    try:
        deadline = time.monotonic()+30
        while not endpoint.exists():
            assert process.poll() is None and time.monotonic() < deadline, 'GUI startup failed'
            time.sleep(.05)
        persistent(cli, ['--attach', str(endpoint)], output, 'gui')
        run = subprocess.run([str(cli), 'call', 'ui.dispatch', '{"id":"close","value":true}',
                              '--attach', str(endpoint)], capture_output=True, text=True, timeout=30)
        assert run.returncode == 0, run.stderr
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
    assert cli.is_file() and (args.skip_gui or gui.is_file()), 'Build/stage matching binaries and DLLs first'
    output = ROOT/'target/test-output/result-revisions-smoke'/str(time.time_ns())
    output.mkdir(parents=True)
    print(output, flush=True)
    plugins = output/'plugins'
    shutil.copytree(ROOT/'plugins', plugins)
    (output/'foreign-settings.json').write_text(json.dumps(dict(version=1, revision=777)), encoding='utf-8')
    for prefix in ['jsonl', 'mcp', 'gui']:
        (output/prefix).mkdir()
    commands, pairs = rows(output, 'jsonl')
    validate(jsonl(cli, ['--plugins', str(plugins), '--config-dir', str(output/'jsonl')],
                   commands, output/'revisions.jsonl'), pairs)
    print('JSONL: published results, historical reports, native ownership and terminal rollback report passed.', flush=True)
    persistent(cli, ['--plugins', str(plugins), '--config-dir', str(output/'mcp')], output, 'mcp')
    print('MCP: the same contracts and a fresh atomic-restoration query passed.', flush=True)
    if not args.skip_gui:
        gui_checks(cli, gui, plugins, output/'gui', output)
        print('GUI attachment: identical revision contracts and pressure-query continuity passed.', flush=True)
    (output/'metrics.json').write_text(json.dumps(dict(width=8, height=6, builtin_pairs=len(pairs),
        transports=['jsonl', 'mcp'] + ([] if args.skip_gui else ['gui']), synthetic_input=True,
        physical_tablet_tested=False, screenshots='No presentation changes; not required.'), indent=2)+'\n', encoding='utf-8')
    print(output)


if __name__ == '__main__':
    main()
