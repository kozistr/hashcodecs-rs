"""Render the checked-in benchmark measurements as dependency-free SVG charts."""

from __future__ import annotations

import csv
import html
import math
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / 'docs' / 'benchmarks'

COLORS = {
    'hashcodecs': '#007f73',
    'hashcodecs list': '#007f73',
    'hashcodecs packed': '#a86f00',
    'base64': '#356ac3',
    'base64 turbo': '#a86f00',
    'murmur3': '#356ac3',
    'murmurs': '#8f4a73',
    'fastmurmur3': '#a86f00',
    'mm3h': '#637083',
    'mmh3': '#d55e00',
    'upstream C': '#d55e00',
    'xxhash': '#d55e00',
    'hashcodecs loop': '#356ac3',
    'pybase64': '#8f4a73',
    'CPython': '#637083',
    'returned bytes': '#356ac3',
    'reusable bytearray': '#007f73',
    'returned encode': '#356ac3',
    'reusable encode': '#007f73',
    'returned decode': '#8f4a73',
    'reusable decode': '#d55e00',
    'encode': '#007f73',
    'decode': '#356ac3',
    'full view': '#007f73',
    'nonzero offset': '#d55e00',
}
FALLBACK_COLORS = ('#007f73', '#d55e00', '#356ac3', '#8f4a73', '#a86f00', '#637083')


@dataclass(frozen=True)
class Panel:
    title: str
    categories: tuple[str, ...]
    series: tuple[tuple[str, tuple[float | None, ...]], ...]


@dataclass(frozen=True)
class Chart:
    filename: str
    title: str
    panels: tuple[Panel, ...]


def panel(title: str, categories: list[str], **series: list[float | None]) -> Panel:
    return Panel(
        title,
        tuple(categories),
        tuple((name.replace('_', ' '), tuple(values)) for name, values in series.items()),
    )


SIZES = ['1 KiB', '4 KiB', '1 MiB', '8 MiB']

CHARTS = (
    Chart(
        'base64-rust.svg',
        'Rust Base64 throughput',
        (
            panel(
                'Standard encode',
                SIZES,
                hashcodecs=[25.19, 35.02, 38.27, 29.24],
                base64=[4.77, 5.40, 4.83, 4.06],
                base64_turbo=[25.11, 33.92, 37.81, 30.99],
            ),
            panel(
                'Standard decode',
                SIZES,
                hashcodecs=[17.92, 25.22, 28.31, 18.79],
                base64=[3.76, 3.98, 3.79, 3.56],
                base64_turbo=[17.37, 23.28, 20.50, 16.87],
            ),
            panel(
                'URL-safe encode',
                SIZES,
                hashcodecs=[25.87, 34.95, 39.68, 29.86],
                base64=[4.79, 5.41, 4.82, 4.08],
                base64_turbo=[25.41, 33.95, 38.29, 31.16],
            ),
            panel(
                'URL-safe decode',
                SIZES,
                hashcodecs=[17.39, 23.84, 27.01, 18.89],
                base64=[3.76, 4.03, 3.81, 3.57],
                base64_turbo=[17.37, 23.16, 20.87, 17.10],
            ),
        ),
    ),
    Chart(
        'murmur3-rust.svg',
        'Rust MurmurHash3 throughput',
        (
            panel(
                'x86 32-bit',
                SIZES,
                hashcodecs=[3.86, 3.77, 3.74, 3.77],
                murmur3=[2.25, 2.26, 2.25, 2.26],
                murmurs=[3.67, 3.51, 3.46, 3.51],
                fastmurmur3=[None] * 4,
                mm3h=[3.93, 3.74, 3.79, 3.74],
            ),
            panel(
                'x86 128-bit',
                SIZES,
                hashcodecs=[7.82, 8.11, 8.38, 8.37],
                murmur3=[4.14, 4.37, 4.35, 4.37],
                murmurs=[7.28, 7.41, 7.57, 7.38],
                fastmurmur3=[None] * 4,
                mm3h=[None] * 4,
            ),
            panel(
                'x64 128-bit',
                SIZES,
                hashcodecs=[9.17, 9.29, 9.31, 9.48],
                murmur3=[5.60, 6.01, 6.04, 5.95],
                murmurs=[8.18, 8.11, 8.22, 8.19],
                fastmurmur3=[8.70, 8.66, 8.68, 8.68],
                mm3h=[7.99, 8.01, 8.19, 7.91],
            ),
        ),
    ),
    Chart(
        'xxh3-rust.svg',
        'Rust XXH3 throughput',
        (
            panel(
                'XXH3-64 one-shot',
                ['64 B', *SIZES],
                hashcodecs=[33.90, 43.23, 62.09, 91.56, 51.30],
                upstream_C=[28.42, 26.27, 39.63, 48.79, 38.62],
            ),
            panel(
                'XXH3-128 one-shot',
                ['64 B', *SIZES],
                hashcodecs=[11.23, 41.21, 69.82, 92.33, 51.28],
                upstream_C=[8.40, 21.85, 37.36, 48.45, 38.61],
            ),
            panel(
                'XXH3-64 batch (32 items)',
                ['64 B', '1 KiB', '4 KiB', '1 MiB'],
                hashcodecs=[28.72, 78.23, 90.86, 32.86],
                upstream_C=[21.19, 26.12, 38.86, 17.98],
            ),
            panel(
                'XXH3-128 batch (32 items)',
                ['64 B', '1 KiB', '4 KiB', '1 MiB'],
                hashcodecs=[10.76, 65.65, 86.06, 32.69],
                upstream_C=[8.28, 21.84, 37.01, 17.94],
            ),
        ),
    ),
    Chart(
        'xxh3-rust-batch-remainders.svg',
        'Rust XXH3 batch remainder throughput',
        (
            panel(
                'XXH3-64 batch (2 items)',
                ['1 KiB', '4 KiB', '1 MiB'],
                hashcodecs=[55.83, 82.69, 95.29],
                upstream_C=[25.82, 38.78, 48.17],
            ),
            panel(
                'XXH3-128 batch (2 items)',
                ['1 KiB', '4 KiB', '1 MiB'],
                hashcodecs=[52.14, 78.58, 94.99],
                upstream_C=[21.10, 36.63, 48.25],
            ),
            panel(
                'XXH3-64 batch (3 items)',
                ['1 KiB', '4 KiB', '1 MiB'],
                hashcodecs=[64.68, 86.93, 91.18],
                upstream_C=[26.05, 39.30, 46.87],
            ),
            panel(
                'XXH3-128 batch (3 items)',
                ['1 KiB', '4 KiB', '1 MiB'],
                hashcodecs=[55.90, 82.34, 90.77],
                upstream_C=[21.44, 36.88, 47.47],
            ),
        ),
    ),
    Chart(
        'base64-python.svg',
        'Python Base64 throughput',
        (
            panel(
                'Standard encode',
                SIZES,
                hashcodecs=[11.70, 24.40, 3.89, 4.12],
                CPython=[0.44, 0.46, 0.40, 0.42],
                pybase64=[5.02, 13.11, 2.68, 2.91],
            ),
            panel(
                'Standard decode',
                SIZES,
                hashcodecs=[8.92, 18.23, 4.43, 4.94],
                CPython=[0.93, 1.08, 0.84, 0.90],
                pybase64=[3.09, 8.01, 3.26, 3.57],
            ),
            panel(
                'URL-safe encode',
                SIZES,
                hashcodecs=[10.49, 22.95, 3.88, 4.10],
                CPython=[0.37, 0.41, 0.33, 0.34],
                pybase64=[0.96, 1.19, 0.84, 0.84],
            ),
            panel(
                'URL-safe decode',
                SIZES,
                hashcodecs=[7.27, 13.94, 4.83, 5.23],
                CPython=[0.47, 0.74, 0.60, 0.59],
                pybase64=[1.13, 1.56, 1.34, 1.38],
            ),
        ),
    ),
    Chart(
        'base64-python-lenient.svg',
        'Lenient Python Base64 throughput',
        (
            panel(
                'MIME whitespace',
                SIZES,
                returned_bytes=[2.48, 3.02, 1.95, 1.93],
                reusable_bytearray=[2.62, 3.09, 3.26, 2.89],
                CPython=[0.85, 0.94, 0.85, 0.84],
                pybase64=[2.46, 3.76, 2.67, 2.62],
            ),
            panel(
                'Ignored non-alphabet bytes',
                SIZES,
                returned_bytes=[1.66, 2.06, 1.60, 1.61],
                reusable_bytearray=[1.86, 2.19, 2.30, 2.18],
                CPython=[0.85, 0.94, 0.83, 0.84],
                pybase64=[2.55, 3.96, 2.62, 2.74],
            ),
        ),
    ),
    Chart(
        'murmur3-python.svg',
        'Python MurmurHash3 throughput',
        tuple(
            panel(title, SIZES, hashcodecs=ours, mmh3=upstream)
            for title, ours, upstream in (
                ('x86 32-bit one-shot', [3.53, 3.84, 3.98, 3.97], [3.44, 3.72, 3.84, 3.83]),
                ('x86 32-bit incremental', [2.83, 3.63, 4.00, 3.99], [2.86, 3.48, 3.84, 3.84]),
                ('x86 128-bit one-shot', [6.43, 8.48, 9.38, 9.33], [6.69, 8.22, 8.89, 8.86]),
                ('x86 128-bit incremental', [4.45, 7.32, 9.43, 9.46], [0.69, 0.78, 0.80, 0.81]),
                ('x64 128-bit one-shot', [7.01, 9.00, 10.04, 10.04], [7.79, 9.48, 10.26, 10.26]),
                ('x64 128-bit incremental', [4.77, 7.92, 10.10, 10.11], [5.32, 8.05, 9.36, 8.20]),
            )
        ),
    ),
    Chart(
        'xxh3-python.svg',
        'Python XXH3 throughput',
        (
            panel(
                'XXH3-64 one-shot', SIZES, hashcodecs=[17.44, 44.77, 91.51, 50.53], xxhash=[13.45, 29.13, 48.08, 38.62]
            ),
            panel(
                'XXH3-128 one-shot', SIZES, hashcodecs=[14.72, 39.88, 91.00, 50.60], xxhash=[9.26, 23.44, 48.46, 38.62]
            ),
            panel(
                'XXH3-64 batch (32 items)',
                ['64 B', '1 KiB', '4 KiB', '1 MiB'],
                hashcodecs_list=[5.27, 42.76, 73.22, 37.24],
                hashcodecs_packed=[13.35, 62.76, 83.99, 37.36],
                xxhash=[2.05, 14.68, 30.39, 17.70],
            ),
            panel(
                'XXH3-128 batch (32 items)',
                ['64 B', '1 KiB', '4 KiB', '1 MiB'],
                hashcodecs_list=[2.95, 30.30, 61.11, 37.20],
                hashcodecs_packed=[7.54, 55.71, 80.41, 37.54],
                xxhash=[1.02, 9.82, 24.06, 17.82],
            ),
        ),
    ),
    Chart(
        'base64-python-reusable.svg',
        'Reusable Python Base64 buffers',
        (
            panel('Standard encode', SIZES, hashcodecs=[13.69, 26.58, 39.96, 30.29]),
            panel('Standard decode', SIZES, hashcodecs=[10.21, 19.54, 29.44, 18.91]),
            panel('URL-safe encode', SIZES, hashcodecs=[12.97, 25.80, 40.06, 30.25]),
            panel('URL-safe decode', SIZES, hashcodecs=[8.75, 15.33, 20.81, 17.46]),
        ),
    ),
    Chart(
        'base64-python-memoryview.svg',
        'Python Base64 memoryview inputs',
        (
            panel(
                'Encode, returned bytes',
                SIZES,
                full_view=[6.24, 19.96, 3.88, 4.71],
                nonzero_offset=[8.11, 18.84, 3.59, 4.05],
            ),
            panel(
                'Encode, reusable bytearray',
                SIZES,
                full_view=[7.16, 22.76, 40.06, 30.37],
                nonzero_offset=[8.32, 17.59, 22.60, 11.18],
            ),
            panel(
                'Decode, returned bytes',
                SIZES,
                full_view=[5.60, 15.62, 4.48, 4.11],
                nonzero_offset=[7.16, 14.90, 4.11, 4.95],
            ),
            panel(
                'Decode, reusable bytearray',
                SIZES,
                full_view=[6.21, 17.20, 29.38, 19.51],
                nonzero_offset=[6.78, 13.74, 15.15, 8.19],
            ),
        ),
    ),
    Chart(
        'base64-python-batch.svg',
        'Python Base64 batch throughput',
        tuple(
            panel(
                title, ['8', '64', '1,024'], hashcodecs=ours, hashcodecs_loop=loop, pybase64=pybase64, CPython=cpython
            )
            for title, ours, loop, pybase64, cpython in (
                ('16 B encode', [0.78, 1.22, 1.20], [0.30, 0.33, 0.33], [0.12, 0.13, 0.13], [0.18, 0.19, 0.19]),
                ('16 B decode', [0.51, 0.66, 0.66], [0.22, 0.24, 0.24], [0.08, 0.08, 0.08], [0.12, 0.12, 0.13]),
                ('256 B encode', [9.54, 11.65, 10.46], [4.33, 4.41, 4.34], [1.86, 1.82, 1.87], [0.38, 0.39, 0.38]),
                ('256 B decode', [6.59, 7.67, 7.05], [3.26, 3.39, 3.27], [1.17, 1.22, 1.23], [0.74, 0.75, 0.72]),
                ('4 KiB encode', [24.82, 17.96, 1.98], [20.55, 15.76, 3.77], [12.23, 8.99, 3.28], [0.46, 0.45, 0.43]),
                ('4 KiB decode', [20.93, 20.14, 7.34], [16.03, 15.50, 12.75], [7.74, 7.62, 7.17], [1.07, 1.06, 1.11]),
            )
        ),
    ),
    Chart(
        'base64-python-batch-reusable.svg',
        'Reusable Python Base64 batch buffers',
        tuple(
            panel(title, ['8', '64', '1,024'], encode=encode, decode=decode)
            for title, encode, decode in (
                ('16 B items', [0.43, 0.65, 0.64], [0.34, 0.48, 0.50]),
                ('256 B items', [4.79, 6.55, 6.19], [4.85, 6.36, 6.11]),
                ('4 KiB items', [27.73, 27.95, 17.73], [21.32, 22.69, 17.17]),
            )
        ),
    ),
    Chart(
        'base64-python-batch-large.svg',
        'Large Python Base64 batches',
        (
            panel(
                '1 MiB items',
                ['1', '2', '4', '8', '16', '32'],
                returned_encode=[3.04, 3.11, 3.14, 3.10, 2.91, 2.80],
                reusable_encode=[39.13, 20.94, 20.16, 19.49, 13.32, 11.16],
                returned_decode=[4.06, 3.89, 3.86, 3.80, 3.52, 3.37],
                reusable_decode=[29.10, 21.57, 21.07, 19.16, 12.20, 10.60],
            ),
        ),
    ),
    Chart(
        'base64-python-mutable.svg',
        'Mutable Python Base64 inputs',
        (
            panel(
                'Encode',
                SIZES,
                returned_bytes=[10.40, 22.98, 3.56, 4.72],
                reusable_bytearray=[13.49, 26.64, 39.97, 30.10],
            ),
            panel(
                'Decode',
                SIZES,
                returned_bytes=[8.67, 17.97, 4.12, 4.14],
                reusable_bytearray=[10.16, 19.57, 29.42, 19.10],
            ),
        ),
    ),
    Chart(
        'murmur3-python-mutable.svg',
        'Mutable Python MurmurHash3 inputs',
        tuple(
            panel(title, SIZES, hashcodecs=values)
            for title, values in (
                ('x86 32-bit one-shot', [3.52, 3.85, 4.00, 3.99]),
                ('x86 32-bit incremental', [2.84, 3.62, 3.98, 3.97]),
                ('x86 128-bit one-shot', [6.00, 8.30, 9.47, 9.48]),
                ('x86 128-bit incremental', [4.40, 7.24, 9.36, 9.34]),
                ('x64 128-bit one-shot', [6.81, 9.05, 10.14, 10.06]),
                ('x64 128-bit incremental', [4.75, 7.91, 10.08, 10.05]),
            )
        ),
    ),
)


def esc(value: object) -> str:
    return html.escape(str(value), quote=True)


def nice_max(value: float) -> float:
    rough = value / 4
    exponent = 10 ** math.floor(math.log10(rough)) if rough else 1
    step = next(candidate * exponent for candidate in (1, 2, 5, 10) if candidate * exponent >= rough)
    return math.ceil(value / step) * step


def color(name: str, index: int) -> str:
    return COLORS.get(name, FALLBACK_COLORS[index % len(FALLBACK_COLORS)])


OFFICIAL_SERIES = {
    'base64-rust.svg': 'base64',
    'murmur3-rust.svg': 'murmur3',
    'xxh3-rust.svg': 'upstream C',
    'xxh3-rust-batch-remainders.svg': 'upstream C',
    'base64-python.svg': 'CPython',
    'base64-python-lenient.svg': 'CPython',
    'murmur3-python.svg': 'mmh3',
    'xxh3-python.svg': 'xxhash',
    'base64-python-batch.svg': 'CPython',
}


def render(chart: Chart) -> str:
    width = 1280
    columns = 1 if len(chart.panels) == 1 else 2
    panel_width = 1180 if columns == 1 else 570
    panel_height = 330
    rows = math.ceil(len(chart.panels) / columns)
    height = 100 + rows * panel_height + 30
    chunks = [
        (
            f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" '
            f'viewBox="0 0 {width} {height}" role="img" aria-labelledby="title desc">'
        ),
        f'<title id="title">{esc(chart.title)}</title>',
        f'<desc id="desc">{esc(chart.title)} in GiB/s. Higher is better.</desc>',
        '<rect width="100%" height="100%" fill="#ffffff"/>',
        (
            '<text x="50" y="48" fill="#172033" font-family="Segoe UI,Arial,sans-serif" '
            f'font-size="28" font-weight="700">{esc(chart.title)}</text>'
        ),
        (
            '<text x="50" y="74" fill="#637083" font-family="Segoe UI,Arial,sans-serif" '
            'font-size="14">Throughput (GiB/s), higher is better</text>'
        ),
    ]

    for panel_index, spec in enumerate(chart.panels):
        column = panel_index % columns
        row = panel_index // columns
        origin_x = 50 + column * 610
        origin_y = 100 + row * panel_height
        plot_x = origin_x + 58
        plot_y = origin_y + 70
        plot_width = panel_width - 78
        plot_height = 210
        available = [value for _, values in spec.series for value in values if value is not None]
        axis_max = nice_max(max(available))

        chunks.extend(
            (
                (
                    f'<text x="{origin_x}" y="{origin_y + 24}" fill="#172033" '
                    'font-family="Segoe UI,Arial,sans-serif" '
                    f'font-size="18" font-weight="600">{esc(spec.title)}</text>'
                ),
                (
                    f'<line x1="{plot_x}" y1="{plot_y + plot_height}" '
                    f'x2="{plot_x + plot_width}" y2="{plot_y + plot_height}" stroke="#9aa5b4"/>'
                ),
            )
        )

        legend_x = origin_x
        for series_index, (name, _) in enumerate(spec.series):
            series_color = color(name, series_index)
            chunks.extend(
                (
                    (
                        f'<line x1="{legend_x}" y1="{origin_y + 48}" x2="{legend_x + 20}" '
                        f'y2="{origin_y + 48}" stroke="{series_color}" stroke-width="3"/>'
                    ),
                    (
                        f'<text x="{legend_x + 26}" y="{origin_y + 53}" fill="#465263" '
                        'font-family="Segoe UI,Arial,sans-serif" '
                        f'font-size="12">{esc(name)}</text>'
                    ),
                )
            )
            legend_x += 34 + len(name) * 7

        for tick in range(5):
            value = axis_max * tick / 4
            y = plot_y + plot_height - plot_height * tick / 4
            chunks.extend(
                (
                    (f'<line x1="{plot_x}" y1="{y:.1f}" x2="{plot_x + plot_width}" y2="{y:.1f}" stroke="#e1e6ed"/>'),
                    (
                        f'<text x="{plot_x - 10}" y="{y + 4:.1f}" text-anchor="end" fill="#637083" '
                        f'font-family="Segoe UI,Arial,sans-serif" font-size="11">{value:g}</text>'
                    ),
                )
            )

        x_step = plot_width / max(len(spec.categories) - 1, 1)
        for category_index, category in enumerate(spec.categories):
            x = plot_x + category_index * x_step
            chunks.append(
                f'<text x="{x:.1f}" y="{plot_y + plot_height + 24}" text-anchor="middle" '
                'fill="#465263" font-family="Segoe UI,Arial,sans-serif" '
                f'font-size="12">{esc(category)}</text>'
            )

        label_boxes: list[tuple[float, float, float, float]] = []
        for series_index, (name, values) in enumerate(spec.series):
            series_color = color(name, series_index)
            official = OFFICIAL_SERIES.get(chart.filename)
            official_values = next(
                (candidate for candidate_name, candidate in spec.series if candidate_name == official), None
            )
            points = []
            for category_index, value in enumerate(values):
                if value is None:
                    continue
                x = plot_x + category_index * x_step
                y = plot_y + plot_height - value / axis_max * plot_height
                points.append((x, y, value, spec.categories[category_index]))
            if len(points) > 1:
                path = ' '.join(f'{x:.1f},{y:.1f}' for x, y, _, _ in points)
                chunks.append(
                    f'<polyline points="{path}" fill="none" stroke="{series_color}" '
                    'stroke-width="3" stroke-linejoin="round" stroke-linecap="round"/>'
                )
            for x, y, value, category in points:
                chunks.append(
                    f'<circle cx="{x:.1f}" cy="{y:.1f}" r="4.5" fill="{series_color}" '
                    'stroke="#ffffff" stroke-width="2">'
                    f'<title>{esc(spec.title)}: {esc(name)}, {esc(category)}, '
                    f'{value:.2f} GiB/s</title></circle>'
                )
                label = f'{value:.2f}'
                if name == 'hashcodecs' and official_values is not None:
                    official_value = official_values[spec.categories.index(category)]
                    if official_value is not None and official_value > 0:
                        label += f' ({value / official_value:.2f}x)'

                preferred = -10 if series_index % 2 == 0 else 17
                offsets = (preferred, -10, 17, -26, 33, -42, 49, -58, 65)
                label_width = len(label) * 6.6
                label_x = min(
                    max(x, plot_x + label_width / 2 + 5),
                    plot_x + plot_width - label_width / 2 - 5,
                )
                candidate_ys = [y + offset for offset in dict.fromkeys(offsets)]
                candidate_ys.extend(plot_y + 12 + lane * 16 for lane in range(13))
                label_y = candidate_ys[-1]
                label_box = (
                    label_x - label_width / 2 - 2,
                    label_y - 11,
                    label_x + label_width / 2 + 2,
                    label_y + 3,
                )
                for candidate_y in candidate_ys:
                    candidate = (
                        label_x - label_width / 2 - 2,
                        candidate_y - 11,
                        label_x + label_width / 2 + 2,
                        candidate_y + 3,
                    )
                    if candidate[1] < plot_y or candidate[3] > plot_y + plot_height:
                        continue
                    if any(
                        candidate[0] < right and candidate[2] > left and candidate[1] < bottom and candidate[3] > top
                        for left, top, right, bottom in label_boxes
                    ):
                        continue
                    label_y = candidate_y
                    label_box = candidate
                    break
                label_boxes.append(label_box)
                chunks.append(
                    f'<text x="{label_x:.1f}" y="{label_y:.1f}" text-anchor="middle" '
                    f'fill="{series_color}" font-family="Segoe UI,Arial,sans-serif" '
                    'font-size="11" font-weight="600" paint-order="stroke" '
                    f'stroke="#ffffff" stroke-width="3" stroke-linejoin="round">{label}</text>'
                )

    chunks.append('</svg>')
    return '\n'.join(chunks) + '\n'


def chart_value(filename: str, panel_title: str, category: str, series_name: str) -> float:
    chart = next(chart for chart in CHARTS if chart.filename == filename)
    spec = next(spec for spec in chart.panels if spec.title == panel_title)
    series = next(values for name, values in spec.series if name == series_name)
    value = series[spec.categories.index(category)]
    if value is None:
        raise ValueError(f'missing benchmark value: {filename}, {panel_title}, {category}, {series_name}')
    return value


def render_performance_at_a_glance() -> str:
    """Render like-for-like Python encode and decode benchmarks for the README."""
    benchmarks = tuple(
        (
            operation,
            tuple(
                (
                    implementation,
                    chart_value('base64-python.svg', f'Standard {operation.lower()}', '4 KiB', implementation),
                )
                for implementation in ('hashcodecs', 'pybase64', 'CPython')
            ),
        )
        for operation in ('Encode', 'Decode')
    )
    width = 1200
    height = 460
    panel_width = width / 2
    label_width = 125
    plot_width = 410
    bar_height = 48
    bar_gap = 30
    first_bar_y = 178
    axis_max = math.ceil(max(value for _, measurements in benchmarks for _, value in measurements) / 2) * 2
    chunks = [
        (
            f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" '
            f'viewBox="0 0 {width} {height}" role="img" aria-labelledby="title desc">'
        ),
        '<title id="title">CPython 3.12 standard Base64 throughput</title>',
        (
            '<desc id="desc">On CPython 3.12 with 4 KiB inputs, hashcodecs reaches '
            f'{benchmarks[0][1][0][1]:.2f} GiB/s encoding and {benchmarks[1][1][0][1]:.2f} GiB/s decoding. '
            'Both panels use the same throughput scale. Higher is better.</desc>'
        ),
        '<rect width="100%" height="100%" rx="18" fill="#f7faf9"/>',
        (
            '<text x="600" y="52" text-anchor="middle" fill="#172033" '
            'font-family="Segoe UI,Arial,sans-serif" font-size="30" font-weight="750">'
            'Python Base64 performance at a glance</text>'
        ),
        (
            '<text x="600" y="84" text-anchor="middle" fill="#465263" '
            'font-family="Segoe UI,Arial,sans-serif" font-size="17" font-weight="600">'
            'Standard · CPython 3.12 · 4 KiB inputs · GiB/s, higher is better</text>'
        ),
        '<line x1="600" y1="112" x2="600" y2="402" stroke="#d8dfe5"/>',
    ]

    for panel_index, (operation, measurements) in enumerate(benchmarks):
        panel_x = panel_index * panel_width
        center_x = panel_x + panel_width / 2
        plot_x = panel_x + label_width + 35
        ours = measurements[0][1]
        pybase64 = measurements[1][1]
        cpython = measurements[2][1]
        chunks.extend(
            (
                (
                    f'<text x="{center_x:.1f}" y="126" text-anchor="middle" fill="#172033" '
                    f'font-family="Segoe UI,Arial,sans-serif" font-size="21" font-weight="700">{operation}</text>'
                ),
                (
                    f'<text x="{center_x:.1f}" y="152" text-anchor="middle" fill="#637083" '
                    'font-family="Segoe UI,Arial,sans-serif" font-size="14" font-weight="600">'
                    f'{ours / cpython:.0f}&#215; CPython · {ours / pybase64:.0f}&#215; pybase64</text>'
                ),
            )
        )

        for series_index, (name, value) in enumerate(measurements):
            y = first_bar_y + series_index * (bar_height + bar_gap)
            bar_width = value / axis_max * plot_width
            series_color = color(name, series_index)
            label_inside = bar_width >= 150
            value_x = plot_x + bar_width - 12 if label_inside else plot_x + bar_width + 12
            chunks.extend(
                (
                    (
                        f'<text x="{plot_x - 16:.1f}" y="{y + 32}" text-anchor="end" fill="#172033" '
                        f'font-family="Segoe UI,Arial,sans-serif" font-size="17" font-weight="650">{esc(name)}</text>'
                    ),
                    (
                        f'<rect x="{plot_x:.1f}" y="{y}" width="{plot_width}" height="{bar_height}" '
                        'rx="8" fill="#e7ecef"/>'
                    ),
                    (
                        f'<rect x="{plot_x:.1f}" y="{y}" width="{bar_width:.1f}" height="{bar_height}" '
                        f'rx="8" fill="{series_color}"/>'
                    ),
                    (
                        f'<text x="{value_x:.1f}" y="{y + 31}" '
                        f'text-anchor="{"end" if label_inside else "start"}" '
                        f'fill="{"#ffffff" if label_inside else "#172033"}" '
                        'font-family="Segoe UI,Arial,sans-serif" font-size="16" font-weight="700">'
                        f'{value:.2f}</text>'
                    ),
                )
            )

    chunks.extend(
        (
            (
                '<text x="600" y="433" text-anchor="middle" fill="#637083" '
                'font-family="Segoe UI,Arial,sans-serif" font-size="12">'
                'Intel Core Ultra 7 265K · Windows 10 x64 · pinned CPU · 15 samples</text>'
            ),
            '</svg>',
        )
    )
    return '\n'.join(chunks) + '\n'


def write_csv() -> None:
    with (OUTPUT / 'results.csv').open('w', newline='', encoding='utf-8') as output:
        writer = csv.writer(output, lineterminator='\n')
        writer.writerow(('chart', 'panel', 'input', 'implementation', 'gib_per_second'))
        for chart in CHARTS:
            for spec in chart.panels:
                for name, values in spec.series:
                    for category, value in zip(spec.categories, values, strict=True):
                        if value is not None:
                            writer.writerow((chart.title, spec.title, category, name, f'{value:.2f}'))


def main() -> None:
    OUTPUT.mkdir(parents=True, exist_ok=True)
    for chart in CHARTS:
        (OUTPUT / chart.filename).write_text(render(chart), encoding='utf-8', newline='\n')
    (OUTPUT / 'performance-at-a-glance.svg').write_text(
        render_performance_at_a_glance(), encoding='utf-8', newline='\n'
    )
    write_csv()


if __name__ == '__main__':
    main()
