"""Independent PNG checks shared by small, trusted smoke-test fixtures.

This is a test reference for RGBA8 non-interlaced output, not an application codec.
"""
import struct
import zlib

SIGNATURE = b'\x89PNG\r\n\x1a\n'


def chunk(kind, payload):
    return struct.pack('>I', len(payload)) + kind + payload + struct.pack('>I', zlib.crc32(kind + payload))


def chunks(data):
    assert data.startswith(SIGNATURE), 'Missing PNG signature'
    at, result = 8, []
    while at < len(data):
        assert at + 12 <= len(data), 'Truncated PNG chunk'
        length, = struct.unpack_from('>I', data, at)
        kind = data[at + 4:at + 8]
        end = at + 8 + length
        assert end + 4 <= len(data), 'Truncated PNG payload'
        payload = data[at + 8:end]
        crc, = struct.unpack_from('>I', data, end)
        assert zlib.crc32(kind + payload) == crc, f'Bad {kind!r} CRC'
        result.append((kind, payload))
        at = end + 4
        if kind == b'IEND':
            assert not payload and at == len(data), 'Data after PNG end'
            break
    assert result and result[0][0] == b'IHDR' and result[-1][0] == b'IEND'
    return result


def density(data):
    values = [struct.unpack('>IIB', payload) for kind, payload in chunks(data) if kind == b'pHYs']
    assert len(values) <= 1, 'Duplicate output pHYs'
    return values[0] if values else None


def _paeth(left, up, upper_left):
    p = left + up - upper_left
    a, b, c = abs(p - left), abs(p - up), abs(p - upper_left)
    return left if a <= b and a <= c else up if b <= c else upper_left


def pixels(data):
    items = chunks(data)
    width, height, bits, color, compression, filtering, interlace = struct.unpack('>IIBBBBB', items[0][1])
    assert (bits, color, compression, filtering, interlace) == (8, 6, 0, 0, 0), 'Expected RGBA8 smoke output'
    raw = zlib.decompress(b''.join(payload for kind, payload in items if kind == b'IDAT'))
    stride = width * 4
    assert len(raw) == height * (stride + 1), 'Unexpected decoded length'
    previous = bytearray(stride)
    result = bytearray()
    for row in range(height):
        at = row * (stride + 1)
        mode = raw[at]
        assert 0 <= mode <= 4
        current = bytearray(raw[at + 1:at + stride + 1])
        for x in range(stride):
            left = current[x - 4] if x >= 4 else 0
            up = previous[x]
            upper_left = previous[x - 4] if x >= 4 else 0
            prediction = [0, left, up, (left + up) // 2, _paeth(left, up, upper_left)][mode]
            current[x] = (current[x] + prediction) & 255
        result.extend(current)
        previous = current
    return width, height, bytes(result)


def rgba_fixture(width, height, rgba, pixel_density=None):
    assert len(rgba) == width * height * 4
    rows = b''.join(b'\0' + rgba[y * width * 4:(y + 1) * width * 4] for y in range(height))
    data = SIGNATURE + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 6, 0, 0, 0))
    if pixel_density is not None:
        data += chunk(b'pHYs', struct.pack('>IIB', *pixel_density))
    return data + chunk(b'IDAT', zlib.compress(rows)) + chunk(b'IEND', b'')
