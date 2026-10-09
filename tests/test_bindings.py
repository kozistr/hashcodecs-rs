from collections.abc import Callable

import pytest

import hashcodecs


class Keyword(str):
    __hash__ = str.__hash__

    def __eq__(self, other: object) -> bool:
        raise AssertionError('binding keyword matching must not call __eq__')

    def __len__(self) -> int:
        raise AssertionError('binding keyword matching must not call __len__')


@pytest.mark.parametrize(
    ('function', 'value', 'options'),
    [
        (hashcodecs.standard_b64encode, b'hello', {}),
        (hashcodecs.b64encode, b'hello', {'altchars': b'-_', 'padded': False}),
        (hashcodecs.urlsafe_b64encode, b'hello', {'padded': False}),
        (hashcodecs.standard_b64decode, b'aGVsbG8=', {}),
        (hashcodecs.b64decode, b'aGVsbG8=', {'validate': True, 'canonical': True}),
        (hashcodecs.urlsafe_b64decode, b'aGVsbG8=', {}),
        (hashcodecs.murmur3_32, b'hello', {'seed': 42}),
        (hashcodecs.murmur3_x86_128_digest, b'hello', {'seed': 42}),
        (hashcodecs.murmur3_x64_128_digest, b'hello', {'seed': 42}),
        (hashcodecs.xxh3_64, b'hello', {'seed': 42}),
        (hashcodecs.xxh3_128, b'hello', {'seed': 42}),
    ],
)
@pytest.mark.parametrize('keyword_type', [str, Keyword])
def test_binding_keyword_strings(
    function: Callable[..., object], value: bytes, options: dict[str, object], keyword_type: type[str]
) -> None:
    expected = function(value, **options)
    # Build fresh names as well as str subclasses; neither requires interning.
    keywords = {keyword_type((name + '!')[:-1]): option for name, option in {'s': value, **options}.items()}
    assert function(**keywords) == expected
    assert function(**dict(reversed(keywords.items()))) == expected

    with pytest.raises(TypeError, match='multiple values'):
        function(value, **keywords)


@pytest.mark.parametrize('function', [hashcodecs.b64encode, hashcodecs.murmur3_32, hashcodecs.xxh3_64])
@pytest.mark.parametrize('name', ['', 'S', 's\x00', 'seéd', 'seed\x00', '\uff53', '😀', '\ud800'])
@pytest.mark.parametrize('keyword_type', [str, Keyword])
def test_binding_rejects_unknown_keyword_strings(
    function: Callable[..., object], name: str, keyword_type: type[str]
) -> None:
    with pytest.raises(TypeError, match='unexpected keyword argument'):
        function(b'hello', **{keyword_type(name): 0})


@pytest.mark.parametrize(
    'function', [hashcodecs.murmur3_32, hashcodecs.murmur3_x86_128_digest, hashcodecs.murmur3_x64_128_digest]
)
def test_murmur3_seed_conversion_boundaries(function: Callable[..., object]) -> None:
    class IntSeed(int):
        def __index__(self) -> int:
            raise AssertionError('integer seed conversion must not call __index__')

    class IndexSeed:
        def __index__(self) -> int:
            return 42

    for seed in (0, 1, (1 << 30) - 1, 1 << 30, (1 << 32) - 1):
        expected = function(b'hello', seed)
        assert function(s=b'hello', seed=IntSeed(seed)) == expected

    assert function(b'hello', False) == function(b'hello', 0)
    assert function(b'hello', True) == function(b'hello', 1)

    for seed in (-1, 1 << 32, (1 << 64) - 1, 1 << 64):
        with pytest.raises(OverflowError):
            function(b'hello', seed=seed)

    for invalid in (None, 1.0, IndexSeed()):
        with pytest.raises(TypeError):
            function(b'hello', seed=invalid)
