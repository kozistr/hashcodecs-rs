import ast
import inspect
from array import array
from collections.abc import Callable
from pathlib import Path

import pytest

import hashcodecs
import hashcodecs.murmur3 as murmur3


def test_functions_keep_public_module_metadata() -> None:
    for name in murmur3.__all__:
        assert getattr(murmur3, name).__module__ == 'hashcodecs.murmur3'


def test_classes_match_typed_documentation() -> None:
    stub = Path(hashcodecs.__file__).with_name('_hashcodecs.pyi')
    declarations = ast.parse(stub.read_text(encoding='utf-8'), filename=str(stub))
    expected = {
        node.name: ast.get_docstring(node, clean=True)
        for node in declarations.body
        if isinstance(node, ast.ClassDef) and node.name.startswith('murmur3_')
    }

    assert {name: getattr(murmur3, name).__doc__ for name in expected} == expected


def test_known_digests_and_buffer_inputs() -> None:
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
def test_one_shot_arguments_follow_public_signature(function: Callable[..., object]) -> None:
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
