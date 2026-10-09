import base64 as stdlib_base64
import binascii
import sys
from collections.abc import Callable
from typing import Any, cast

import pytest
from base64_compat_harness import (
    Action,
    ActionKind,
    AltcharsHook,
    BoolHook,
    BufferHook,
    EncodeHook,
    Invocation,
    Observation,
    SentinelError,
    TranslateHook,
    WarningFilter,
    observe,
)

import hashcodecs.base64 as base64

PYTHON_315 = sys.version_info >= (3, 15)

BASE64_ALPHABET = getattr(
    binascii, 'BASE64_ALPHABET', b'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/'
)

dynamic_b64decode: Callable[..., bytes] = base64.b64decode

dynamic_b64decode_into: Callable[..., int] = base64.b64decode_into

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


@pytest.mark.parametrize('urlsafe', [False, True])
@pytest.mark.parametrize('reusable', [False, True])
@pytest.mark.parametrize('translated', [b'ZGVm', 'ZGVm', 'é'])
def test_decode_dispatches_translate_for_bytes_subclasses(
    urlsafe: bool, reusable: bool, translated: bytes | str
) -> None:
    class Source(bytes):
        def translate(self, table: Any, delete: Any = b'') -> Any:
            return translated

    source = Source(b'YWJj')
    if translated == 'é':
        reference_function = stdlib_base64.urlsafe_b64decode if urlsafe else stdlib_base64.b64decode
        reference_args = (source,) if urlsafe else (source, b'-_')
        with pytest.raises(ValueError, match='ASCII') as reference:
            reference_function(*reference_args)
        function = getattr(base64, f'{"urlsafe_" if urlsafe else ""}b64decode{"_into" if reusable else ""}')
        output = bytearray(b'....')
        args = (source, output) if reusable else (source,)
        if not urlsafe:
            args += (b'-_',)
        with pytest.raises(type(reference.value)) as actual:
            function(*args)
        assert str(actual.value) == str(reference.value)
        assert output == b'....'
        return

    expected = stdlib_base64.urlsafe_b64decode(source) if urlsafe else stdlib_base64.b64decode(source, b'-_')
    if not reusable:
        function = cast(Callable[..., bytes], base64.urlsafe_b64decode if urlsafe else base64.b64decode)
        args = (source,) if urlsafe else (source, b'-_')
        assert function(*args) == expected
        return

    output = bytearray(len(expected))
    if urlsafe:
        written = base64.urlsafe_b64decode_into(source, output, padded=True)
    else:
        written = base64.b64decode_into(source, output, b'-_')
    assert written == len(expected)
    assert output == expected


@pytest.mark.parametrize('urlsafe', [False, True])
@pytest.mark.parametrize('reusable', [False, True])
@pytest.mark.parametrize(
    'hook',
    [
        'translate',
        pytest.param('contains', marks=pytest.mark.skipif(not PYTHON_315, reason='requires legacy-symbol warnings')),
    ],
)
def test_propagate_input_errors(urlsafe: bool, reusable: bool, hook: str) -> None:
    failure = SentinelError(hook)

    class Source(bytes):
        def __contains__(self, item: object) -> bool:
            if hook == 'contains':
                raise failure
            return super().__contains__(item)

        def translate(self, table: Any, delete: Any = b'') -> bytes:
            raise failure

    source = Source(b'-_8=')
    reference = stdlib_base64.urlsafe_b64decode if urlsafe else stdlib_base64.b64decode
    args = (source,) if urlsafe else (source, b'-_')
    with pytest.raises(SentinelError) as expected:
        reference(*args)
    assert expected.value is failure

    output = bytearray(b'....')
    function = getattr(base64, f'{"urlsafe_" if urlsafe else ""}b64decode{"_into" if reusable else ""}')
    args = (source, output) if reusable else (source,)
    if not urlsafe:
        args += (b'-_',)
    with pytest.raises(SentinelError) as actual:
        function(*args)
    assert actual.value is failure
    assert output == b'....'


@pytest.mark.skipif(not PYTHON_315, reason='requires the CPython 3.15 Base64 API')
@pytest.mark.parametrize('reusable', [False, True])
@pytest.mark.parametrize('configured', [False, True])
def test_python_315_decode_argument_conversion_order_matches_cpython(reusable: bool, configured: bool) -> None:
    def run(function: Callable[..., bytes | int], into: bool) -> tuple[bytes, list[str]]:
        events: list[str] = []

        class Buffer:
            def __init__(self, value: bytes, name: str) -> None:
                self.value = value
                self.name = name

            def __buffer__(self, flags: int) -> memoryview:
                events.append(f'{self.name}.__buffer__')
                return memoryview(self.value)

            def __release_buffer__(self, view: memoryview) -> None:
                events.append(f'{self.name}.__release_buffer__')

        class Altchars(bytes):
            def __len__(self) -> int:
                events.append('altchars.__len__')
                return 2

            def __radd__(self, other: object) -> bytes:
                events.append('altchars.__radd__')
                return BASE64_ALPHABET

        class Option:
            def __init__(self, name: str, value: bool) -> None:
                self.name = name
                self.value = value

            def __bool__(self) -> bool:
                events.append(f'{self.name}.__bool__')
                return self.value

        kwargs: dict[str, object] = {
            'validate': Option('validate', True),
            'padded': Option('padded', True),
            'canonical': Option('canonical', False),
        }
        if configured:
            kwargs['ignorechars'] = Buffer(b'', 'ignorechars')
        args: tuple[object, ...] = (Buffer(b'YWJj', 'input'), Altchars(b'-_'))
        output = bytearray(3)
        if into:
            written = function(args[0], output, args[1], **kwargs)
            assert isinstance(written, int)
            result = bytes(output[:written])
        else:
            result = function(*args, **kwargs)
            assert isinstance(result, bytes)
        return result, events

    expected = run(stdlib_base64.b64decode, False)
    if reusable:
        assert run(base64.b64decode_into, True) == expected
    else:
        assert run(base64.b64decode, False) == expected


@pytest.mark.skipif(not PYTHON_315, reason='requires Python 3.15 compatibility warnings')
@pytest.mark.parametrize('urlsafe', [False, True])
@pytest.mark.parametrize('reusable', [False, True])
def test_python_315_translated_subclasses_retain_altchar_warnings(urlsafe: bool, reusable: bool) -> None:
    class Source(bytes):
        pass

    source = Source(b'++8=')
    output = bytearray(2)
    if urlsafe:
        function = cast(Callable[..., object], base64.urlsafe_b64decode_into if reusable else base64.urlsafe_b64decode)
        args = (source, output) if reusable else (source,)
        kwargs = {'padded': True}
    else:
        function = cast(Callable[..., object], base64.b64decode_into if reusable else base64.b64decode)
        args = (source, output, b'-_') if reusable else (source, b'-_')
        kwargs = {}

    with pytest.warns(FutureWarning, match="invalid character '\\+'"):
        result = function(*args, **kwargs)

    if reusable:
        assert result == 2
        assert output == b'\xfb\xef'
    else:
        assert result == b'\xfb\xef'


@pytest.mark.parametrize('reusable', [False, True])
def test_sliced_decode_snapshots_before_truthiness_callbacks(reusable: bool) -> None:
    storage = bytearray(b'!YWJj!')
    view = memoryview(storage)[1:-1]
    output = bytearray(4)

    class Validate:
        def __bool__(self) -> bool:
            storage[1:-1] = b'ZGVm'
            view.release()
            output.clear()
            return True

    if reusable:
        with pytest.raises(ValueError, match='destination has 0'):
            dynamic_b64decode_into(view, output, validate=Validate())
        assert output == b''
    else:
        assert dynamic_b64decode(view, validate=Validate()) == b'abc'


@pytest.mark.skipif(sys.version_info < (3, 12), reason='requires Python buffer release hooks')
@pytest.mark.parametrize(
    ('operation', 'value', 'expected'),
    [
        (base64.standard_b64encode_into, b'abc', b'YWJj'),
        (base64.b64encode_into, b'abc', b'YWJj'),
        (base64.urlsafe_b64encode_into, b'abc', b'YWJj'),
        (base64.standard_b64decode_into, b'YWJj', b'abc'),
        (base64.b64decode_into, b'YWJj', b'abc'),
        (base64.urlsafe_b64decode_into, b'YWJj', b'abc'),
    ],
)
@pytest.mark.parametrize('remaining_capacity', [0, 3, 4, 16])
def test_reentrant_buffer_release_hooks_run_before_reusable_output_writes(
    operation: Callable[..., int], value: bytes, expected: bytes, remaining_capacity: int
) -> None:
    output = bytearray(16)
    releases = []

    class Buffer:
        def __buffer__(self, flags: int) -> memoryview:
            return memoryview(value)

        def __release_buffer__(self, view: memoryview) -> None:
            releases.append(True)
            output[:] = b'.' * remaining_capacity

    if remaining_capacity < len(expected):
        with pytest.raises(
            ValueError, match=f'requires {len(expected)} bytes but the destination has {remaining_capacity}'
        ):
            operation(Buffer(), output)
        assert output == b'.' * remaining_capacity
    else:
        assert operation(Buffer(), output) == len(expected)
        assert output == expected + b'.' * (remaining_capacity - len(expected))
    assert releases == [True]


@pytest.mark.skipif(sys.version_info < (3, 12), reason='requires Python buffer release hooks')
def test_reentrant_ignorechars_release_hook_runs_before_reusable_output_write() -> None:
    class Buffer:
        def __init__(self, value: bytes, output: bytearray) -> None:
            self.value = value
            self.output = output

        def __buffer__(self, flags: int) -> memoryview:
            return memoryview(self.value)

        def __release_buffer__(self, view: memoryview) -> None:
            self.output.clear()

    decoded = bytearray(3)
    with pytest.raises(ValueError, match='requires 3 bytes but the destination has 0'):
        base64.b64decode_into(b'YWJj', decoded, ignorechars=Buffer(b'', decoded))
    assert decoded == b''


def test_subclasses_and_python_buffer_hooks_follow_cpython_slow_path() -> None:
    class BytesSubclass(bytes):
        pass

    class ByteArraySubclass(bytearray):
        pass

    class StringSubclass(str):
        encode_calls: int

        def __new__(cls, value: str):
            instance = super().__new__(cls, value)
            instance.encode_calls = 0
            return instance

        def encode(self, encoding: str = 'utf-8', errors: str = 'strict') -> bytes:
            self.encode_calls += 1
            return super().encode(encoding, errors)

    assert base64.b64encode(BytesSubclass(b'abc')) == b'YWJj'
    assert base64.b64encode(ByteArraySubclass(b'abc')) == b'YWJj'
    text = StringSubclass('YWJj')
    assert base64.b64decode(text, validate=True) == b'abc'
    assert text.encode_calls == 1

    class RaisingString(str):
        def encode(self, encoding: str = 'utf-8', errors: str = 'strict') -> bytes:
            raise RuntimeError('custom encode failure')

    with pytest.raises(RuntimeError, match='custom encode failure'):
        base64.b64decode(RaisingString('YWJj'))

    if sys.version_info >= (3, 12):  # noqa: UP036 - package supports Python 3.10.

        class BufferHook:
            def __init__(self, value: bytes) -> None:
                self.value = value
                self.calls = 0

            def __buffer__(self, flags: int) -> memoryview:
                self.calls += 1
                return memoryview(self.value)

        encoded = BufferHook(b'YWJj')
        payload = BufferHook(b'abc')
        assert base64.b64decode(encoded, validate=True) == b'abc'
        assert base64.b64encode(payload) == b'YWJj'
        assert encoded.calls == 1
        assert payload.calls == 1

        class ExportFailure(RuntimeError):
            pass

        class RaisingBuffer:
            def __init__(self) -> None:
                self.calls = 0

            def __buffer__(self, flags: int) -> memoryview:
                self.calls += 1
                raise ExportFailure('custom export failure')

        raising = RaisingBuffer()
        with pytest.raises(ExportFailure, match='custom export failure'):
            base64.b64encode(raising)
        assert raising.calls == 1

        class BufferList(list):
            def __buffer__(self, flags: int) -> memoryview:
                return memoryview(b'abc')

        assert base64.b64encode(BufferList()) == b'YWJj'


def _decode_into_case(
    function: Callable[..., Any],
    native_into: bool,
    urlsafe: bool,
) -> Callable[[], Invocation]:
    def factory() -> Invocation:
        sentinel = SentinelError('sentinel')
        output = bytearray(b'.....')
        initial = bytes(output)

        if native_into:

            def call() -> object:
                if urlsafe:
                    return function(b'++8=', output)
                return function(b'++8=', output, b'-_')
        else:

            def call() -> int:
                decoded = function(b'++8=') if urlsafe else function(b'++8=', b'-_')
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
@pytest.mark.parametrize('urlsafe', [False, True])
def test_decode_into_warning_cleanup(warning_filter: WarningFilter, urlsafe: bool) -> None:
    reference = stdlib_base64.urlsafe_b64decode if urlsafe else stdlib_base64.b64decode
    candidate = base64.urlsafe_b64decode_into if urlsafe else base64.b64decode_into
    expected = observe(_decode_into_case(reference, False, urlsafe), warning_filter)
    actual = observe(_decode_into_case(candidate, True, urlsafe), warning_filter)
    assert actual == expected


def _decode_action_case(
    function: Callable[..., bytes], hook: str, action_name: ActionKind
) -> Callable[[], Invocation]:
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
def test_decode_callback_actions(hook: str, action_name: ActionKind) -> None:
    expected = observe(_decode_action_case(stdlib_base64.b64decode, hook, action_name))
    actual = observe(_decode_action_case(base64.b64decode, hook, action_name))
    assert actual == expected


@pytest.mark.parametrize('encoded', ['YWJj', 'é', '\ud800'])
def test_decode_encode_result(encoded: str) -> None:
    def run(function: Callable[..., bytes]) -> Observation:
        def factory() -> Invocation:
            sentinel = SentinelError('sentinel')
            events: list[str] = []
            source = EncodeHook('ignored', Action('normal'), events, sentinel, encoded=encoded)
            return Invocation(lambda: function(source), events, sentinel)

        return observe(factory)

    assert run(base64.b64decode) == run(stdlib_base64.b64decode)


def _decode_into_action_case(
    function: Callable[..., Any],
    native_into: bool,
    hook: str,
    action_name: ActionKind,
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
def test_decode_destination_actions(hook: str, action_name: ActionKind) -> None:
    expected = observe(_decode_into_action_case(stdlib_base64.b64decode, False, hook, action_name))
    actual = observe(_decode_into_action_case(base64.b64decode_into, True, hook, action_name))
    assert actual == expected


def _invalid_args_case(function: Callable[..., bytes], scenario: str) -> Callable[[], Invocation]:
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
def test_invalid_argument_precedence(
    scenario: str, reference: Callable[..., bytes], candidate: Callable[..., bytes]
) -> None:
    assert observe(_invalid_args_case(candidate, scenario)) == observe(_invalid_args_case(reference, scenario))
