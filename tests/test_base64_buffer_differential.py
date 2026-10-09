import base64 as stdlib_base64
import sys
from collections.abc import Callable

import pytest
from base64_compat_harness import (
    Invocation,
    SentinelError,
    observe,
)

import hashcodecs.base64 as base64

stdlib_b64encode: Callable[..., bytes] = stdlib_base64.b64encode

stdlib_b64decode: Callable[..., bytes] = stdlib_base64.b64decode

PYTHON_315 = sys.version_info >= (3, 15)


def _released_view(value: bytes) -> memoryview:
    view = memoryview(value)
    view.release()
    return view


@pytest.mark.parametrize(
    ('kind', 'encode_input', 'decode_input'),
    [
        ('contiguous', lambda: memoryview(b'abc'), lambda: memoryview(b'YWJj')),
        ('sliced', lambda: memoryview(b'_abc_')[1:-1], lambda: memoryview(b'_YWJj_')[1:-1]),
        ('strided', lambda: memoryview(b'a.b.c')[::2], lambda: memoryview(b'YxWxJxjx')[::2]),
        ('released', lambda: _released_view(b'abc'), lambda: _released_view(b'YWJj')),
    ],
)
def test_memoryview_inputs(
    kind: str,
    encode_input: Callable[[], memoryview],
    decode_input: Callable[[], memoryview],
) -> None:
    del kind
    assert observe(
        lambda: Invocation(lambda: base64.b64encode(encode_input()), [], SentinelError('sentinel'))
    ) == observe(lambda: Invocation(lambda: stdlib_base64.b64encode(encode_input()), [], SentinelError('sentinel')))
    assert observe(
        lambda: Invocation(lambda: base64.b64decode(decode_input(), validate=True), [], SentinelError('sentinel'))
    ) == observe(
        lambda: Invocation(
            lambda: stdlib_base64.b64decode(decode_input(), validate=True),
            [],
            SentinelError('sentinel'),
        )
    )


def _overlap_case(direction: str, native_into: bool) -> Callable[[], Invocation]:
    def factory() -> Invocation:
        sentinel = SentinelError('sentinel')
        if direction == 'encode':
            output = bytearray(b'abc.....')
            source = memoryview(output)[:3]
            allocating = stdlib_base64.b64encode
            into = base64.b64encode_into
        else:
            output = bytearray(b'YWJj....')
            source = memoryview(output)[:4]
            allocating = stdlib_base64.b64decode
            into = base64.b64decode_into
        initial = bytes(output)

        def call() -> int:
            if native_into:
                return into(source, output)
            result = allocating(source)
            output[: len(result)] = result
            return len(result)

        return Invocation(
            call,
            [],
            sentinel,
            mutables={'output': output},
            outputs={'output': output},
            output_initial={'output': initial},
        )

    return factory


@pytest.mark.parametrize('direction', ['encode', 'decode'])
def test_into_overlap(direction: str) -> None:
    assert observe(_overlap_case(direction, True)) == observe(_overlap_case(direction, False))


def _capacity_case(direction: str, native_into: bool, output_size: int) -> Callable[[], Invocation]:
    def factory() -> Invocation:
        sentinel = SentinelError('sentinel')
        output = bytearray([0xA5] * output_size)
        initial = bytes(output)

        def call() -> int:
            if direction == 'encode':
                if native_into:
                    return base64.b64encode_into(b'\xfb\xff', output, b'-_', padded=False)
                result = stdlib_b64encode(b'\xfb\xff', b'-_', padded=False)
            else:
                if native_into:
                    return base64.b64decode_into(b'-_8', output, b'-_', validate=True, padded=False)
                result = stdlib_b64decode(b'-_8', b'-_', validate=True, padded=False)
            if len(output) < len(result):
                raise ValueError(f'Base64 output requires {len(result)} bytes but the destination has {len(output)}')
            output[: len(result)] = result
            return len(result)

        return Invocation(
            call,
            [],
            sentinel,
            mutables={'output': output},
            outputs={'output': output},
            output_initial={'output': initial},
        )

    return factory


@pytest.mark.skipif(not PYTHON_315, reason='requires the CPython 3.15 Base64 API')
@pytest.mark.parametrize(('direction', 'required'), [('encode', 3), ('decode', 2)])
@pytest.mark.parametrize('difference', [-2, -1, 0, 3])
def test_into_capacity(
    direction: str,
    required: int,
    difference: int,
) -> None:
    output_size = max(0, required + difference)
    assert observe(_capacity_case(direction, True, output_size)) == observe(
        _capacity_case(direction, False, output_size)
    )
