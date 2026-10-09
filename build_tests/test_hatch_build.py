import json
import platform
import runpy
import subprocess
import sys
from pathlib import Path
from typing import Any

import pytest
from packaging.tags import Tag, mac_platforms

CustomBuildHook = runpy.run_path(str(Path(__file__).parents[1] / 'hatch_build.py'))['CustomBuildHook']


@pytest.mark.parametrize('base_executable', ['python', None])
def test_isolated_builds_reuse_interpreter_and_dependency_cache(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path, base_executable: str | None
) -> None:
    monkeypatch.delenv('CARGO_TARGET_DIR', raising=False)
    monkeypatch.delenv('CARGO_LLVM_COV_TARGET_DIR', raising=False)
    globals_ = CustomBuildHook.initialize.__globals__
    monkeypatch.setitem(globals_, 'sys_tags', lambda: iter([Tag('cp314', 'cp314', 'win_amd64')]))
    interpreter = tmp_path / 'python'
    monkeypatch.setattr(sys, '_base_executable', str(interpreter) if base_executable else None)
    environments = []
    artifact = tmp_path / 'hashcodecs.dll'
    messages = json.dumps(
        {
            'reason': 'compiler-artifact',
            'target': {'name': 'hashcodecs', 'crate_types': ['cdylib']},
            'filenames': [str(artifact)],
        }
    )

    def run(command: list[str], **kwargs: Any) -> subprocess.CompletedProcess[str]:
        assert command[:2] == ['cargo', 'rustc']
        assert command[command.index('--crate-type') + 1] == 'cdylib'
        assert '--lib' in command
        assert '--release' in command
        environments.append(kwargs['env'])
        return subprocess.CompletedProcess(command, 0, stdout=messages)

    monkeypatch.setattr(subprocess, 'run', run)
    monkeypatch.setattr(CustomBuildHook, '_wheel_tag', staticmethod(lambda _extension: 'cp314-cp314-win_amd64'))
    hook = CustomBuildHook(str(tmp_path), {}, None, None, str(tmp_path / 'dist'), 'wheel')
    for name in ['isolated-first', 'isolated-second']:
        monkeypatch.setattr(
            sys, 'executable', str(tmp_path / name / 'python') if base_executable else str(interpreter)
        )
        build_data: dict[str, Any] = {}
        hook.initialize('standard', build_data)
        assert build_data['pure_python'] is False
        assert str(artifact) in build_data['force_include']

    assert environments[0]['PYO3_PYTHON'] == environments[1]['PYO3_PYTHON'] == str(interpreter.resolve())
    assert (
        environments[0]['CARGO_TARGET_DIR']
        == environments[1]['CARGO_TARGET_DIR']
        == str(tmp_path / 'target' / 'hatch' / 'cp314-cp314-win_amd64')
    )


def test_python_abis_use_separate_output_directories(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> None:
    monkeypatch.delenv('CARGO_TARGET_DIR', raising=False)
    monkeypatch.delenv('CARGO_LLVM_COV_TARGET_DIR', raising=False)
    directories = []

    def run(command: list[str], **kwargs: Any) -> subprocess.CompletedProcess[str]:
        directory = Path(kwargs['env']['CARGO_TARGET_DIR'])
        directories.append(directory)
        return subprocess.CompletedProcess(
            command,
            0,
            stdout=json.dumps(
                {
                    'reason': 'compiler-artifact',
                    'target': {'name': 'hashcodecs', 'crate_types': ['cdylib']},
                    'filenames': [str(directory / 'release' / 'hashcodecs.dll')],
                }
            ),
        )

    monkeypatch.setattr(subprocess, 'run', run)
    hook = CustomBuildHook(str(tmp_path), {}, None, None, str(tmp_path / 'dist'), 'wheel')
    for tag in [Tag('cp314', 'cp314', 'win_amd64'), Tag('cp314', 'cp314t', 'win_amd64')]:
        monkeypatch.setitem(CustomBuildHook.initialize.__globals__, 'sys_tags', lambda tag=tag: iter([tag]))
        build_data: dict[str, Any] = {}
        hook.initialize('standard', build_data)
        extension = directories[-1] / 'release' / 'hashcodecs.dll'
        assert str(extension) in build_data['force_include']

    assert directories[0] != directories[1]


@pytest.mark.parametrize(
    ('target', 'coverage_target', 'expected'),
    [
        ('custom-target', 'coverage-target', 'custom-target'),
        (None, 'coverage-target', 'coverage-target'),
        ('', 'coverage-target', 'coverage-target'),
        ('absolute', None, 'absolute'),
    ],
)
def test_configured_target_directory_is_preserved(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path, target: str | None, coverage_target: str | None, expected: str
) -> None:
    if target == 'absolute':
        target = str(tmp_path / target)
    for name, value in [('CARGO_TARGET_DIR', target), ('CARGO_LLVM_COV_TARGET_DIR', coverage_target)]:
        if value is None:
            monkeypatch.delenv(name, raising=False)
        else:
            monkeypatch.setenv(name, value)

    def run(command: list[str], **kwargs: Any) -> subprocess.CompletedProcess[str]:
        assert kwargs['env']['CARGO_TARGET_DIR'] == str(tmp_path / expected)
        return subprocess.CompletedProcess(command, 0, stdout='')

    monkeypatch.setattr(subprocess, 'run', run)
    monkeypatch.setattr(
        CustomBuildHook, '_cdylib_artifact', staticmethod(lambda _messages: tmp_path / 'hashcodecs.so')
    )
    monkeypatch.setattr(CustomBuildHook, '_wheel_tag', staticmethod(lambda _extension: 'cp314-cp314-win_amd64'))
    hook = CustomBuildHook(str(tmp_path), {}, None, None, str(tmp_path / 'dist'), 'wheel')
    hook.initialize('standard', {})


def cargo_message(rendered: str | None) -> str:
    return json.dumps({'reason': 'compiler-message', 'message': {'rendered': rendered}})


def test_failed_build_prints_rendered_cargo_diagnostics(capsys: pytest.CaptureFixture[str]) -> None:
    messages = '\n'.join(
        [cargo_message('first diagnostic\n'), cargo_message(None), cargo_message('second diagnostic\n')]
    )

    CustomBuildHook._print_cargo_diagnostics(messages)

    assert capsys.readouterr().err == 'first diagnostic\nsecond diagnostic\n'


def test_failed_build_falls_back_to_unparsed_cargo_output(capsys: pytest.CaptureFixture[str]) -> None:
    messages = 'cargo failed before emitting JSON\n'

    CustomBuildHook._print_cargo_diagnostics(messages)

    assert capsys.readouterr().err == messages


@pytest.mark.parametrize(
    ('load_command', 'expected'),
    [
        ('cmd LC_BUILD_VERSION\n      minos 11.0\n        sdk 15.4', (11, 0)),
        ('cmd LC_VERSION_MIN_MACOSX\n    version 10.12\n        sdk 14.0', (10, 12)),
    ],
)
def test_macos_deployment_target_is_read_from_macho(load_command: str, expected: tuple[int, int]) -> None:
    assert CustomBuildHook._parse_macos_deployment_target(load_command) == expected


def test_missing_macos_deployment_target_is_rejected() -> None:
    with pytest.raises(RuntimeError, match='otool did not report'):
        CustomBuildHook._parse_macos_deployment_target('cmd LC_SEGMENT_64\ncmdsize 72')


def test_macos_wheel_tag_uses_binary_deployment_target(monkeypatch: pytest.MonkeyPatch) -> None:
    globals_ = CustomBuildHook._wheel_tag.__globals__
    monkeypatch.setitem(globals_, 'sys_tags', lambda: iter([Tag('cp312', 'cp312', 'macosx_15_0_arm64')]))
    monkeypatch.setattr(platform, 'system', lambda: 'Darwin')
    monkeypatch.setattr(platform, 'machine', lambda: 'arm64')
    monkeypatch.setattr(CustomBuildHook, '_macos_deployment_target', lambda _extension: (11, 0))

    assert CustomBuildHook._wheel_tag(Path('hashcodecs.so')) == 'cp312-cp312-macosx_11_0_arm64'
    for host_major in range(11, 16):
        assert 'macosx_11_0_arm64' in mac_platforms(version=(host_major, 0), arch='arm64')
