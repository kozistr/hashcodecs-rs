from __future__ import annotations

import base64 as stdlib_base64
import warnings
from collections.abc import Callable, Mapping
from dataclasses import dataclass, field
from typing import Literal

WarningFilter = Literal['always', 'error']


@dataclass(frozen=True)
class Returned:
    type: type[object]
    value: object


@dataclass(frozen=True)
class Raised:
    type: type[BaseException]
    arguments: tuple[object, ...]
    message: str
    is_sentinel: bool


@dataclass(frozen=True)
class WarningRecord:
    category: type[Warning]
    message: str


@dataclass(frozen=True)
class MutableState:
    name: str
    contents: bytes | None
    length: int | None
    error: Raised | None


@dataclass(frozen=True)
class BufferState:
    name: str
    resize_succeeded: bool
    resize_error: Raised | None


@dataclass(frozen=True)
class OutputState:
    name: str
    contents: bytes
    length: int
    written_prefix: bytes
    untouched_suffix: bytes
    failed: bool


@dataclass(frozen=True)
class Observation:
    outcome: Returned | Raised
    warnings: tuple[WarningRecord, ...]
    callbacks: tuple[str, ...]
    mutables: tuple[MutableState, ...]
    buffers: tuple[BufferState, ...]
    outputs: tuple[OutputState, ...]


@dataclass
class Invocation:
    call: Callable[[], object]
    events: list[str]
    sentinel: BaseException
    mutables: Mapping[str, object] = field(default_factory=dict)
    buffers: tuple[BufferHook, ...] = ()
    outputs: Mapping[str, bytearray] = field(default_factory=dict)
    output_initial: Mapping[str, bytes] = field(default_factory=dict)


def observe(factory: Callable[[], Invocation], warning_filter: WarningFilter = 'always') -> Observation:
    invocation = factory()
    result: object | None = None
    error: BaseException | None = None
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter(warning_filter)
        try:
            result = invocation.call()
        except BaseException as caught_error:
            error = caught_error

    outcome = Returned(type(result), result) if error is None else raised(error, invocation.sentinel)

    mutations = tuple(_mutable_state(name, value, invocation.sentinel) for name, value in invocation.mutables.items())
    outputs = tuple(
        _output_state(
            name,
            output,
            invocation.output_initial[name],
            result if error is None else None,
            error is not None,
        )
        for name, output in invocation.outputs.items()
    )
    buffers = tuple(buffer.after_call(invocation.sentinel) for buffer in invocation.buffers)
    return Observation(
        outcome,
        tuple(WarningRecord(item.category, str(item.message)) for item in caught),
        tuple(invocation.events),
        mutations,
        buffers,
        outputs,
    )


def observe_call(call: Callable[[], object], warning_filter: WarningFilter = 'always') -> Observation:
    return observe(lambda: Invocation(call, [], RuntimeError('unused sentinel')), warning_filter)


def raised(error: BaseException, sentinel: BaseException) -> Raised:
    return Raised(type(error), error.args, str(error), error is sentinel)


def _mutable_state(name: str, value: object, sentinel: BaseException) -> MutableState:
    try:
        return MutableState(name, bytes(value), len(value), None)  # type: ignore[arg-type]
    except BaseException as error:
        return MutableState(name, None, None, raised(error, sentinel))


def _output_state(
    name: str,
    output: bytearray,
    initial: bytes,
    result: object | None,
    failed: bool,
) -> OutputState:
    contents = bytes(output)
    if type(result) is int and 0 <= result <= len(output):
        boundary = result
    else:
        changed = [index for index, pair in enumerate(zip(initial, contents, strict=False)) if pair[0] != pair[1]]
        boundary = changed[-1] + 1 if changed else 0
        if len(initial) != len(contents):
            boundary = max(boundary, min(len(initial), len(contents)))
    suffix = contents[boundary:] if contents[boundary:] == initial[boundary:] else b''
    return OutputState(name, contents, len(output), contents[:boundary], suffix, failed)


@dataclass(frozen=True)
class Action:
    kind: Literal['normal', 'invalid', 'raise', 'replace', 'grow', 'shrink', 'reenter']
    value: object | None = None

    def run(
        self,
        name: str,
        events: list[str],
        sentinel: BaseException,
        normal: object,
        invalid: object,
        target: bytearray | None,
        reentrant: Callable[[], object] | None,
    ) -> object:
        events.append(name)
        if self.kind == 'raise':
            raise sentinel
        if self.kind == 'invalid':
            return invalid if self.value is None else self.value
        if self.kind == 'replace':
            _replace(target)
        elif self.kind == 'grow':
            _grow(target)
        elif self.kind == 'shrink':
            _shrink(target)
        elif self.kind == 'reenter':
            (reentrant or _default_reentrant)()
        return normal if self.value is None else self.value


NORMAL = Action('normal')


def _replace(target: bytearray | None) -> None:
    if target is None:
        return
    target[:] = bytes(byte ^ 0x20 for byte in target)


def _grow(target: bytearray | None) -> None:
    if target is not None:
        target.extend(b'!')


def _shrink(target: bytearray | None) -> None:
    if target:
        del target[-1]


def _default_reentrant() -> None:
    assert stdlib_base64.b64decode(b'WVdKag==') == b'YWJj'


class BoolHook:
    def __init__(
        self,
        name: str,
        action: Action,
        events: list[str],
        sentinel: BaseException,
        value: bool = True,
        *,
        target: bytearray | None = None,
        reentrant: Callable[[], object] | None = None,
    ) -> None:
        self.name = name
        self.action = action
        self.events = events
        self.sentinel = sentinel
        self.value = value
        self.target = target
        self.reentrant = reentrant

    def __bool__(self) -> bool:
        return self.action.run(
            f'{self.name}.__bool__',
            self.events,
            self.sentinel,
            self.value,
            1,
            self.target,
            self.reentrant,
        )  # type: ignore[return-value]


class IndexHook:
    def __init__(
        self,
        name: str,
        action: Action,
        events: list[str],
        sentinel: BaseException,
        value: int = 0,
        *,
        target: bytearray | None = None,
        reentrant: Callable[[], object] | None = None,
    ) -> None:
        self.name = name
        self.action = action
        self.events = events
        self.sentinel = sentinel
        self.value = value
        self.target = target
        self.reentrant = reentrant

    def __index__(self) -> int:
        return self.action.run(
            f'{self.name}.__index__',
            self.events,
            self.sentinel,
            self.value,
            'invalid index',
            self.target,
            self.reentrant,
        )  # type: ignore[return-value]


class AltcharsHook(bytes):
    def __new__(  # noqa: PYI034
        cls,
        value: bytes,
        events: list[str],
        sentinel: BaseException,
        *,
        length: Action = NORMAL,
        radd: Action = NORMAL,
        representation: Action = NORMAL,
        alphabet: object | None = None,
        target: bytearray | None = None,
        reentrant: Callable[[], object] | None = None,
    ) -> AltcharsHook:
        instance = super().__new__(cls, value)
        instance.events = events
        instance.sentinel = sentinel
        instance.length_action = length
        instance.radd_action = radd
        instance.repr_action = representation
        instance.alphabet = alphabet
        instance.target = target
        instance.reentrant = reentrant
        return instance

    def __len__(self) -> int:
        return self.length_action.run(
            'altchars.__len__',
            self.events,
            self.sentinel,
            bytes.__len__(self),
            'invalid length',
            self.target,
            self.reentrant,
        )  # type: ignore[return-value]

    def __radd__(self, other: object) -> object:
        normal = self.alphabet if self.alphabet is not None else bytes(other) + bytes(self)
        return self.radd_action.run(
            'altchars.__radd__',
            self.events,
            self.sentinel,
            normal,
            'invalid alphabet',
            self.target,
            self.reentrant,
        )

    def __repr__(self) -> str:
        return self.repr_action.run(
            'altchars.__repr__',
            self.events,
            self.sentinel,
            bytes.__repr__(self),
            1,
            self.target,
            self.reentrant,
        )  # type: ignore[return-value]


class EncodeHook(str):
    def __new__(  # noqa: PYI034
        cls,
        value: str,
        action: Action,
        events: list[str],
        sentinel: BaseException,
        *,
        encoded: object | None = None,
        target: bytearray | None = None,
        reentrant: Callable[[], object] | None = None,
    ) -> EncodeHook:
        instance = super().__new__(cls, value)
        instance.action = action
        instance.events = events
        instance.sentinel = sentinel
        instance.encoded = value.encode('ascii') if encoded is None else encoded
        instance.target = target
        instance.reentrant = reentrant
        return instance

    def encode(self, encoding: str = 'utf-8', errors: str = 'strict') -> bytes:
        return self.action.run(
            'input.encode',
            self.events,
            self.sentinel,
            self.encoded,
            'invalid encoding',
            self.target,
            self.reentrant,
        )  # type: ignore[return-value]


class TranslateHook(bytes):
    def __new__(  # noqa: PYI034
        cls,
        value: bytes,
        action: Action,
        events: list[str],
        sentinel: BaseException,
        *,
        translated: object | None = None,
        target: bytearray | None = None,
        reentrant: Callable[[], object] | None = None,
    ) -> TranslateHook:
        instance = super().__new__(cls, value)
        instance.action = action
        instance.events = events
        instance.sentinel = sentinel
        instance.translated = value if translated is None else translated
        instance.target = target
        instance.reentrant = reentrant
        return instance

    def translate(self, table: bytes) -> bytes:
        return self.action.run(
            'input.translate',
            self.events,
            self.sentinel,
            self.translated,
            1,
            self.target,
            self.reentrant,
        )  # type: ignore[return-value]


class BufferHook:
    def __init__(
        self,
        name: str,
        contents: bytes,
        events: list[str],
        sentinel: BaseException,
        *,
        acquire: Action = NORMAL,
        release: Action = NORMAL,
        target: bytearray | None = None,
        reentrant: Callable[[], object] | None = None,
    ) -> None:
        self.name = name
        self.owner = bytearray(contents)
        self.events = events
        self.sentinel = sentinel
        self.acquire_action = acquire
        self.release_action = release
        self.target = target
        self.reentrant = reentrant

    def __buffer__(self, flags: int) -> memoryview:
        return self.acquire_action.run(
            f'{self.name}.__buffer__',
            self.events,
            self.sentinel,
            memoryview(self.owner),
            self.owner,
            self.target,
            self.reentrant,
        )  # type: ignore[return-value]

    def __release_buffer__(self, view: memoryview) -> None:
        self.release_action.run(
            f'{self.name}.__release_buffer__',
            self.events,
            self.sentinel,
            None,
            1,
            self.target,
            self.reentrant,
        )

    def after_call(self, sentinel: BaseException) -> BufferState:
        try:
            self.owner.append(0)
            self.owner.pop()
        except BaseException as error:
            return BufferState(self.name, False, raised(error, sentinel))
        return BufferState(self.name, True, None)
