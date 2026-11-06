"""Check actual DLL A8 primitives; this does not claim application mask support.

Synthetic synchronous loans exercise the native ABI without Session, history,
project storage or physical tablet hardware. No GUI or build tools are started.
"""
import argparse
import ctypes as C
import hashlib
import json
import math
from pathlib import Path
import time

ROOT = Path(__file__).resolve().parents[1]
U32, U8, SIZE, I32, PTR = C.c_uint32, C.c_uint8, C.c_size_t, C.c_int32, C.c_void_p


class Header(C.Structure):
    _fields_ = [('version', U32), ('size', U32)]


class Region(C.Structure):
    _fields_ = [(name, U32) for name in ['x', 'y', 'width', 'height']]


class Pixels(C.Structure):
    _fields_ = [('data', C.POINTER(U32)), ('len', SIZE)] + [(name, U32) for name in ['width', 'height', 'stride', 'origin_x', 'origin_y']]


class A8Pixels(C.Structure):
    _fields_ = [('data', C.POINTER(U8)), ('len', SIZE)] + [(name, U32) for name in ['width', 'height', 'stride', 'origin_x', 'origin_y']]


Transform = C.CFUNCTYPE(I32, PTR, C.POINTER(Pixels))
A8Transform = C.CFUNCTYPE(I32, PTR, C.POINTER(A8Pixels))
Loan = C.CFUNCTYPE(I32, PTR, Region, Transform, PTR)
A8Loan = C.CFUNCTYPE(I32, PTR, Region, A8Transform, PTR)
Pick = C.CFUNCTYPE(I32, PTR, C.c_float, C.c_float, C.POINTER(U32))


class ToolHost(C.Structure):
    _fields_ = [('version', U32), ('size', U32), ('context', PTR), ('pixels', Loan), ('pick', Pick)]


class ToolHostA8(C.Structure):
    _fields_ = [('base', ToolHost), ('a8', A8Loan)]


class Parameters(C.Structure):
    _fields_ = [(name, C.c_float) for name in ['radius', 'opacity', 'hardness']] + [('color', U32)] + [(name, C.c_float) for name in ['stabilizer', 'gamma', 'minimum_radius']] + [('flags', U32)]


class Sample(C.Structure):
    _fields_ = [('phase', U32), ('flags', U32)] + [(name, C.c_float) for name in ['x', 'y', 'pressure']] + [('time', U32)] + [(name, C.c_float) for name in ['view_x', 'view_y']] + [('width', U32), ('height', U32), ('zoom', C.c_float)]


class ToolOutput(C.Structure):
    _fields_ = [('flags', U32), ('color', U32)] + [(name, C.c_float) for name in ['zoom', 'pan_dx', 'pan_dy']]


Create = C.CFUNCTYPE(PTR)
Destroy = C.CFUNCTYPE(None, PTR)
Pointer = C.CFUNCTYPE(I32, PTR, U32, C.POINTER(Sample), C.POINTER(Parameters), C.POINTER(ToolHost), C.POINTER(ToolOutput))


class ToolApi(C.Structure):
    _fields_ = [('version', U32), ('size', U32), ('create', Create), ('destroy', Destroy), ('pointer', Pointer)]


class Options(C.Structure):
    _fields_ = [('version', U32), ('size', U32), ('spacing', C.c_float), ('minimum_opacity', C.c_float)]


class OptionsExt(C.Structure):
    _fields_ = [('base', Options), ('flags', U32)]


class Coverage(C.Structure):
    _fields_ = [('data', C.POINTER(U8)), ('len', SIZE), ('width', U32), ('height', U32), ('stride', U32)]


class OptionsSelection(C.Structure):
    _fields_ = [('base', OptionsExt), ('selection', Coverage)]


Configure = C.CFUNCTYPE(I32, PTR, C.POINTER(Options))


class OptionsApi(C.Structure):
    _fields_ = [('version', U32), ('size', U32), ('configure', Configure)]


class OptionsApiExt(C.Structure):
    _fields_ = [('base', OptionsApi), ('features', U32)]


class Layer(C.Structure):
    _fields_ = [('data', C.POINTER(U32)), ('len', SIZE), ('opacity', U32), ('visible', U32), ('mode', U32)]


class Mask(C.Structure):
    _fields_ = [('data', C.POINTER(U8)), ('len', SIZE), ('stride', U32), ('constant', U32)]


class Node(C.Structure):
    _fields_ = [('kind', U32), ('a', U32), ('b', U32), ('mode', U32), ('amount', C.c_float)]


Render = C.CFUNCTYPE(I32, C.POINTER(Node), SIZE, C.POINTER(Layer), SIZE, U32, U32, C.POINTER(Pixels))
RenderMasked = C.CFUNCTYPE(I32, C.POINTER(Node), SIZE, C.POINTER(Layer), SIZE, C.POINTER(Mask), SIZE, U32, U32, C.POINTER(Pixels))


class Compositor(C.Structure):
    _fields_ = [('version', U32), ('size', U32), ('render', Render)]


class CompositorFeatures(C.Structure):
    _fields_ = [('base', Compositor), ('capabilities', C.c_uint64)]


class CompositorModes(C.Structure):
    _fields_ = [('base', CompositorFeatures), ('modes', C.c_uint64)]


class CompositorMasks(C.Structure):
    _fields_ = [('base', CompositorModes), ('render_masked', RenderMasked)]


def pointer_at(array, byte_offset, element):
    return C.cast(C.byref(array, byte_offset), C.POINTER(element))


def entry(library, symbol, table):
    function = getattr(library, symbol)
    function.argtypes, function.restype = [], C.POINTER(Header)
    address = function()
    assert address and address.contents.version == 1, f'{symbol}: invalid header'
    assert address.contents.size >= C.sizeof(table), f'{symbol}: incomplete {table.__name__}'
    return C.cast(address, C.POINTER(table)).contents


class Painting:
    def __init__(self, path, require_a8=True):
        self.library = C.CDLL(str(path))
        self.tools = entry(self.library, 'micopi_tools_v1', ToolApi)
        self.options = entry(self.library, 'micopi_tool_options_v1', OptionsApiExt)
        assert self.tools.create and self.tools.destroy and self.tools.pointer and self.options.base.configure
        assert bool(self.options.features & 4) == require_a8, 'A8 capability mismatch'

    def configure(self, state, flags, selection=None, spacing=.17, minimum=.13):
        value = OptionsSelection(OptionsExt(Options(1, C.sizeof(OptionsSelection), spacing, minimum), flags), selection or Coverage())
        return self.options.base.configure(state, C.cast(C.byref(value), C.POINTER(Options)))


class Canvas:
    def __init__(self, initial, width=96, height=64):
        self.width, self.height, self.stride, self.guard = width, height, width+5, 16
        length = self.guard*2+self.stride*height
        self.a8 = (U8*length)(*([231]*length))
        self.rgba = (U32*length)(*([0xe1d2c3b4]*length))
        for y in range(height):
            for x in range(width):
                self.a8[self.guard+y*self.stride+x] = initial
                self.rgba[self.guard+y*self.stride+x] = 0xff000000 | initial*0x010101
        self.calls = dict(a8=0, rgba=0)
        self.errors = []
        self.rgba_loan = Loan(lambda context, region, transform, user: self.loan(region, transform, user, False))
        self.a8_loan = A8Loan(lambda context, region, transform, user: self.loan(region, transform, user, True))
        self.pick = Pick(lambda context, x, y, value: -1)
        self.host = ToolHostA8(ToolHost(1, 40, None, self.rgba_loan, self.pick), self.a8_loan)

    def loan(self, r, transform, user, a8):
        try:
            if not transform or not r.width or not r.height or r.x+r.width > self.width or r.y+r.height > self.height:
                raise ValueError('Out-of-frame native loan')
            self.calls['a8' if a8 else 'rgba'] += 1
            index = self.guard+r.y*self.stride+r.x
            size = (r.height-1)*self.stride+r.width
            data = self.a8 if a8 else self.rgba
            descriptor = (A8Pixels if a8 else Pixels)(pointer_at(data, index*(1 if a8 else 4), U8 if a8 else U32), size, r.width, r.height, self.stride, r.x, r.y)
            return transform(user, C.byref(descriptor))
        except Exception as error:
            self.errors.append(str(error))
            return -1

    def values(self, a8):
        data = self.a8 if a8 else self.rgba
        return [data[self.guard+y*self.stride+x] for y in range(self.height) for x in range(self.width)]

    def check_guards(self):
        assert not self.errors, self.errors
        positions = list(range(self.guard))+list(range(len(self.a8)-self.guard, len(self.a8)))
        positions += [self.guard+y*self.stride+x for y in range(self.height) for x in range(self.width, self.stride)]
        assert all(self.a8[i] == 231 and self.rgba[i] == 0xe1d2c3b4 for i in positions), 'Loan overwrote row padding/guards'


def sample(i, phase, pen=True, eraser=False):
    return Sample(phase, int(pen) | (2 if eraser else 0), 12+i*3.5, 25+math.sin(i*.43)*11,
                  0 if i == 0 or phase == 2 else .05+(i*7 % 13)/14,
                  (0xfffffff0+i*4) & 0xffffffff, 0, 0, 96, 64, 1)


def stroke(plugin, canvas, a8, handler=1, color=0x2468da, strength=.75, selected=True, pen=True, eraser=False, spacing=.17):
    selection_data = (U8*(99*64))(*([0]*(99*64)))
    for y in range(64):
        for x in range(96):
            selection_data[y*99+x] = [0, 64, 128, 255][(x//7+y//5) % 4]
    selection = Coverage(selection_data, len(selection_data), 96, 64, 99) if selected else Coverage()
    state = plugin.tools.create()
    assert state, 'Native tool creation failed'
    try:
        assert plugin.configure(state, (4 if a8 else 0) | (2 if selected else 0), selection, spacing) == 0
        params = Parameters(6.5, .72, .45, color, strength, 1.5, .08, 3)
        for i in range(20):
            point = sample(i, 0 if i == 0 else 2 if i == 19 else 1, pen, eraser)
            output = ToolOutput()
            assert plugin.tools.pointer(state, handler, C.byref(point), C.byref(params) if i == 0 else None, C.byref(canvas.host.base), C.byref(output)) == 0, f'Pointer {i} failed'
            if i == 8:
                assert plugin.configure(state, 0) != 0, 'Reconfigured active contact'
        if selected:
            result = canvas.values(a8)
            initial = 211 if a8 else 0xffd3d3d3
            assert all(result[y*96+x] == initial for y in range(64) for x in range(96) if selection_data[y*99+x] == 0), 'Selection exclusion changed'
    finally:
        plugin.tools.destroy(state)
    canvas.check_guards()


def check_painting(plugin, old):
    checks = 0
    for handler, color, eraser in [(1, 0x2468da, False), (2, 0, False), (1, 0x2468da, True)]:
        gray = 255 if handler == 2 or eraser else (54*0x24+183*0x68+19*0xda+128)//256
        for strength, pen, selected, spacing in [(0, True, False, .11), (.75, True, True, .17), (1, False, True, .33)]:
            a8, rgba = Canvas(211), Canvas(211)
            rgba.host.base.size = 32
            stroke(plugin, a8, True, handler, color, strength, selected, pen, eraser, spacing)
            stroke(plugin, rgba, False, 1, gray*0x010101, strength, selected, pen, False, spacing)
            assert a8.values(True) == [p & 255 for p in rgba.values(False)], 'A8/opaque-grayscale stroke mismatch'
            assert a8.calls['rgba'] == 0 and rgba.calls['a8'] == 0 and a8.calls['a8'] == rgba.calls['rgba'] > 0
            assert all(p == 0xffd3d3d3 for p in a8.values(False)), 'A8 target wrote RGBA'
            assert min(a8.values(True)) < 211 if gray < 211 else max(a8.values(True)) > 211
            if old:
                baseline = Canvas(211)
                baseline.host.base.size = 32
                stroke(old, baseline, False, 1, gray*0x010101, strength, selected, pen, False, spacing)
                assert baseline.values(False) == rgba.values(False), 'Legacy RGBA DLL bytes changed'
            checks += 1
    for size, null in [(32, False), (39, False), (40, True)]:
        canvas, state = Canvas(211), plugin.tools.create()
        try:
            canvas.host.base.size = size
            if null:
                canvas.host.a8 = A8Loan()
            assert plugin.configure(state, 4) == 0
            before = bytes(canvas.a8), bytes(canvas.rgba)
            p, params, output = sample(0, 0), Parameters(4, 1, 1, 0, 0, 1, .08, 3), ToolOutput()
            assert plugin.tools.pointer(state, 1, C.byref(p), C.byref(params), C.byref(canvas.host.base), C.byref(output)) != 0
            assert (bytes(canvas.a8), bytes(canvas.rgba)) == before and canvas.calls == dict(a8=0, rgba=0)
        finally:
            plugin.tools.destroy(state)
        checks += 1
    # An interior pixel at a mouse DOWN has exact opacity 128; independently
    # check grayscale conversion and single selection scaling/interpolation.
    for handler, expected_value in [(1, 98), (2, 255)]:
        canvas, state = Canvas(73), plugin.tools.create()
        coverage = (U8*(96*64))(*([128]*(96*64)))
        try:
            assert plugin.configure(state, 6, Coverage(coverage, len(coverage), 96, 64, 96), minimum=0) == 0
            p = Sample(0, 0, 30.5, 30.5, 1, 0, 0, 0, 96, 64, 1)
            params, output = Parameters(3, .5, 1, 0x2468da, 0, 1, .08, 0), ToolOutput()
            assert plugin.tools.pointer(state, handler, C.byref(p), C.byref(params), C.byref(canvas.host.base), C.byref(output)) == 0
            opacity = (128*128+127)//255
            expected = (73*(255-opacity)+expected_value*opacity+127)//255
            assert canvas.values(True)[30*96+30] == expected, 'Independent integer dab reference failed'
            p.phase = 3
            assert plugin.tools.pointer(state, handler, C.byref(p), None, C.byref(canvas.host.base), C.byref(output)) == 0
        finally:
            plugin.tools.destroy(state)
        canvas.check_guards()
        checks += 1
    return checks


def source(pixel, opacity=255):
    alpha = (pixel >> 24)*opacity//255
    return (pixel & 0xffffff) | (alpha << 24) if alpha else 0


def mixed(before, after, weight):
    if weight == 0:
        return before
    if weight == 255:
        return after
    a, b = before >> 24, after >> 24
    denominator = a*(255-weight)+b*weight
    alpha = (denominator+127)//255
    if not alpha:
        return 0
    result = alpha << 24
    for shift in [16, 8, 0]:
        numerator = ((before >> shift) & 255)*a*(255-weight)+((after >> shift) & 255)*b*weight
        result |= ((2*numerator+denominator)//(2*denominator)) << shift
    return result


class Frame:
    width, height, x, y, w, h, stride, guard = 17, 11, 3, 2, 7, 6, 10, 8

    def __init__(self):
        self.data = (U32*(self.width*self.height+self.guard*2))(*([0x6a123456]*(self.width*self.height+self.guard*2)))
        self.output = Pixels(pointer_at(self.data, self.guard*4, U32), (self.h-1)*self.stride+self.w, self.w, self.h, self.stride, self.x, self.y)

    def values(self):
        return [self.data[self.guard+y*self.stride+x] for y in range(self.h) for x in range(self.w)]

    def check_guards(self):
        touched = {self.guard+y*self.stride+x for y in range(self.h) for x in range(self.w)}
        assert all(value == 0x6a123456 for i, value in enumerate(self.data) if i not in touched), 'Compositor overwrote guards/padding'


def compositor(path, masked):
    library = C.CDLL(str(path))
    value = entry(library, 'micopi_compositor_v1', CompositorMasks if masked else CompositorModes)
    if masked:
        assert value.base.base.capabilities & 4 and value.render_masked
        assert value.base.modes & ((1 << 26)-1) == (1 << 26)-1
    return library, value


def check_compositor(table, old):
    raw = [(U32*187)(*[(i*0x231f798d+seed) & 0xffffffff for i in range(187)]) for seed in [0x80102030, 0xbce04090]]
    layers = (Layer*2)(Layer(raw[0], 187, 255, 1, 0), Layer(raw[1], 187, 211, 1, 0))
    storage = (U8*(20*11))(*[(i*53+17) % 256 for i in range(20*11)])
    checks = 0
    for mask in [Mask(None, 0, 0, n) for n in [0, 1, 64, 128, 254, 255]]+[Mask(storage, len(storage), 20, 0)]:
        masks = (Mask*1)(mask)
        for opacity in [0, 128, 255]:
            for mix in [False, True]:
                frame = Frame()
                nodes = (Node*5)(Node(0, 0, 0, 0, 0), Node(0, 1, 0, 0, 0), Node(6, 0, 0, 0, 0), Node(8, 0, 1, 2, opacity/255) if mix else Node(7, 1, 2, 0, opacity/255), Node(3, 3, 0, 0, 0))
                assert table.render_masked(nodes, len(nodes), layers, 2, masks, 1, 17, 11, C.byref(frame.output)) == 0
                expected = []
                for y in range(frame.y, frame.y+frame.h):
                    for x in range(frame.x, frame.x+frame.w):
                        amount = mask.constant if not mask.data else storage[y*20+x]
                        weight = (opacity*amount+127)//255
                        a, b = source(raw[0][y*17+x]), source(raw[1][y*17+x], 211)
                        expected.append(mixed(a, b, weight) if mix else source(b, weight))
                assert frame.values() == expected, f'Independent A8 composition reference failed: mix={mix}, opacity={opacity}'
                frame.check_guards()
                checks += 1
    valid_masks = (Mask*1)(Mask(storage, len(storage), 20, 0))
    bad_nodes = [[Node(6, 0, 0, 0, 0)], [Node(0, 0, 0, 0, 0), Node(7, 0, 0, 0, 1)],
                 [Node(6, 0, 0, 0, 0), Node(3, 0, 0, 0, 0)],
                 [Node(0, 0, 0, 0, 0), Node(6, 0, 0, 0, 0), Node(8, 0, 0, 0, .5)],
                 [Node(0, 0, 0, 0, 0), Node(6, 0, 0, 0, 0), Node(7, 0, 1, 0, float('nan'))]]
    for items in bad_nodes:
        nodes, frame = (Node*len(items))(*items), Frame()
        before = bytes(frame.data)
        assert table.render_masked(nodes, len(nodes), layers, 2, valid_masks, 1, 17, 11, C.byref(frame.output)) != 0
        assert bytes(frame.data) == before, 'Bad typed domain wrote output'
        checks += 1
    for mask in [Mask(None, 0, 0, 256), Mask(storage, len(storage), 16, 0), Mask(storage, 1, 20, 0), Mask(storage, len(storage), 20, 1)]:
        frame, nodes = Frame(), (Node*1)(Node(0, 0, 0, 0, 0))
        before = bytes(frame.data)
        assert table.render_masked(nodes, 1, layers, 2, (Mask*1)(mask), 1, 17, 11, C.byref(frame.output)) != 0
        assert bytes(frame.data) == before, 'Bad unused coverage descriptor wrote output'
        checks += 1
    for mode in range(26):
        nodes = (Node*4)(Node(0, 0, 0, 0, 0), Node(0, 1, 0, 0, 0), Node(2, 0, 1, mode, .73), Node(3, 2, 0, 0, 0))
        first, second = Frame(), Frame()
        assert table.base.base.base.render(nodes, 4, layers, 2, 17, 11, C.byref(first.output)) == 0
        assert table.render_masked(nodes, 4, layers, 2, None, 0, 17, 11, C.byref(second.output)) == 0
        assert bytes(first.data) == bytes(second.data), 'Unmasked fallback changed bytes'
        first.check_guards()
        if old:
            baseline = Frame()
            assert old.base.base.render(nodes, 4, layers, 2, 17, 11, C.byref(baseline.output)) == 0
            assert bytes(first.data) == bytes(baseline.data), f'Old compositor mode {mode} byte mismatch'
        checks += 1
    return checks


def artifact(path):
    return dict(path=str(path), bytes=path.stat().st_size, sha256=hashlib.sha256(path.read_bytes()).hexdigest())


def main():
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument('--painting', type=Path, default=ROOT/'plugins/painting/micopi_painting_tools.dll')
    parser.add_argument('--compositor', type=Path, default=ROOT/'plugins/compositing/micopi_compositing.dll')
    parser.add_argument('--old-painting', type=Path)
    parser.add_argument('--old-compositor', type=Path)
    args = parser.parse_args()
    assert C.sizeof(PTR) == 8, 'The native ABI check requires 64-bit Python'
    for kind, size in [(ToolHost, 32), (ToolHostA8, 40), (A8Pixels, 40), (Mask, 24), (Compositor, 16), (CompositorMasks, 40)]:
        assert C.sizeof(kind) == size, f'ctypes layout mismatch: {kind.__name__}'
    paths = {key: value.resolve() for key, value in vars(args).items() if value is not None}
    assert all(path.is_file() for path in paths.values()), 'A requested native DLL does not exist'
    painting = Painting(paths['painting'])
    old_painting = Painting(paths['old_painting'], False) if 'old_painting' in paths else None
    library, native = compositor(paths['compositor'], True)
    old_library, old_native = compositor(paths['old_compositor'], False) if 'old_compositor' in paths else (None, None)
    checks = dict(painting=check_painting(painting, old_painting), composition=check_compositor(native, old_native))
    output = ROOT/'target/test-output/native-mask-smoke'/str(time.time_ns())
    output.mkdir(parents=True)
    report = dict(scope='Native primitives only; no application mask, Session/replay or physical tablet claim',
                  checks=checks, old_painting_a8_gated=old_painting is not None,
                  artifacts={name: artifact(path) for name, path in paths.items()})
    (output/'report.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
    print(f'Actual DLL native-mask conformance passed: {checks["painting"]} painting fixtures, {checks["composition"]} composition fixtures; scoped A8/selection/stride, typed references and legacy parity.')
    print(output)


if __name__ == '__main__':
    main()
