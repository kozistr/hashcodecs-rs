"""Hatchling hook that packages the in-tree PyO3 extension."""

from __future__ import annotations

import json
import os
import platform
import subprocess
import sys
from pathlib import Path
from typing import Any

from hatchling.builders.hooks.plugin.interface import BuildHookInterface
from packaging.tags import Tag, mac_platforms, sys_tags


class CustomBuildHook(BuildHookInterface[Any]):
    """Build the CPython extension wheel."""

    def initialize(self, version: str, build_data: dict[str, Any]) -> None:
        if self.target_name != 'wheel':
            return

        root = Path(self.root)
        build_tag = str(next(sys_tags()))
        env = os.environ.copy()
        configured_target_dir = env.get('CARGO_TARGET_DIR') or env.get('CARGO_LLVM_COV_TARGET_DIR')
        if configured_target_dir:
            target_dir = Path(configured_target_dir)
            if not target_dir.is_absolute():
                target_dir = root / target_dir
        else:
            target_dir = root / 'target' / 'hatch' / build_tag
        env['CARGO_TARGET_DIR'] = str(target_dir)
        env['PYO3_PYTHON'] = sys.executable
        result = subprocess.run(
            [
                'cargo',
                'build',
                '--locked',
                '--manifest-path',
                str(root / 'Cargo.toml'),
                '--release',
                '--features',
                'extension-module',
                '--message-format=json-render-diagnostics',
            ],
            check=False,
            cwd=root,
            env=env,
            stdout=subprocess.PIPE,
            text=True,
        )
        if result.returncode:
            self._print_cargo_diagnostics(result.stdout)
        result.check_returncode()

        extension = self._cdylib_artifact(result.stdout)
        build_data['force_include'] = {str(extension): f'hashcodecs/_hashcodecs{self._extension_suffix()}'}
        build_data['pure_python'] = False
        build_data['tag'] = self._wheel_tag(extension)

    @staticmethod
    def _print_cargo_diagnostics(messages: str) -> None:
        rendered_any = False
        for line in messages.splitlines():
            try:
                message = json.loads(line)
            except json.JSONDecodeError:
                continue
            rendered = message.get('message', {}).get('rendered')
            if rendered:
                sys.stderr.write(rendered)
                rendered_any = True
        if not rendered_any:
            sys.stderr.write(messages)

    @staticmethod
    def _cdylib_artifact(messages: str) -> Path:
        suffixes = {'.dll', '.dylib', '.so'}
        for line in messages.splitlines():
            try:
                message = json.loads(line)
            except json.JSONDecodeError:
                continue
            if (
                message.get('reason') == 'compiler-artifact'
                and message.get('target', {}).get('name') == 'hashcodecs'
                and 'cdylib' in message.get('target', {}).get('crate_types', [])
            ):
                for filename in message.get('filenames', []):
                    artifact = Path(filename)
                    if artifact.suffix in suffixes:
                        return artifact
        raise RuntimeError('Cargo did not report the hashcodecs cdylib artifact')

    @staticmethod
    def _extension_suffix() -> str:
        return '.pyd' if platform.system() == 'Windows' else '.so'

    @staticmethod
    def _wheel_tag(extension: Path) -> str:
        tag = next(sys_tags())
        if platform.system() != 'Darwin':
            return str(tag)

        deployment_target = CustomBuildHook._macos_deployment_target(extension)
        platform_tag = next(mac_platforms(version=deployment_target, arch=platform.machine()))
        return str(Tag(tag.interpreter, tag.abi, platform_tag))

    @staticmethod
    def _macos_deployment_target(extension: Path) -> tuple[int, int]:
        result = subprocess.run(
            ['otool', '-l', str(extension)],
            check=True,
            stdout=subprocess.PIPE,
            text=True,
        )
        return CustomBuildHook._parse_macos_deployment_target(result.stdout)

    @staticmethod
    def _parse_macos_deployment_target(load_commands: str) -> tuple[int, int]:
        command = ''
        version_field = {'LC_BUILD_VERSION': 'minos', 'LC_VERSION_MIN_MACOSX': 'version'}
        for line in load_commands.splitlines():
            fields = line.split()
            if len(fields) != 2:
                continue
            name, value = fields
            if name == 'cmd':
                command = value
                continue
            if name != version_field.get(command):
                continue
            parts = value.split('.')
            if len(parts) >= 2 and parts[0].isdigit() and parts[1].isdigit():
                return int(parts[0]), int(parts[1])
        raise RuntimeError('otool did not report a macOS deployment target')
