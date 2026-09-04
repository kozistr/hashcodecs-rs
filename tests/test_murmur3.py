import ast
import inspect
import sys
from array import array
from collections.abc import Callable
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from threading import Barrier
from typing import Any

import pytest

import hashcodecs
import hashcodecs.murmur3 as murmur3

FREE_THREADED = not getattr(sys, '_is_gil_enabled', lambda: True)()
GILProgressAssertion = Callable[[Callable[[], object], object, int], None]


def test_murmur3_functions_keep_public_module_metadata() -> None:
    for name in murmur3.__all__:
        assert getattr(murmur3, name).__module__ == 'hashcodecs.murmur3'


def test_murmur3_classes_keep_generated_docstrings() -> None:
    stub = Path(hashcodecs.__file__).with_name('_hashcodecs.pyi')
    declarations = ast.parse(stub.read_text(encoding='utf-8'), filename=str(stub))
    expected = {
        node.name: ast.get_docstring(node, clean=True)
        for node in declarations.body
        if isinstance(node, ast.ClassDef) and node.name.startswith('murmur3_')
    }

    assert {name: getattr(murmur3, name).__doc__ for name in expected} == expected


def test_murmur3_known_answers_and_buffer_inputs() -> None:
    assert hashcodecs.murmur3_32(bytearray()) == 0
    assert hashcodecs.murmur3_32(b'hello') == 0x248BFA47
    assert murmur3.murmur3_32(b'hello') == 0x248BFA47
    assert hashcodecs.murmur3_32(bytearray(b'hello')) == 0x248BFA47
    assert hashcodecs.murmur3_32(memoryview(b'hello')) == 0x248BFA47
    assert hashcodecs.murmur3_32(array('B', b'hello')) == 0x248BFA47
    assert hashcodecs.murmur3_x86_128_digest(bytes([1, 2, 3])) == bytes.fromhex('e16401f6334213b5334213b5334213b5')
    assert hashcodecs.murmur3_x64_128_digest(bytes([1, 2, 3])) == bytes.fromhex('a937130eef3e641a659a233c404a4e49')

    noncontiguous = memoryview(b'h.e.l.l.o.')[::2]
    assert hashcodecs.murmur3_32(noncontiguous) == hashcodecs.murmur3_32(b'hello')
    assert hashcodecs.murmur3_x86_128_digest(noncontiguous) == hashcodecs.murmur3_x86_128_digest(b'hello')
    assert hashcodecs.murmur3_x64_128_digest(noncontiguous) == hashcodecs.murmur3_x64_128_digest(b'hello')
    for constructor in (murmur3.murmur3_x86_32, murmur3.murmur3_x86_128, murmur3.murmur3_x64_128):
        assert constructor(noncontiguous).digest() == constructor(b'hello').digest()


@pytest.mark.parametrize(
    'function',
    [hashcodecs.murmur3_32, hashcodecs.murmur3_x86_128_digest, hashcodecs.murmur3_x64_128_digest],
)
def test_murmur3_one_shot_argument_compatibility(function: Callable[..., object]) -> None:
    assert str(inspect.signature(function)) == '(s, seed=0)'
    expected = function(b'hello', 42)
    assert function(s=b'hello', seed=42) == expected
    assert function(bytearray(b'hello'), seed=42) == expected
    assert function(memoryview(b'hello'), 42) == expected

    with pytest.raises(TypeError):
        function()
    with pytest.raises(TypeError):
        function(b'hello', s=b'hello')
    with pytest.raises(TypeError):
        function(b'hello', 42, seed=42)
    with pytest.raises(TypeError):
        function(b'hello', unknown=42)
    with pytest.raises(OverflowError):
        function(b'hello', -1)
    with pytest.raises(OverflowError):
        function(b'hello', 1 << 32)


@pytest.mark.parametrize(
    ('constructor', 'one_shot', 'name', 'digest_size', 'block_size'),
    [
        (
            murmur3.murmur3_x86_32,
            lambda data, seed=0: murmur3.murmur3_32(data, seed).to_bytes(4, 'little'),
            'murmur3_x86_32',
            4,
            4,
        ),
        (murmur3.murmur3_x86_128, murmur3.murmur3_x86_128_digest, 'murmur3_x86_128', 16, 16),
        (murmur3.murmur3_x64_128, murmur3.murmur3_x64_128_digest, 'murmur3_x64_128', 16, 16),
    ],
)
def test_hashlib_style_murmur3_api(
    constructor: Callable[..., Any],
    one_shot: Callable[..., bytes],
    name: str,
    digest_size: int,
    block_size: int,
) -> None:
    hasher = constructor(memoryview(b'prefix'), 42)
    assert hasher.update(bytearray(b'-suffix')) is None
    expected = one_shot(b'prefix-suffix', 42)
    assert hasher.digest() == expected
    assert hasher.hexdigest() == expected.hex()
    assert hasher.digest() == expected
    assert hasher.name == name
    assert hasher.digest_size == digest_size
    assert hasher.block_size == block_size

    snapshot = constructor(b'prefix', seed=42).copy()
    snapshot.update(b'-suffix')
    assert snapshot.digest() == expected
    assert constructor().digest() == one_shot(b'')


def test_incremental_murmur3_matches_one_shot_at_every_boundary() -> None:
    constructors = (
        (
            hashcodecs.murmur3_x86_32,
            lambda data, seed: hashcodecs.murmur3_32(data, seed).to_bytes(4, 'little'),
        ),
        (hashcodecs.murmur3_x86_128, hashcodecs.murmur3_x86_128_digest),
        (hashcodecs.murmur3_x64_128, hashcodecs.murmur3_x64_128_digest),
    )
    for length in range(65):
        payload = bytes((index * 43 + 5) & 0xFF for index in range(length))
        for constructor, one_shot in constructors:
            actual = constructor(seed=0xFEEDBEEF)
            for offset in range(0, length, 3):
                actual.update(payload[offset : offset + 3])
            assert actual.digest() == one_shot(payload, 0xFEEDBEEF)


@pytest.mark.parametrize('constructor', [murmur3.murmur3_x86_32, murmur3.murmur3_x86_128, murmur3.murmur3_x64_128])
def test_hashlib_style_murmur3_rejects_invalid_inputs(constructor: Callable[..., Any]) -> None:
    with pytest.raises(TypeError):
        constructor([1, 2, 3])
    with pytest.raises(OverflowError):
        constructor(seed=-1)
    with pytest.raises(TypeError):
        constructor().update([1, 2, 3])


@pytest.mark.skipif(FREE_THREADED, reason='requires a GIL-enabled CPython build')
def test_large_murmur3_calls_release_the_gil(assert_releases_gil: GILProgressAssertion) -> None:
    payload = bytes(range(256)) * 257
    for function in (
        hashcodecs.murmur3_32,
        hashcodecs.murmur3_x86_128_digest,
        hashcodecs.murmur3_x64_128_digest,
    ):
        expected = function(payload, 42)
        assert_releases_gil(lambda function=function: function(payload, 42), expected, 256)

    for constructor in (murmur3.murmur3_x86_32, murmur3.murmur3_x86_128, murmur3.murmur3_x64_128):
        expected = constructor(payload, 42).digest()

        def incremental_digest(constructor: Callable[..., Any] = constructor) -> bytes:
            hasher = constructor(seed=42)
            hasher.update(payload)
            return hasher.digest()

        assert_releases_gil(incremental_digest, expected, 256)


@pytest.mark.skipif(not FREE_THREADED, reason='requires a free-threaded CPython build')
@pytest.mark.parametrize('use_memoryview', [False, True], ids=['bytearray', 'memoryview'])
@pytest.mark.parametrize(
    ('one_shot', 'constructor'),
    [
        (hashcodecs.murmur3_32, murmur3.murmur3_x86_32),
        (hashcodecs.murmur3_x86_128_digest, murmur3.murmur3_x86_128),
        (hashcodecs.murmur3_x64_128_digest, murmur3.murmur3_x64_128),
    ],
)
def test_murmur3_mutable_input_races_are_serialized(
    one_shot: Callable[[object], object],
    constructor: Callable[..., Any],
    use_memoryview: bool,
) -> None:
    size = 1024 * 1024
    first = b'a' * size
    second = b'b' * size
    value = bytearray(first)
    input_value = memoryview(value) if use_memoryview else value
    expected = {one_shot(first), one_shot(second)}
    start = Barrier(2)

    def hash_value() -> list[object]:
        start.wait()
        results = []
        for _ in range(32):
            results.append(one_shot(input_value))
            hasher = constructor()
            hasher.update(input_value)
            results.append(
                hasher.digest() if not isinstance(results[-1], int) else int.from_bytes(hasher.digest(), 'little')
            )
        return results

    def mutate_value() -> None:
        start.wait()
        for index in range(64):
            value[:] = first if index % 2 else second

    with ThreadPoolExecutor(max_workers=2) as executor:
        hashes_future = executor.submit(hash_value)
        mutate_future = executor.submit(mutate_value)
        hashes = hashes_future.result()
        mutate_future.result()

    assert set(hashes) <= expected
