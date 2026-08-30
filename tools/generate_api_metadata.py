"""Generate Python and native API metadata from the typed extension stub."""

from __future__ import annotations

import argparse
import ast
import json
import re
import sys
import textwrap
from collections.abc import Iterable
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
STUB = ROOT / 'hashcodecs' / '_hashcodecs.pyi'

MODULES = ('base64', 'murmur3', 'xxhash')
MODULE_DOCSTRINGS = {
    'base64': "An API-compatible subset of Python's :mod:`base64` module.",
    'murmur3': 'One-shot and incremental MurmurHash3 functions.',
    'xxhash': 'Canonical XXH3 functions.',
}
API_TITLES = {
    'base64': 'Base64 API Reference',
    'murmur3': 'MurmurHash3 API Reference',
    'xxhash': 'XXH3 API Reference',
}
API_FILENAMES = {'base64': 'base64.md', 'murmur3': 'murmur3.md', 'xxhash': 'xxh3.md'}
API_INTRODUCTIONS = {
    'base64': (
        'This reference uses the authoritative typed API declaration. '
        'It shows the exact signatures and behavior of `hashcodecs.base64`.'
    ),
    'murmur3': (
        'This reference uses the authoritative typed API declaration for all functions and incremental hashers.'
    ),
    'xxhash': 'This reference uses the authoritative typed API declaration for all functions.',
}
METHOD_FILES = {
    'murmur3': ROOT / 'src' / 'bindings' / 'murmur3' / 'methods.rs',
    'xxhash': ROOT / 'src' / 'bindings' / 'xxhash' / 'methods.rs',
}
MURMUR_INCREMENTAL = ROOT / 'src' / 'bindings' / 'murmur3' / 'incremental.rs'
BASE64_CALLBACKS = ROOT / 'src' / 'bindings' / 'base64' / 'callbacks.rs'
BASE64_GENERATED_SCHEMA = ROOT / 'src' / 'bindings' / 'base64' / 'schema_generated.rs'
GENERATED_HEADER = '# tools/generate_api_metadata.py generates this file from hashcodecs/_hashcodecs.pyi.\n'
RUST_GENERATED_HEADER = '// tools/generate_api_metadata.py generates this file from hashcodecs/_hashcodecs.pyi.\n'
METHOD_DOC_PATTERN = re.compile(
    r'(?P<prefix>ml_doc:\s+cr(?P<hashes>\#*)")(?P<body>.*?)(?P<suffix>"(?P=hashes)\s*\.as_ptr\(\),)',
    re.DOTALL,
)
METHOD_ENTRY_PATTERN = re.compile(
    r'ml_name:\s+c"(?P<name>[^"]+)"\.as_ptr\(\),\s*'
    r'ml_meth:\s+ffi::PyMethodDefPointer\s*\{\s*'
    r'PyCFunctionFastWithKeywords:\s*(?P<callback>[a-z0-9_]+),',
    re.DOTALL,
)
MURMUR_CLASS_DOC_PATTERN = re.compile(
    r'(?P<prefix>define_python_hasher!\(\s*'
    r'[A-Za-z0-9_]+,\s*'
    r'[A-Za-z0-9_]+,\s*'
    r'"(?P<name>murmur3_[a-z0-9_]+)",\s*)'
    r'(?P<body>.*?)'
    r'(?P<suffix>,\s*\n\s*\d+,\s*\n\s*\d+,)',
    re.DOTALL,
)
CALLBACK_PATTERN = re.compile(
    r'callback!\s*\{\s*'
    r'(?P<name>[a-z0-9_]+),\s*'
    r'\|(?P<py>[a-z0-9_]+)\s*;\s*(?P<parameters>[a-z0-9_,\s]+)\|\s*\{',
)


@dataclass(frozen=True)
class VersionedDefault:
    since: tuple[int, int]
    before: bool
    after: bool


VERSIONED_DEFAULTS = {
    ('urlsafe_b64decode', 'padded'): VersionedDefault((3, 15), True, False),
    ('urlsafe_b64decode_into', 'padded'): VersionedDefault((3, 15), True, False),
}
MISSING_DEFAULTS = {
    ('b64decode', 'validate'),
    ('b64decode', 'ignorechars'),
    ('b64decode_into', 'validate'),
    ('b64decode_into', 'ignorechars'),
}


def module_for(name: str) -> str:
    if 'b64' in name:
        return 'base64'
    if name.startswith('murmur3_'):
        return 'murmur3'
    if name.startswith('xxh3_'):
        return 'xxhash'
    raise ValueError(f'cannot assign public API name {name!r} to a module')


def declarations() -> dict[str, list[ast.FunctionDef | ast.ClassDef]]:
    tree = ast.parse(STUB.read_text(encoding='utf-8'), filename=str(STUB))
    grouped = {module: [] for module in MODULES}
    for node in tree.body:
        if isinstance(node, (ast.FunctionDef, ast.ClassDef)) and not node.name.startswith('_'):
            grouped[module_for(node.name)].append(node)
    return grouped


def format_names(names: Iterable[str], indent: str = '    ') -> str:
    return ''.join(f'{indent}{name},\n' for name in names)


def format_string_names(names: Iterable[str], indent: str = '    ') -> str:
    return ''.join(f"{indent}'{name}',\n" for name in names)


def natural_key(name: str) -> tuple[str | int, ...]:
    return tuple(int(part) if part.isdigit() else part for part in re.split(r'(\d+)', name))


def render_runtime_module(module: str, names: list[str]) -> str:
    return (
        f'{GENERATED_HEADER}"""{MODULE_DOCSTRINGS[module]}"""\n\n'
        'from ._hashcodecs import (\n'
        f'{format_names(names)})\n\n'
        'for _public_api in (\n'
        f'{format_names(names)}):\n'
        '    _public_api.__module__ = __name__\n'
        'del _public_api\n\n'
        '__all__ = [\n'
        f'{format_string_names(names)}]\n'
    )


def render_root_module(grouped_names: dict[str, list[str]]) -> str:
    imports = ''.join(f'from .{module} import (\n{format_names(names)})\n' for module, names in grouped_names.items())
    all_names = sorted((name for names in grouped_names.values() for name in names), key=natural_key)
    return (
        f'{GENERATED_HEADER}"""Base64, MurmurHash3, and XXH3 functions with runtime SIMD dispatch."""\n\n'
        f'{imports}\n'
        '__all__ = [\n'
        f'{format_string_names(all_names)}]\n'
    )


def render_stub(import_module: str, names: list[str]) -> str:
    imports = ''.join(f'from .{import_module} import {name} as {name}\n' for name in names)
    return f'{GENERATED_HEADER}{imports}\n__all__ = [\n{format_string_names(names)}]\n'


def render_api_reference(module: str, names: list[str]) -> str:
    members = ''.join(f'        - {name}\n' for name in names)
    introduction = textwrap.fill(API_INTRODUCTIONS[module], width=119)
    return (
        f'# {API_TITLES[module]}\n\n'
        f'{introduction}\n\n'
        '::: hashcodecs._hashcodecs\n'
        '    options:\n'
        '      members:\n'
        f'{members}'
        '      show_root_heading: false\n'
    )


def function_parameters(function: ast.FunctionDef) -> list[tuple[ast.arg, ast.expr | None]]:
    if function.args.vararg or function.args.kwarg:
        raise ValueError(f'{function.name}: variadic native functions are unsupported')
    positional = [*function.args.posonlyargs, *function.args.args]
    positional_defaults = [None] * (len(positional) - len(function.args.defaults)) + list(function.args.defaults)
    return [
        *zip(positional, positional_defaults, strict=True),
        *zip(function.args.kwonlyargs, function.args.kw_defaults, strict=True),
    ]


def resolved_versioned_default(default: VersionedDefault, *, python_315: bool) -> bool:
    version = (3, 15) if python_315 else (3, 14)
    return default.after if version >= default.since else default.before


def render_default(value: ast.expr | None, function: str, argument: str, *, python_315: bool) -> str | None:
    if value is None:
        return None
    if isinstance(value, ast.Constant) and value.value is Ellipsis:
        key = (function, argument)
        if versioned := VERSIONED_DEFAULTS.get(key):
            return repr(resolved_versioned_default(versioned, python_315=python_315))
        if key in MISSING_DEFAULTS:
            return "['NOT SPECIFIED']"
        raise ValueError(f'{function}.{argument}: ellipsis default has no declared runtime behavior')
    return ast.unparse(value)


def text_signature(function: ast.FunctionDef, *, python_315: bool = False) -> str:
    positional_count = len(function.args.posonlyargs) + len(function.args.args)
    rendered = []
    for index, (argument, default) in enumerate(function_parameters(function)):
        if index == positional_count and function.args.kwonlyargs:
            rendered.append('*')
        value = render_default(default, function.name, argument.arg, python_315=python_315)
        rendered.append(argument.arg if value is None else f'{argument.arg}={value}')
    arguments = ', '.join(rendered)
    return f'{function.name}($module, /, {arguments})'


def runtime_doc(function: ast.FunctionDef, *, python_315: bool = False) -> str:
    docstring = ast.get_docstring(function, clean=True)
    if not docstring:
        raise ValueError(f'{function.name} has no API docstring')
    return f'{text_signature(function, python_315=python_315)}\n--\n\n{docstring}'


def replace_runtime_docs(
    path: Path,
    source: str,
    functions: dict[str, ast.FunctionDef],
) -> str:
    seen: set[str] = set()

    def replace(match: re.Match[str]) -> str:
        first_line = match.group('body').splitlines()[0]
        name = first_line.split('(', 1)[0]
        function = functions.get(name)
        if function is None:
            raise ValueError(f'{path}: native method {name!r} is absent from {STUB.name}')
        if name in seen:
            raise ValueError(f'{path}: native method {name!r} has duplicate documentation')
        seen.add(name)
        return f'{match.group("prefix")}{runtime_doc(function)}{match.group("suffix")}'

    rendered = METHOD_DOC_PATTERN.sub(replace, source)
    if seen != set(functions):
        raise ValueError(f'{path}: documented native methods {sorted(seen)!r} do not match {sorted(functions)!r}')
    return rendered


def render_method_file(module: str, path: Path, functions: dict[str, ast.FunctionDef]) -> str:
    source = path.read_text(encoding='utf-8')
    entries = [(match.group('name'), match.group('callback')) for match in METHOD_ENTRY_PATTERN.finditer(source)]
    method_names = [name for name, _ in entries]
    if len(method_names) != len(set(method_names)) or set(method_names) != set(functions):
        raise ValueError(f'{path}: native method names {method_names!r} do not match the typed declarations')
    callback_suffix = '' if module == 'murmur3' else '_digest'
    for name, callback in entries:
        expected_callback = f'{name}{callback_suffix}'
        if callback != expected_callback:
            raise ValueError(
                f'{path}: native method {name!r} uses callback {callback!r}, expected {expected_callback!r}'
            )
    documented_names = [
        match.group('body').splitlines()[0].split('(', 1)[0] for match in METHOD_DOC_PATTERN.finditer(source)
    ]
    if documented_names != method_names:
        raise ValueError(
            f'{path}: method table names {method_names!r} do not match documentation {documented_names!r}'
        )
    return replace_runtime_docs(path, source, functions)


def render_murmur_class_file(path: Path, classes: dict[str, ast.ClassDef]) -> str:
    source = path.read_text(encoding='utf-8')
    seen: set[str] = set()

    def replace(match: re.Match[str]) -> str:
        name = match.group('name')
        class_definition = classes.get(name)
        if class_definition is None:
            raise ValueError(f'{path}: native class {name!r} is absent from {STUB.name}')
        if name in seen:
            raise ValueError(f'{path}: native class {name!r} has duplicate documentation')
        seen.add(name)
        documentation = ast.get_docstring(class_definition, clean=True)
        if not documentation:
            raise ValueError(f'{name} has no API docstring')
        summary_line, separator, examples = documentation.partition('\n\nExamples:\n')
        summary_prefix = 'Incremental MurmurHash3 '
        summary_suffix = (
            ' hasher.\n\nArgs:\n    data: Optional initial bytes-like data.\n    seed: Initial unsigned 32-bit seed.'
        )
        if not separator or not summary_line.startswith(summary_prefix) or not summary_line.endswith(summary_suffix):
            raise ValueError(f'{name}: class docstring does not match the native MurmurHash3 template')
        summary = summary_line.removeprefix(summary_prefix).removesuffix(summary_suffix)
        rendered = f'{summary_prefix}{summary}{summary_suffix}\n\nExamples:\n{examples}'
        if rendered != documentation:
            raise ValueError(f'{name}: class docstring cannot be represented by the native MurmurHash3 template')
        return (
            f'{match.group("prefix")}{json.dumps(summary, ensure_ascii=False)},\n'
            f'    {json.dumps(examples, ensure_ascii=False)}{match.group("suffix")}'
        )

    rendered = MURMUR_CLASS_DOC_PATTERN.sub(replace, source)
    if seen != set(classes):
        raise ValueError(f'{path}: documented native classes {sorted(seen)!r} do not match {sorted(classes)!r}')
    return rendered


def rust_c_string(value: str) -> str:
    if '\0' in value:
        raise ValueError('Rust C string literals cannot contain NUL bytes')
    hashes = ''
    while f'"{hashes}' in value:
        hashes += '#'
    return f'cr{hashes}"{value}"{hashes}'


def rust_default(function: ast.FunctionDef, argument: ast.arg, default: ast.expr | None) -> str:
    if default is None:
        return 'DefaultValue::Required'
    if isinstance(default, ast.Constant):
        if default.value is Ellipsis:
            key = (function.name, argument.arg)
            if versioned := VERSIONED_DEFAULTS.get(key):
                before = str(versioned.before).lower()
                after = str(versioned.after).lower()
                return f'DefaultValue::VersionedBool {{ before: {before}, since: {versioned.since}, after: {after} }}'
            if key in MISSING_DEFAULTS:
                return 'DefaultValue::Missing'
            raise ValueError(f'{function.name}.{argument.arg}: ellipsis default has no declared runtime behavior')
        if default.value is None:
            return 'DefaultValue::None'
        if isinstance(default.value, bool):
            return f'DefaultValue::Bool({str(default.value).lower()})'
        if isinstance(default.value, int):
            return f'DefaultValue::I128({default.value})'
    raise ValueError(f'{function.name}.{argument.arg}: unsupported native default {ast.dump(default)}')


def binding_constant(name: str) -> str:
    return name.upper()


def validate_base64_callbacks(functions: list[ast.FunctionDef]) -> None:
    source = BASE64_CALLBACKS.read_text(encoding='utf-8')
    callbacks: dict[str, list[str]] = {}
    for match in CALLBACK_PATTERN.finditer(source):
        name = match.group('name')
        if name in callbacks:
            raise ValueError(f'{BASE64_CALLBACKS}: duplicate callback {name!r}')
        callbacks[name] = [
            parameter.strip() for parameter in match.group('parameters').split(',') if parameter.strip()
        ]

    expected = {function.name for function in functions}
    if set(callbacks) != expected:
        raise ValueError(
            f'{BASE64_CALLBACKS}: callbacks {sorted(callbacks)!r} do not match typed declarations {sorted(expected)!r}'
        )
    for function in functions:
        expected_parameters = [argument.arg for argument, _ in function_parameters(function)]
        if callbacks[function.name] != expected_parameters:
            raise ValueError(
                f'{BASE64_CALLBACKS}: callback {function.name!r} parameters {callbacks[function.name]!r} '
                f'do not match typed declaration {expected_parameters!r}'
            )


def render_base64_binding(function: ast.FunctionDef) -> str:
    parameters = function_parameters(function)
    defaults = [rust_default(function, argument, default) for argument, default in parameters]
    required = sum(default is None for _, default in parameters)
    if any(default is None for _, default in parameters[required:]):
        raise ValueError(f'{function.name}: required parameters must precede optional parameters')
    max_positional = len(function.args.posonlyargs) + len(function.args.args)
    constant = binding_constant(function.name)
    parameter_names = ', '.join(f'c"{argument.arg}"' for argument, _ in parameters)
    documentation = runtime_doc(function)
    python_315_documentation = runtime_doc(function, python_315=True)
    versioned_documentation = (
        f'Some({rust_c_string(python_315_documentation)})' if python_315_documentation != documentation else 'None'
    )
    argument_types = ', '.join('Argument' for _ in parameters)
    arguments = ',\n                '.join(
        f'Argument::new(values[{index}], {default})' for index, default in enumerate(defaults)
    )
    return f"""binding! {{
    {constant}: {len(parameters)} {{
        name: c"{function.name}",
        callback: {function.name},
        parameters: [{parameter_names}],
        max_positional: {max_positional},
        required: {required},
        documentation: {rust_c_string(documentation)},
        python_315_documentation: {versioned_documentation},
    }}
}}

#[inline(always)]
pub(super) unsafe fn {function.name}(
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
    operation: impl FnOnce(Python<'_>, {argument_types}) -> *mut ffi::PyObject,
) -> *mut ffi::PyObject {{
    unsafe {{
        {constant}.invoke(args, nargs, keywords, |py, values| {{
            operation(
                py,
                {arguments},
            )
        }})
    }}
}}
"""


def render_base64_schema(functions: list[ast.FunctionDef]) -> str:
    validate_base64_callbacks(functions)
    bindings = '\n'.join(render_base64_binding(function) for function in functions)
    registrations = '\n'.join(
        f'    unsafe {{ {binding_constant(function.name)}.register(methods, &mut method_count, version) }};'
        for function in functions
    )
    return f"""{RUST_GENERATED_HEADER}
pub(super) const BINDING_COUNT: usize = {len(functions)};

{bindings}
pub(super) unsafe fn register_all(methods: *mut ffi::PyMethodDef, version: (u8, u8)) {{
    let mut method_count = 0;
{registrations}
    assert_eq!(
        method_count, BINDING_COUNT,
        "Base64 method table must match its generated schema",
    );
}}
"""


def generated_files() -> dict[Path, str]:
    grouped = declarations()
    grouped_names = {
        module: sorted((node.name for node in nodes), key=natural_key) for module, nodes in grouped.items()
    }
    outputs: dict[Path, str] = {}
    for module, names in grouped_names.items():
        outputs[ROOT / 'hashcodecs' / f'{module}.py'] = render_runtime_module(module, names)
        outputs[ROOT / 'hashcodecs' / f'{module}.pyi'] = render_stub('_hashcodecs', names)
        outputs[ROOT / 'docs' / 'api' / API_FILENAMES[module]] = render_api_reference(module, names)
    outputs[ROOT / 'hashcodecs' / '__init__.py'] = render_root_module(grouped_names)
    outputs[ROOT / 'hashcodecs' / '__init__.pyi'] = render_stub(
        '_hashcodecs', sorted((name for names in grouped_names.values() for name in names), key=natural_key)
    )

    for module, path in METHOD_FILES.items():
        functions = {node.name: node for node in grouped[module] if isinstance(node, ast.FunctionDef)}
        outputs[path] = render_method_file(module, path, functions)

    murmur_classes = {node.name: node for node in grouped['murmur3'] if isinstance(node, ast.ClassDef)}
    outputs[MURMUR_INCREMENTAL] = render_murmur_class_file(MURMUR_INCREMENTAL, murmur_classes)

    base64_functions = [node for node in grouped['base64'] if isinstance(node, ast.FunctionDef)]
    outputs[BASE64_GENERATED_SCHEMA] = render_base64_schema(base64_functions)
    return outputs


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true', help='fail when generated files are stale')
    args = parser.parse_args()

    stale = []
    for path, content in generated_files().items():
        if path.exists():
            current = path.read_text(encoding='utf-8')
            if path in METHOD_FILES.values():
                current = re.sub(r'"###\s*\.as_ptr\(\)', '"###.as_ptr()', current)
                content = re.sub(r'"###\s*\.as_ptr\(\)', '"###.as_ptr()', content)
            if current == content:
                continue
        stale.append(path.relative_to(ROOT))
        if not args.check:
            path.write_text(content, encoding='utf-8', newline='')

    if stale and args.check:
        print('Generated API metadata is stale:', file=sys.stderr)
        for path in stale:
            print(f'  {path}', file=sys.stderr)
        print('Run: python tools/generate_api_metadata.py', file=sys.stderr)
        return 1
    if stale:
        print(f'Updated {len(stale)} generated API files.')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
