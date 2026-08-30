import json
import platform
import runpy
from pathlib import Path

import pytest
from packaging.tags import Tag, mac_platforms

CustomBuildHook = runpy.run_path(str(Path(__file__).parents[1] / 'hatch_build.py'))['CustomBuildHook']


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
