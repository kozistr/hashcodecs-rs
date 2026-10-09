import base64 as stdlib_base64
import binascii
import sys
from collections.abc import Callable

import pytest
from base64_compat_harness import (
    Action,
    BufferHook,
    Invocation,
    Raised,
    Returned,
    SentinelError,
    WarningFilter,
    observe,
)

import hashcodecs.base64 as base64

PYTHON_315 = sys.version_info >= (3, 15)


def _batch_decode_case(mode: str, warning_case: bool = False) -> Callable[[], Invocation]:
    def factory() -> Invocation:
        sentinel = SentinelError('sentinel')
        values = [b'YWJj', b'++8=', b'//8=', b'ZGVm'] if warning_case else [b'YWJj', b'YWJjY===', b'ZGVm']
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


def _batch_encode_case(batch: bool) -> Callable[[], Invocation]:
    def factory() -> Invocation:
        sentinel = SentinelError('sentinel')
        values = [b'abc', b'def', b'ghi']
        outputs = [bytearray(b'.....'), bytearray(b'...'), bytearray(b'.....')]
        initial = {f'output{index}': bytes(output) for index, output in enumerate(outputs)}

        def call() -> list[int]:
            if batch:
                return base64.b64encode_batch_into(values, outputs)
            return [base64.b64encode_into(value, output) for value, output in zip(values, outputs, strict=True)]

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
def test_batch_warning_order(
    warning_filter: WarningFilter,
) -> None:
    actual = observe(_batch_decode_case('batch', warning_case=True), warning_filter)
    assert actual == observe(_batch_decode_case('cpython', warning_case=True), warning_filter)
    if warning_filter == 'error':
        assert isinstance(actual.outcome, Raised)
        assert actual.outcome.type is FutureWarning
        assert [state.contents for state in actual.outputs] == [b'abc...', b'......', b'......', b'......']
    else:
        assert actual.outcome == Returned(list, [3, 2, 2, 3])
        assert [warning.category for warning in actual.warnings] == [FutureWarning, FutureWarning]
        assert [warning.message.split("'")[1] for warning in actual.warnings] == ['+', '/']
        assert [state.contents for state in actual.outputs] == [
            b'abc...',
            b'\xfb\xef....',
            b'\xff\xff....',
            b'def...',
        ]


def test_batch_failure_state() -> None:
    actual = observe(_batch_decode_case('batch'))
    assert actual == observe(_batch_decode_case('into'))
    assert isinstance(actual.outcome, Raised)
    assert actual.outcome.type is binascii.Error
    assert [state.contents for state in actual.outputs] == [b'abc...', b'abc...', b'......']


def test_encode_batch_failure_state() -> None:
    actual = observe(_batch_encode_case(True))
    assert actual == observe(_batch_encode_case(False))
    assert isinstance(actual.outcome, Raised)
    assert actual.outcome.type is ValueError
    assert [state.contents for state in actual.outputs] == [b'YWJj.', b'...', b'.....']


@pytest.mark.skipif(not PYTHON_315, reason='requires Python-level buffer protocol support')
@pytest.mark.parametrize('operation', ['encode', 'decode'])
def test_batch_callback_fail_fast(operation: str) -> None:
    def case(batch: bool) -> Callable[[], Invocation]:
        def factory() -> Invocation:
            sentinel = SentinelError('sentinel')
            events: list[str] = []
            if operation == 'encode':
                inputs = (
                    BufferHook('input0', b'abc', events, sentinel),
                    BufferHook('input1', b'def', events, sentinel, acquire=Action('raise')),
                    BufferHook('input2', b'ghi', events, sentinel),
                )
            else:
                inputs = (
                    BufferHook('input0', b'YWJj', events, sentinel),
                    BufferHook('input1', b'YWJ!', events, sentinel),
                    BufferHook('input2', b'ZGVm', events, sentinel),
                )

            def call() -> list[bytes]:
                if operation == 'encode':
                    if batch:
                        return base64.b64encode_batch(list(inputs))
                    return [stdlib_base64.b64encode(value) for value in inputs]
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

    actual = observe(case(True))
    assert actual == observe(case(False))
    expected_callbacks = ['input0.__buffer__', 'input0.__release_buffer__', 'input1.__buffer__']
    if operation == 'decode':
        expected_callbacks.append('input1.__release_buffer__')
    assert list(actual.callbacks) == expected_callbacks
