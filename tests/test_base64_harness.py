from collections.abc import Callable

from base64_compat_harness import (
    Invocation,
    Observation,
    Raised,
    Returned,
    SentinelError,
    observe,
)


def test_harness_exception_details() -> None:
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


def test_harness_return_type() -> None:
    def observed(value: object) -> Observation:
        return observe(lambda: Invocation(lambda: value, [], SentinelError('sentinel')))

    assert observed(b'abc') != observed(bytearray(b'abc'))
    outcome = observed(b'abc').outcome
    assert outcome == Returned(bytes, b'abc')
