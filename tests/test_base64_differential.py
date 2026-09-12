import base64 as stdlib_base64
import binascii
import random
import sys
from collections.abc import Callable
from functools import partial

import pytest
from base64_compat_harness import (
    Action,
    AltcharsHook,
    BoolHook,
    BufferHook,
    EncodeHook,
    IndexHook,
    Invocation,
    Observation,
    Raised,
    Returned,
    TranslateHook,
    WarningFilter,
    observe,
)

import hashcodecs.base64 as base64

PYTHON_315 = sys.version_info >= (3, 15)
BASE64_ALPHABET = getattr(
    binascii, 'BASE64_ALPHABET', b'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/'
)


class SentinelError(Exception):
    pass


def _decode_into_invocation(
    function: Callable[..., bytes] | Callable[..., int],
    native_into: bool,
) -> Callable[[], Invocation]:
    def factory() -> Invocation:
        sentinel = SentinelError('sentinel')
        output = bytearray(b'.....')
        initial = bytes(output)

        if native_into:

            def call() -> object:
                return function(b'++8=', output, b'-_')
        else:

            def call() -> int:
                decoded = function(b'++8=', b'-_')
                if len(output) < len(decoded):
                    raise ValueError('destination is too small')
                output[: len(decoded)] = decoded
                return len(decoded)

        return Invocation(
            call,
            [],
            sentinel,
            mutables={'output': output},
            outputs={'output': output},
            output_initial={'output': initial},
        )

    return factory


@pytest.mark.skipif(not PYTHON_315, reason='requires Python 3.15 compatibility warnings')
@pytest.mark.parametrize('warning_filter', ['always', 'error'])
def test_decode_into_warning_cleanup_matches_a_cpython_call_then_copy(warning_filter: WarningFilter) -> None:
    expected = observe(_decode_into_invocation(stdlib_base64.b64decode, False), warning_filter)
    actual = observe(_decode_into_invocation(base64.b64decode_into, True), warning_filter)
    assert actual == expected


def _encode_callback_invocation(function: Callable[..., bytes]) -> Callable[[], Invocation]:
    def factory() -> Invocation:
        sentinel = SentinelError('sentinel')
        events: list[str] = []
        source = BufferHook('input', b'abc', events, sentinel)
        alphabet = BufferHook('alphabet', b'Z' * 64, events, sentinel)
        altchars = AltcharsHook(b'-_', events, sentinel, alphabet=alphabet)
        padded = BoolHook('padded', Action('normal'), events, sentinel)
        wrapcol = IndexHook('wrapcol', Action('normal'), events, sentinel)
        return Invocation(
            lambda: function(source, altchars, padded=padded, wrapcol=wrapcol),
            events,
            sentinel,
            mutables={'input': source.owner, 'alphabet': alphabet.owner},
            buffers=(source, alphabet),
        )

    return factory


@pytest.mark.skipif(not PYTHON_315, reason='requires the CPython 3.15 Base64 API')
def test_encode_callback_and_buffer_lifetime_order_matches_cpython() -> None:
    expected = observe(_encode_callback_invocation(stdlib_base64.b64encode))
    actual = observe(_encode_callback_invocation(base64.b64encode))
    assert actual == expected


def test_harness_distinguishes_exception_details_and_sentinel_identity() -> None:
    def factory(error: BaseException) -> Callable[[], Invocation]:
        def make() -> Invocation:
            sentinel = SentinelError('sentinel')
            return Invocation(lambda: (_ for _ in ()).throw(error), [], sentinel)

        return make

    first = observe(factory(ValueError('first', 1)))
    second = observe(factory(ValueError('second', 1)))
    assert first != second
    assert isinstance(first.outcome, Raised)
    assert first.outcome.arguments == ('first', 1)
    assert first.outcome.message == "('first', 1)"
    assert not first.outcome.is_sentinel


def test_harness_preserves_exact_return_type() -> None:
    def observed(value: object) -> Observation:
        return observe(lambda: Invocation(lambda: value, [], SentinelError('sentinel')))

    assert observed(b'abc') != observed(bytearray(b'abc'))
    outcome = observed(b'abc').outcome
    assert outcome == Returned(bytes, b'abc')


@pytest.mark.parametrize('validate', [False, True])
def test_all_byte_values_at_decode_positions_match_cpython(validate: bool) -> None:
    templates = (b'{}AAA', b'A{}AA', b'AA{}A', b'AAA{}', b'AA=={}')
    for template in templates:
        for value in range(256):
            encoded = template.replace(b'{}', bytes([value]))
            assert _decode_observation(base64.b64decode, encoded, validate) == _decode_observation(
                stdlib_base64.b64decode, encoded, validate
            )


def _decode_observation(function: Callable[..., bytes], encoded: bytes, validate: bool) -> Observation:
    return observe(
        lambda: Invocation(
            lambda: function(encoded, validate=validate),
            [],
            SentinelError('sentinel'),
        )
    )


@pytest.mark.skipif(not PYTHON_315, reason='requires the CPython 3.15 Base64 API')
def test_generated_altchar_pairs_match_cpython_on_short_inputs() -> None:
    inputs = (b'', b'AA==', b'+/8=', b'++8=', b'//8=', b'=w==')
    payloads = (b'', b'\x00', b'\xfb', b'\xff', b'\xfb\xff')
    for first in range(256):
        for second in range(256):
            altchars = bytes([first, second])
            for payload in payloads:
                assert base64.b64encode(payload, altchars) == stdlib_base64.b64encode(payload, altchars)
            for encoded in inputs:
                expected = observe(
                    lambda encoded=encoded, altchars=altchars: Invocation(
                        lambda: stdlib_base64.b64decode(encoded, altchars, validate=True),
                        [],
                        SentinelError('sentinel'),
                    )
                )
                actual = observe(
                    lambda encoded=encoded, altchars=altchars: Invocation(
                        lambda: base64.b64decode(encoded, altchars, validate=True),
                        [],
                        SentinelError('sentinel'),
                    )
                )
                assert actual == expected


def _full_alphabet_decode_invocation(function: Callable[..., bytes], alphabet: bytes, encoded: bytes) -> Invocation:
    sentinel = SentinelError('sentinel')
    events: list[str] = []
    altchars = AltcharsHook(b'-_', events, sentinel, alphabet=alphabet)
    return Invocation(
        lambda: function(encoded, altchars, validate=True, ignorechars=b'!'),
        events,
        sentinel,
    )


@pytest.mark.skipif(not PYTHON_315, reason='requires the CPython 3.15 Base64 API')
@pytest.mark.parametrize(
    'alphabet',
    [
        BASE64_ALPHABET,
        BASE64_ALPHABET[::-1],
        b'Z' * 64,
        bytes(range(128, 192)),
        b'=' + BASE64_ALPHABET[1:],
        BASE64_ALPHABET[:-1] + b'=',
    ],
)
def test_generated_full_alphabets_match_cpython(alphabet: bytes) -> None:
    def factory(function: Callable[..., bytes]) -> Invocation:
        sentinel = SentinelError('sentinel')
        events: list[str] = []
        altchars = AltcharsHook(b'-_', events, sentinel, alphabet=alphabet)
        return Invocation(lambda: function(bytes(range(64)), altchars), events, sentinel)

    assert observe(lambda: factory(base64.b64encode)) == observe(lambda: factory(stdlib_base64.b64encode))

    encoded_inputs = (
        b'',
        alphabet[:4],
        alphabet[:2] + b'==',
        alphabet[61:64] + b'=',
        b'====',
        alphabet[:3] + b'!',
    )
    for encoded in encoded_inputs:
        assert observe(partial(_full_alphabet_decode_invocation, base64.b64decode, alphabet, encoded)) == observe(
            partial(_full_alphabet_decode_invocation, stdlib_base64.b64decode, alphabet, encoded)
        )


ENCODE_ACTION_CASES = (
    ('altchars.length', 'normal'),
    ('altchars.length', 'invalid'),
    ('altchars.length', 'raise'),
    ('altchars.length', 'replace'),
    ('altchars.length', 'grow'),
    ('altchars.length', 'shrink'),
    ('altchars.length', 'reenter'),
    ('altchars.radd', 'invalid'),
    ('altchars.radd', 'raise'),
    ('altchars.radd', 'replace'),
    ('altchars.radd', 'grow'),
    ('altchars.radd', 'shrink'),
    ('altchars.radd', 'reenter'),
    ('altchars.repr', 'invalid'),
    ('altchars.repr', 'raise'),
    ('padded', 'invalid'),
    ('padded', 'raise'),
    ('padded', 'replace'),
    ('padded', 'grow'),
    ('padded', 'shrink'),
    ('padded', 'reenter'),
    ('wrapcol', 'invalid'),
    ('wrapcol', 'raise'),
    ('wrapcol', 'replace'),
    ('wrapcol', 'grow'),
    ('wrapcol', 'shrink'),
    ('wrapcol', 'reenter'),
    ('input.acquire', 'invalid'),
    ('input.acquire', 'raise'),
    ('input.acquire', 'replace'),
    ('input.acquire', 'grow'),
    ('input.acquire', 'shrink'),
    ('input.acquire', 'reenter'),
    ('alphabet.acquire', 'invalid'),
    ('alphabet.acquire', 'raise'),
    ('alphabet.acquire', 'replace'),
    ('alphabet.acquire', 'grow'),
    ('alphabet.acquire', 'shrink'),
    ('alphabet.acquire', 'reenter'),
    ('input.release', 'replace'),
    ('input.release', 'grow'),
    ('input.release', 'shrink'),
    ('input.release', 'reenter'),
    ('alphabet.release', 'replace'),
    ('alphabet.release', 'grow'),
    ('alphabet.release', 'shrink'),
    ('alphabet.release', 'reenter'),
)


def _encode_action_invocation(function: Callable[..., bytes], hook: str, action_name: str) -> Callable[[], Invocation]:
    def factory() -> Invocation:
        sentinel = SentinelError('sentinel')
        events: list[str] = []
        side_effect = bytearray(b'side')

        def reentrant() -> bytes:
            return function(b'x')

        action = Action(action_name)
        input_acquire = action if hook == 'input.acquire' else Action('normal')
        input_release = action if hook == 'input.release' else Action('normal')
        source = BufferHook(
            'input',
            b'abc',
            events,
            sentinel,
            acquire=input_acquire,
            release=input_release,
            target=side_effect if hook == 'input.release' else None,
            reentrant=reentrant,
        )
        alphabet_acquire = action if hook == 'alphabet.acquire' else Action('normal')
        alphabet_release = action if hook == 'alphabet.release' else Action('normal')
        alphabet = BufferHook(
            'alphabet',
            b'Z' * 64,
            events,
            sentinel,
            acquire=alphabet_acquire,
            release=alphabet_release,
            target=side_effect if hook == 'alphabet.release' else source.owner,
            reentrant=reentrant,
        )
        length = action if hook == 'altchars.length' else Action('normal')
        radd = action if hook == 'altchars.radd' else Action('normal')
        representation = action if hook == 'altchars.repr' else Action('normal')
        if hook == 'altchars.repr':
            length = Action('normal', 1)
        altchars = AltcharsHook(
            b'-_',
            events,
            sentinel,
            length=length,
            radd=radd,
            representation=representation,
            alphabet=alphabet,
            target=source.owner,
            reentrant=reentrant,
        )
        padded = BoolHook(
            'padded',
            action if hook == 'padded' else Action('normal'),
            events,
            sentinel,
            target=source.owner,
            reentrant=reentrant,
        )
        wrapcol = IndexHook(
            'wrapcol',
            action if hook == 'wrapcol' else Action('normal'),
            events,
            sentinel,
            target=source.owner,
            reentrant=reentrant,
        )
        return Invocation(
            lambda: function(source, altchars, padded=padded, wrapcol=wrapcol),
            events,
            sentinel,
            mutables={'input': source.owner, 'alphabet': alphabet.owner, 'side_effect': side_effect},
            buffers=(source, alphabet),
        )

    return factory


@pytest.mark.skipif(not PYTHON_315, reason='requires the CPython 3.15 Base64 API')
@pytest.mark.parametrize(('hook', 'action_name'), ENCODE_ACTION_CASES)
def test_generated_encode_callback_actions_match_cpython(hook: str, action_name: str) -> None:
    expected = observe(_encode_action_invocation(stdlib_base64.b64encode, hook, action_name))
    actual = observe(_encode_action_invocation(base64.b64encode, hook, action_name))
    assert actual == expected


DECODE_ACTION_CASES = (
    tuple(
        (hook, action)
        for hook in ('input.encode', 'input.translate', 'input.acquire', 'ignorechars.acquire')
        for action in ('normal', 'invalid', 'raise', 'replace', 'grow', 'shrink', 'reenter')
    )
    + tuple(
        (hook, action)
        for hook in ('input.release', 'ignorechars.release')
        for action in ('replace', 'grow', 'shrink', 'reenter')
    )
    + tuple(
        (hook, action)
        for hook in ('validate', 'padded', 'canonical')
        for action in ('invalid', 'raise', 'replace', 'grow', 'shrink', 'reenter')
    )
)


def _decode_action_invocation(function: Callable[..., bytes], hook: str, action_name: str) -> Callable[[], Invocation]:
    def factory() -> Invocation:
        sentinel = SentinelError('sentinel')
        events: list[str] = []
        side_effect = bytearray(b'side')

        def reentrant() -> bytes:
            return function(b'YWJj')

        action = Action(action_name)
        buffers: list[BufferHook] = []
        altchars: bytes | None = None
        kwargs: dict[str, object] = {}
        if hook == 'input.encode':
            source: object = EncodeHook('YWJj', action, events, sentinel, target=side_effect, reentrant=reentrant)
        elif hook == 'input.translate':
            source = TranslateHook(
                b'-_8=',
                action,
                events,
                sentinel,
                translated=b'+/8=',
                target=side_effect,
                reentrant=reentrant,
            )
            altchars = b'-_'
        elif hook in ('input.acquire', 'input.release'):
            source_buffer = BufferHook(
                'input',
                b'YWJj',
                events,
                sentinel,
                acquire=action if hook == 'input.acquire' else Action('normal'),
                release=action if hook == 'input.release' else Action('normal'),
                target=side_effect,
                reentrant=reentrant,
            )
            source = source_buffer
            buffers.append(source_buffer)
        else:
            source = b'Y!WJj' if hook.startswith('ignorechars.') else b'YWJj'

        if hook.startswith('ignorechars.'):
            ignorechars = BufferHook(
                'ignorechars',
                b'!',
                events,
                sentinel,
                acquire=action if hook == 'ignorechars.acquire' else Action('normal'),
                release=action if hook == 'ignorechars.release' else Action('normal'),
                target=side_effect,
                reentrant=reentrant,
            )
            buffers.append(ignorechars)
            kwargs['ignorechars'] = ignorechars
        for option in ('validate', 'padded', 'canonical'):
            if hook == option:
                kwargs[option] = BoolHook(
                    option,
                    action,
                    events,
                    sentinel,
                    target=side_effect,
                    reentrant=reentrant,
                )

        return Invocation(
            lambda: function(source, altchars, **kwargs),
            events,
            sentinel,
            mutables={'side_effect': side_effect},
            buffers=tuple(buffers),
        )

    return factory


@pytest.mark.skipif(not PYTHON_315, reason='requires the CPython 3.15 Base64 API')
@pytest.mark.parametrize(('hook', 'action_name'), DECODE_ACTION_CASES)
def test_generated_decode_callback_actions_match_cpython(hook: str, action_name: str) -> None:
    expected = observe(_decode_action_invocation(stdlib_base64.b64decode, hook, action_name))
    actual = observe(_decode_action_invocation(base64.b64decode, hook, action_name))
    assert actual == expected


def _decode_into_action_invocation(
    function: Callable[..., bytes] | Callable[..., int],
    native_into: bool,
    hook: str,
    action_name: str,
) -> Callable[[], Invocation]:
    def factory() -> Invocation:
        sentinel = SentinelError('sentinel')
        events: list[str] = []
        output = bytearray(b'.....')
        initial = bytes(output)
        action = Action(action_name)

        def reentrant() -> object:
            if native_into:
                return function(b'YWJj', bytearray(3))
            return function(b'YWJj')

        if hook == 'input.release':
            source_buffer = BufferHook(
                'input',
                b'YWJj',
                events,
                sentinel,
                release=action,
                target=output,
                reentrant=reentrant,
            )
            source: object = source_buffer
            buffers = (source_buffer,)
            kwargs: dict[str, object] = {}
        else:
            source = b'YWJj'
            buffers = ()
            kwargs = {'canonical': BoolHook('canonical', action, events, sentinel, target=output, reentrant=reentrant)}

        def call() -> object:
            if native_into:
                return function(source, output, **kwargs)
            decoded = function(source, **kwargs)
            if len(output) < len(decoded):
                raise ValueError(f'destination requires {len(decoded)} bytes but has {len(output)}')
            output[: len(decoded)] = decoded
            return len(decoded)

        return Invocation(
            call,
            events,
            sentinel,
            mutables={'output': output},
            buffers=buffers,  # type: ignore[arg-type]
            outputs={'output': output},
            output_initial={'output': initial},
        )

    return factory


@pytest.mark.skipif(not PYTHON_315, reason='requires the CPython 3.15 Base64 API')
@pytest.mark.parametrize('hook', ['canonical', 'input.release'])
@pytest.mark.parametrize('action_name', ['invalid', 'raise', 'replace', 'grow', 'shrink', 'reenter'])
def test_generated_destination_mutations_match_cpython_call_then_copy(hook: str, action_name: str) -> None:
    expected = observe(_decode_into_action_invocation(stdlib_base64.b64decode, False, hook, action_name))
    actual = observe(_decode_into_action_invocation(base64.b64decode_into, True, hook, action_name))
    assert actual == expected


def _invalid_argument_invocation(function: Callable[..., bytes], scenario: str) -> Callable[[], Invocation]:
    def factory() -> Invocation:
        sentinel = SentinelError('sentinel')
        events: list[str] = []
        if scenario == 'encode_altchars_before_input':
            altchars = AltcharsHook(b'-_', events, sentinel, length=Action('raise'))

            def call() -> bytes:
                return function(object(), altchars)

        elif scenario == 'encode_input_before_padded':
            altchars = AltcharsHook(b'-_', events, sentinel, alphabet=b'Z' * 64)
            padded = BoolHook('padded', Action('raise'), events, sentinel)

            def call() -> bytes:
                return function(object(), altchars, padded=padded)

        elif scenario == 'decode_input_before_altchars':
            source = BufferHook('input', b'YWJj', events, sentinel, acquire=Action('invalid'))
            altchars = AltcharsHook(b'-_', events, sentinel, length=Action('raise'))

            def call() -> bytes:
                return function(source, altchars)

        else:
            altchars = AltcharsHook(
                b'x',
                events,
                sentinel,
                length=Action('normal', 1),
                representation=Action('raise'),
            )
            validate = BoolHook('validate', Action('raise'), events, sentinel)

            def call() -> bytes:
                return function(b'YWJj', altchars, validate=validate)

        return Invocation(call, events, sentinel)

    return factory


@pytest.mark.skipif(not PYTHON_315, reason='requires the CPython 3.15 Base64 API')
@pytest.mark.parametrize(
    ('scenario', 'reference', 'candidate'),
    [
        ('encode_altchars_before_input', stdlib_base64.b64encode, base64.b64encode),
        ('encode_input_before_padded', stdlib_base64.b64encode, base64.b64encode),
        ('decode_input_before_altchars', stdlib_base64.b64decode, base64.b64decode),
        ('decode_altchars_before_validate', stdlib_base64.b64decode, base64.b64decode),
    ],
)
def test_two_invalid_arguments_preserve_cpython_error_precedence(
    scenario: str, reference: Callable[..., bytes], candidate: Callable[..., bytes]
) -> None:
    assert observe(_invalid_argument_invocation(candidate, scenario)) == observe(
        _invalid_argument_invocation(reference, scenario)
    )


def _configured_decode_observation(
    function: Callable[..., bytes],
    encoded: object,
    altchars: bytes | None = None,
    **kwargs: object,
) -> Observation:
    return observe(
        lambda: Invocation(
            lambda: function(encoded, altchars, **kwargs),
            [],
            SentinelError('sentinel'),
        )
    )


@pytest.mark.skipif(not PYTHON_315, reason='requires the CPython 3.15 Base64 API')
@pytest.mark.parametrize(
    'encoded',
    [
        b'',
        b'=',
        b'==',
        b'====',
        b'=AAA',
        b'A=AA',
        b'AA=A',
        b'AAA=',
        b'AA==',
        b'AA===',
        b'AA====',
        b'AA==A',
        b'AA==AAAA',
        b'AAAA=',
        b'AAAA==',
        b'AAAA===',
        b'YWJj=',
        b'YWJj==',
        b'Y=WJj',
        b'YW=Jj',
        b'====YWJj',
        b'YWJj====',
    ],
)
def test_generated_padding_placements_match_cpython(encoded: bytes) -> None:
    for validate in (False, True):
        for padded in (False, True):
            for canonical in (False, True):
                kwargs = {'validate': validate, 'padded': padded, 'canonical': canonical}
                assert _configured_decode_observation(
                    base64.b64decode, encoded, **kwargs
                ) == _configured_decode_observation(stdlib_base64.b64decode, encoded, **kwargs)


@pytest.mark.skipif(not PYTHON_315, reason='requires the CPython 3.15 Base64 API')
@pytest.mark.parametrize(
    ('encoded', 'altchars', 'ignorechars'),
    [
        (b'AYAA', None, b'A'),
        (b'YWJj', None, b'Y'),
        (b'+/8=', None, b'+'),
        (b'+/8=', None, b'/'),
        (b'AA==', None, b'='),
        (b'A=A=', None, b'A='),
        (b'-_8=', b'-_', b'-'),
        (b'-_8=', b'-_', b'_'),
        (b'-_8=', b'-_', b'-_='),
        (b'@@8=', b'@#', b'@'),
        (b'@#8=', b'@#', b'#='),
    ],
)
def test_ignored_bytes_overlapping_alphabet_and_padding_match_cpython(
    encoded: bytes,
    altchars: bytes | None,
    ignorechars: bytes,
) -> None:
    for validate in (False, True):
        kwargs = {'validate': validate, 'ignorechars': ignorechars}
        assert _configured_decode_observation(
            base64.b64decode, encoded, altchars, **kwargs
        ) == _configured_decode_observation(stdlib_base64.b64decode, encoded, altchars, **kwargs)


@pytest.mark.skipif(not PYTHON_315, reason='requires the CPython 3.15 Base64 API')
def test_exhaustive_canonical_trailing_bits_match_cpython() -> None:
    for symbol in BASE64_ALPHABET:
        for encoded in (
            b'Q' + bytes([symbol]),
            b'Q' + bytes([symbol]) + b'==',
            b'QU' + bytes([symbol]),
            b'QU' + bytes([symbol]) + b'=',
        ):
            for validate in (False, True):
                for padded in (False, True):
                    for canonical in (False, True):
                        kwargs = {'validate': validate, 'padded': padded, 'canonical': canonical}
                        assert _configured_decode_observation(
                            base64.b64decode, encoded, **kwargs
                        ) == _configured_decode_observation(stdlib_base64.b64decode, encoded, **kwargs)


def _released_memoryview(value: bytes) -> memoryview:
    view = memoryview(value)
    view.release()
    return view


@pytest.mark.parametrize(
    ('kind', 'encode_input', 'decode_input'),
    [
        ('contiguous', lambda: memoryview(b'abc'), lambda: memoryview(b'YWJj')),
        ('sliced', lambda: memoryview(b'_abc_')[1:-1], lambda: memoryview(b'_YWJj_')[1:-1]),
        ('strided', lambda: memoryview(b'a.b.c')[::2], lambda: memoryview(b'YxWxJxjx')[::2]),
        ('released', lambda: _released_memoryview(b'abc'), lambda: _released_memoryview(b'YWJj')),
    ],
)
def test_memoryview_shapes_match_cpython(
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


def _overlapping_into_invocation(direction: str, native_into: bool) -> Callable[[], Invocation]:
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
def test_overlapping_memoryviews_match_cpython_call_then_copy(direction: str) -> None:
    assert observe(_overlapping_into_invocation(direction, True)) == observe(
        _overlapping_into_invocation(direction, False)
    )


FAST_PATH_LENGTHS = (
    0,
    1,
    2,
    3,
    15,
    16,
    17,
    31,
    32,
    33,
    47,
    48,
    49,
    63,
    64,
    65,
    95,
    96,
    97,
    127,
    128,
    129,
)


@pytest.mark.parametrize('length', FAST_PATH_LENGTHS)
def test_simd_block_and_tail_boundaries_match_cpython(length: int) -> None:
    payload = bytes((index * 37 + 11) & 0xFF for index in range(length))
    encoded = stdlib_base64.b64encode(payload)
    assert base64.b64encode(payload) == encoded
    assert base64.b64decode(encoded, validate=True) == stdlib_base64.b64decode(encoded, validate=True)

    encode_output = bytearray([0xA5] * (len(encoded) + 7))
    assert base64.b64encode_into(payload, encode_output) == len(encoded)
    assert encode_output[: len(encoded)] == encoded
    assert encode_output[len(encoded) :] == bytes([0xA5] * 7)

    decode_output = bytearray([0xA5] * (length + 7))
    assert base64.b64decode_into(encoded, decode_output, validate=True) == length
    assert decode_output[:length] == payload
    assert decode_output[length:] == bytes([0xA5] * 7)


@pytest.mark.skipif(not PYTHON_315, reason='requires the CPython 3.15 Base64 API')
@pytest.mark.parametrize('encoded_length', [4092, 4095, 4096, 4097, 4100, 8191, 8192, 8193])
def test_configured_staging_boundaries_match_cpython(encoded_length: int) -> None:
    symbols = (b'QUJD' * ((encoded_length + 3) // 4))[:encoded_length]
    insertion = min(encoded_length, 4096)
    encoded = symbols[:insertion] + b'!' + symbols[insertion:]
    kwargs = {'validate': True, 'ignorechars': b'!'}
    assert _configured_decode_observation(base64.b64decode, encoded, **kwargs) == _configured_decode_observation(
        stdlib_base64.b64decode, encoded, **kwargs
    )


@pytest.mark.parametrize('length', [256 * 1024 - 1, 256 * 1024, 256 * 1024 + 1])
def test_gil_release_boundaries_match_cpython(length: int) -> None:
    payload = bytes((index * 17 + 3) & 0xFF for index in range(length))
    expected = stdlib_base64.b64encode(payload)
    assert base64.b64encode(payload) == expected
    assert base64.b64decode(expected, validate=True) == payload


@pytest.mark.skipif(not PYTHON_315, reason='requires the CPython 3.15 Base64 API')
def test_targeted_deterministic_decode_fuzz_matches_cpython() -> None:
    randomizer = random.Random(0xB64C0DEC)
    altchar_choices = (None, b'-_', b'@#', b'++', b'=_', bytes([0x80, 0xFF]))
    ignorechar_choices = (None, b'', b'!', b' \n', b'=', bytes([0x80, 0xFF]))

    for _ in range(512):
        payload = randomizer.randbytes(randomizer.randrange(65))
        encoded = bytearray(stdlib_base64.b64encode(payload))
        if encoded and randomizer.randrange(2):
            del encoded[-randomizer.randrange(1, min(3, len(encoded)) + 1) :]
        for _ in range(randomizer.randrange(4)):
            encoded.insert(randomizer.randrange(len(encoded) + 1), randomizer.randrange(256))
        encoded.extend(b'=' * randomizer.randrange(4))

        altchars = randomizer.choice(altchar_choices)
        ignorechars = randomizer.choice(ignorechar_choices)
        kwargs: dict[str, object] = {
            'validate': bool(randomizer.randrange(2)),
            'padded': bool(randomizer.randrange(2)),
            'canonical': bool(randomizer.randrange(2)),
        }
        if ignorechars is not None:
            kwargs['ignorechars'] = ignorechars
        value = bytes(encoded)
        assert _configured_decode_observation(
            base64.b64decode, value, altchars, **kwargs
        ) == _configured_decode_observation(stdlib_base64.b64decode, value, altchars, **kwargs)


def _batch_decode_invocation(mode: str, warning_case: bool = False) -> Callable[[], Invocation]:
    def factory() -> Invocation:
        sentinel = SentinelError('sentinel')
        values = [b'YWJj', b'++8=' if warning_case else b'YWJjY===', b'ZGVm']
        outputs = [bytearray(b'......') for _ in values]
        initial = {f'output{index}': bytes(output) for index, output in enumerate(outputs)}

        def call() -> list[int]:
            if mode == 'batch':
                return base64.b64decode_batch_into(
                    values,
                    outputs,
                    b'-_' if warning_case else None,
                    validate=not warning_case,
                )

            written = []
            for value, output in zip(values, outputs, strict=True):
                if mode == 'cpython':
                    decoded = stdlib_base64.b64decode(
                        value,
                        b'-_' if warning_case else None,
                        validate=not warning_case,
                    )
                    output[: len(decoded)] = decoded
                    written.append(len(decoded))
                else:
                    written.append(base64.b64decode_into(value, output, validate=True))
            return written

        output_map = {f'output{index}': output for index, output in enumerate(outputs)}
        return Invocation(
            call,
            [],
            sentinel,
            mutables=output_map,
            outputs=output_map,
            output_initial=initial,
        )

    return factory


@pytest.mark.skipif(not PYTHON_315, reason='requires Python 3.15 compatibility warnings')
@pytest.mark.parametrize('warning_filter', ['always', 'error'])
def test_batch_warning_behavior_matches_sequential_cpython(
    warning_filter: WarningFilter,
) -> None:
    actual = observe(_batch_decode_invocation('batch', warning_case=True), warning_filter)
    assert actual == observe(_batch_decode_invocation('cpython', warning_case=True), warning_filter)
    if warning_filter == 'error':
        assert isinstance(actual.outcome, Raised)
        assert actual.outcome.type is FutureWarning
        assert [state.contents for state in actual.outputs] == [b'abc...', b'......', b'......']
    else:
        assert actual.outcome == Returned(list, [3, 2, 3])
        assert len(actual.warnings) == 1
        assert [state.contents for state in actual.outputs] == [b'abc...', b'\xfb\xef....', b'def...']


def test_batch_into_matches_repeated_into_failure_state() -> None:
    actual = observe(_batch_decode_invocation('batch'))
    assert actual == observe(_batch_decode_invocation('into'))
    assert isinstance(actual.outcome, Raised)
    assert actual.outcome.type is binascii.Error
    assert [state.contents for state in actual.outputs] == [b'abc...', b'abc...', b'......']


@pytest.mark.skipif(not PYTHON_315, reason='requires Python-level buffer protocol support')
def test_allocating_batch_is_fail_fast_in_callback_order() -> None:
    def invocation(batch: bool) -> Callable[[], Invocation]:
        def factory() -> Invocation:
            sentinel = SentinelError('sentinel')
            events: list[str] = []
            inputs = (
                BufferHook('input0', b'YWJj', events, sentinel),
                BufferHook('input1', b'YWJ!', events, sentinel),
                BufferHook('input2', b'ZGVm', events, sentinel),
            )

            def call() -> list[bytes]:
                if batch:
                    return base64.b64decode_batch(list(inputs), validate=True)
                return [stdlib_base64.b64decode(value, validate=True) for value in inputs]

            return Invocation(
                call,
                events,
                sentinel,
                mutables={f'input{index}': value.owner for index, value in enumerate(inputs)},
                buffers=inputs,
            )

        return factory

    actual = observe(invocation(True))
    assert actual == observe(invocation(False))
    assert actual.callbacks == (
        'input0.__buffer__',
        'input0.__release_buffer__',
        'input1.__buffer__',
        'input1.__release_buffer__',
    )
