"""Check native direct HSL commands, selection, Undo and workflow transport parity."""
import argparse
import importlib.util
from pathlib import Path
import shutil
import time

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('hsl_geometry_helpers', ROOT/'scripts/smoke-mask-geometry.py')
geometry = importlib.util.module_from_spec(spec)
spec.loader.exec_module(geometry)
editing, Plan = geometry.editing, geometry.Plan


def commands():
    p = Plan()
    p.add('nodes', 'node.catalog')
    p.add('new', 'canvas.new', width=4, height=4)
    p.add('fill', 'layer.fill_color', color='#ff000080')
    p.add('selection', 'selection.shape', kind='rectangle', x=1, y=1, width=2, height=2)
    p.add('hue', 'layer.hue_saturation', hue=120)
    p.add('green', 'histogram.query', source='layer', layer_id=1)
    p.add('undo', 'history.undo')
    p.add('red', 'histogram.query', source='layer', layer_id=1)
    p.add('redo', 'history.redo')
    p.add('green-again', 'histogram.query', source='layer', layer_id=1)
    p.add('new-workflow', 'canvas.new', width=4, height=4)
    p.add('fill-workflow', 'layer.fill_color', color='#ff000080')
    p.add('workflow', 'workflow.set', graph=dict(version=1, nodes=[dict(
        id=1, type='action.layer.hue_saturation', position=[0, 0],
        parameters=dict(hue=-120))], edges=[]))
    p.add('dry', 'workflow.validate')
    p.add('run', 'workflow.run', atomic=True)
    p.add('blue', 'histogram.query', source='layer', layer_id=1)
    p.add('verify', 'replay.verify')
    return p


def validate(values, plan):
    at = lambda key: plan.at(values, key)
    assert any(node['id'] == 'action.layer.hue_saturation' for node in at('nodes')['nodes'])
    assert at('hue')['implementation'] == 'independent Rust DLL'
    result = {}
    for key, colors in [('green', [(255, 0, 0)]*12+[(0, 255, 0)]*4),
                        ('red', [(255, 0, 0)]*16),
                        ('green-again', [(255, 0, 0)]*12+[(0, 255, 0)]*4),
                        ('blue', [(0, 0, 255)]*16)]:
        actual = at(key)
        assert actual['exact'] and actual['nontransparent_pixels'] == 16
        for channel, index in [('red', 0), ('green', 1), ('blue', 2)]:
            expected = [sum(color[index] == byte for color in colors) for byte in range(256)]
            assert actual[channel] == expected, (key, channel)
        result[key] = actual['red'], actual['green'], actual['blue']
    assert at('dry')['ok'] and at('run')['ok'] and at('verify')['valid']
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli', default='target/build/x86_64-pc-windows-gnu/release/micopi-cli.exe')
    cli = (ROOT/parser.parse_args().cli).resolve()
    assert cli.is_file(), 'Build/stage matching native DLLs first'
    output = ROOT/'target/test-output/hue-saturation-smoke'/str(time.time_ns())
    output.mkdir(parents=True)
    plugins, profile = output/'plugins', output/'profile'
    shutil.copytree(ROOT/'plugins', plugins)
    profile.mkdir()
    flags = ['--plugins', str(plugins), '--config-dir', str(profile)]
    plan = commands()
    batch = validate(geometry.run_jsonl(cli, flags, plan, output/'hue-saturation.jsonl'), plan)
    mcp = validate(editing.run_mcp(cli, flags, plan, require_masks=False), plan)
    assert batch == mcp
    print('Hue/Saturation: independent primary-color bins, selection, Undo/Redo, atomic workflow, replay and JSONL/MCP parity passed.')
    print(output)


if __name__ == '__main__':
    main()
