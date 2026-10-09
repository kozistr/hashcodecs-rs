import base64 as stdlib_base64
import sys
from collections.abc import Callable
from typing import Any

import pytest
from base64_compat_harness import (
    Action,
    ActionKind,
    AltcharsHook,
    BoolHook,
    BufferHook,
    IndexHook,
    Invocation,
    SentinelError,
    observe,
)

import hashcodecs.base64 as base64

PYTHON_315 = sys.version_info >= (3, 15)

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

ENCODE_INTO_ACTION_CASES = tuple(
    ('padded', action) for action in ('invalid', 'raise', 'replace', 'grow', 'shrink', 'reenter')
) + tuple(('input.release', action) for action in ('replace', 'grow', 'shrink', 'reenter'))


@pytest.mark.skipif(not PYTHON_315, reason='requires the CPython 3.15 Base64 API')
@pytest.mark.parametrize('urlsafe', [False, True])
def test_python_315_encode_keeps_input_exported_while_converting_padded(urlsafe: bool) -> None:
    def run(function: Callable[..., bytes]) -> tuple[type[Exception] | bytes, bytes]:
        source = bytearray(b'abc')

        class Padded:
            def __bool__(self) -> bool:
                source.extend(b'def')
                return True

        try:
            result: type[Exception] | bytes = function(source, padded=Padded())
        except Exception as error:
            result = type(error)
        return result, bytes(source)

    expected_function = stdlib_base64.urlsafe_b64encode if urlsafe else stdlib_base64.b64encode
    actual_function = base64.urlsafe_b64encode if urlsafe else base64.b64encode
    assert run(actual_function) == run(expected_function)


@pytest.mark.skipif(PYTHON_315, reason='CPython 3.15 constructs altchars before acquiring the input')
def test_legacy_encode_releases_input_export_before_altchars_callbacks() -> None:
    def run(function: Callable[..., bytes]) -> tuple[bytes, bytes]:
        source = bytearray(b'abc')

        class Altchars(bytes):
            def __len__(self) -> int:
                source.extend(b'def')
                return 2

        return function(source, Altchars(b'-_')), bytes(source)

    assert run(base64.b64encode) == run(stdlib_base64.b64encode)


@pytest.mark.skipif(not PYTHON_315, reason='requires the CPython 3.15 Base64 API')
def test_python_315_encode_argument_conversion_order_matches_cpython() -> None:
    def run(function: Callable[..., bytes]) -> tuple[bytes, list[str]]:
        events: list[str] = []

        class Buffer:
            def __init__(self, value: bytes, name: str) -> None:
                self.value = value
                self.name = name

            def __buffer__(self, flags: int) -> memoryview:
                events.append(f'{self.name}.__buffer__')
                return memoryview(self.value)

        class Altchars:
            def __len__(self) -> int:
                events.append('altchars.__len__')
                return 2

            def __radd__(self, other: object) -> Buffer:
                events.append('altchars.__radd__')
                return Buffer(b'Z' * 64, 'alphabet')

        class Padded:
            def __bool__(self) -> bool:
                events.append('padded.__bool__')
                return True

        class Wrapcol:
            def __index__(self) -> int:
                events.append('wrapcol.__index__')
                return 0

        result = function(Buffer(b'abc', 'input'), Altchars(), padded=Padded(), wrapcol=Wrapcol())
        return result, events

    expected = run(stdlib_base64.b64encode)
    assert expected == (
        b'ZZZZ',
        [
            'altchars.__len__',
            'altchars.__radd__',
            'input.__buffer__',
            'padded.__bool__',
            'wrapcol.__index__',
            'alphabet.__buffer__',
        ],
    )
    assert run(base64.b64encode) == expected


def _encode_callback_case(function: Callable[..., bytes]) -> Callable[[], Invocation]:
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
def test_encode_callback_order() -> None:
    expected = observe(_encode_callback_case(stdlib_base64.b64encode))
    actual = observe(_encode_callback_case(base64.b64encode))
    assert actual == expected


def _encode_action_case(
    function: Callable[..., bytes], hook: str, action_name: ActionKind
) -> Callable[[], Invocation]:
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
def test_encode_callback_actions(hook: str, action_name: ActionKind) -> None:
    expected = observe(_encode_action_case(stdlib_base64.b64encode, hook, action_name))
    actual = observe(_encode_action_case(base64.b64encode, hook, action_name))
    assert actual == expected


def _encode_into_action_case(
    function: Callable[..., Any],
    native_into: bool,
    hook: str,
    action_name: ActionKind,
) -> Callable[[], Invocation]:
    def factory() -> Invocation:
        sentinel = SentinelError('sentinel')
        events: list[str] = []
        output = bytearray(b'....')
        initial = bytes(output)
        action = Action(action_name)

        def reentrant() -> object:
            if native_into:
                return function(b'x', bytearray(4))
            return function(b'x')

        if hook == 'input.release':
            source_buffer = BufferHook(
                'input',
                b'abc',
                events,
                sentinel,
                release=action,
                target=output,
                reentrant=reentrant,
            )
            source: object = source_buffer
            buffers = (source_buffer,)
            padded: object = True
        else:
            source = b'abc'
            buffers = ()
            padded = BoolHook('padded', action, events, sentinel, target=output, reentrant=reentrant)

        def call() -> int:
            if native_into:
                return function(source, output, padded=padded)
            result = function(source, padded=padded)
            if len(output) < len(result):
                raise ValueError(f'Base64 output requires {len(result)} bytes but the destination has {len(output)}')
            output[: len(result)] = result
            return len(result)

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
@pytest.mark.parametrize(('hook', 'action_name'), ENCODE_INTO_ACTION_CASES)
def test_encode_destination_actions(
    hook: str,
    action_name: ActionKind,
) -> None:
    expected = observe(_encode_into_action_case(stdlib_base64.urlsafe_b64encode, False, hook, action_name))
    actual = observe(_encode_into_action_case(base64.urlsafe_b64encode_into, True, hook, action_name))
    assert actual == expected
