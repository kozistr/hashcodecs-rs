import json
import runpy
from pathlib import Path

import pytest

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
