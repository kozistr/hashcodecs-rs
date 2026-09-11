import base64 as stdlib_base64
import binascii
import sys
from collections.abc import Callable

import pytest
from base64_compat_harness import (
    Action,
    AltcharsHook,
    BoolHook,
    BufferHook,
    IndexHook,
    Invocation,
    Observation,
    Raised,
    Returned,
    WarningFilter,
    observe,
)

import hashcodecs.base64 as base64

PYTHON_315 = sys.version_info >= (3, 15)


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
    for first in range(256):
        for second in range(256):
            altchars = bytes([first, second])
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


@pytest.mark.skipif(not PYTHON_315, reason='requires the CPython 3.15 Base64 API')
@pytest.mark.parametrize(
    'alphabet',
    [
        binascii.BASE64_ALPHABET,
        binascii.BASE64_ALPHABET[::-1],
        b'Z' * 64,
        bytes(range(128, 192)),
        b'=' + binascii.BASE64_ALPHABET[1:],
        binascii.BASE64_ALPHABET[:-1] + b'=',
    ],
)
def test_generated_full_alphabets_match_cpython(alphabet: bytes) -> None:
    def factory(function: Callable[..., bytes]) -> Invocation:
        sentinel = SentinelError('sentinel')
        events: list[str] = []
        altchars = AltcharsHook(b'-_', events, sentinel, alphabet=alphabet)
        return Invocation(lambda: function(bytes(range(64)), altchars), events, sentinel)

    assert observe(lambda: factory(base64.b64encode)) == observe(lambda: factory(stdlib_base64.b64encode))


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
