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
                hashcodecs=[33.90, 41.83, 62.96, 79.74, 46.03],
                upstream_C=[28.42, 26.27, 39.63, 48.79, 38.62],
            ),
            panel(
                'XXH3-128 one-shot',
                ['64 B', *SIZES],
                hashcodecs=[11.23, 34.06, 57.72, 79.70, 46.13],
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
        'base64-python.svg',
        'Python Base64 throughput',
        (
            panel(
                'Standard encode',
                SIZES,
                hashcodecs=[11.00, 23.27, 3.73, 4.04],
                CPython=[0.44, 0.46, 0.40, 0.42],
                pybase64=[5.02, 13.11, 2.68, 2.91],
            ),
            panel(
                'Standard decode',
                SIZES,
                hashcodecs=[8.62, 17.47, 4.41, 4.85],
                CPython=[0.93, 1.08, 0.84, 0.90],
                pybase64=[3.09, 8.01, 3.26, 3.57],
            ),
            panel(
                'URL-safe encode',
                SIZES,
                hashcodecs=[9.92, 22.48, 3.89, 4.08],
                CPython=[0.37, 0.39, 0.33, 0.34],
                pybase64=[0.96, 1.12, 0.84, 0.84],
            ),
            panel(
                'URL-safe decode',
                SIZES,
                hashcodecs=[7.44, 13.88, 4.84, 5.13],
                CPython=[0.47, 0.69, 0.60, 0.59],
                pybase64=[1.13, 1.47, 1.34, 1.38],
            ),
        ),
    ),
    Chart(
        'murmur3-python.svg',
        'Python MurmurHash3 throughput',
        tuple(
            panel(title, SIZES, hashcodecs=ours, mmh3=upstream)
            for title, ours, upstream in (
                ('x86 32-bit one-shot', [3.55, 3.85, 4.00, 3.99], [3.44, 3.72, 3.84, 3.83]),
                ('x86 32-bit incremental', [2.84, 3.62, 4.00, 3.99], [2.86, 3.48, 3.84, 3.84]),
                ('x86 128-bit one-shot', [6.61, 8.57, 9.42, 9.43], [6.69, 8.22, 8.89, 8.86]),
                ('x86 128-bit incremental', [4.40, 7.20, 9.40, 9.41], [0.69, 0.78, 0.80, 0.81]),
                ('x64 128-bit one-shot', [6.99, 9.02, 10.08, 10.03], [7.79, 9.48, 10.26, 10.26]),
                ('x64 128-bit incremental', [4.73, 7.87, 10.07, 10.05], [5.32, 8.05, 9.36, 8.20]),
            )
        ),
    ),
    Chart(
        'xxh3-python.svg',
        'Python XXH3 throughput',
        (
            panel(
                'XXH3-64 one-shot', SIZES, hashcodecs=[16.85, 40.70, 78.83, 46.54], xxhash=[13.42, 29.23, 47.10, 38.50]
            ),
            panel(
                'XXH3-128 one-shot', SIZES, hashcodecs=[14.42, 36.68, 78.41, 46.53], xxhash=[9.27, 22.42, 46.74, 38.79]
            ),
            panel(
                'XXH3-64 batch (32 items)',
                ['64 B', '1 KiB', '4 KiB', '1 MiB'],
                hashcodecs=[5.20, 43.02, 61.22, 36.59],
                xxhash=[2.15, 14.55, 30.50, 18.39],
            ),
            panel(
                'XXH3-128 batch (32 items)',
                ['64 B', '1 KiB', '4 KiB', '1 MiB'],
                hashcodecs=[2.72, 29.27, 52.01, 36.50],
                xxhash=[1.01, 9.87, 23.87, 13.75],
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
                'Encode',
                SIZES,
                returned_bytes=[6.24, 19.96, 3.88, 4.71],
                reusable_bytearray=[7.16, 22.76, 40.06, 30.37],
            ),
            panel(
                'Decode',
                SIZES,
                returned_bytes=[5.60, 15.62, 4.48, 4.11],
                reusable_bytearray=[6.21, 17.20, 29.38, 19.51],
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
                ('16 B encode', [0.54, 0.71, 0.73], [0.30, 0.33, 0.33], [0.12, 0.13, 0.13], [0.18, 0.19, 0.19]),
                ('16 B decode', [0.47, 0.63, 0.63], [0.22, 0.24, 0.24], [0.08, 0.08, 0.08], [0.12, 0.12, 0.13]),
                ('256 B encode', [6.85, 8.56, 8.32], [4.33, 4.41, 4.34], [1.86, 1.82, 1.87], [0.38, 0.39, 0.38]),
                ('256 B decode', [6.39, 7.60, 7.02], [3.26, 3.39, 3.27], [1.17, 1.22, 1.23], [0.74, 0.75, 0.72]),
                ('4 KiB encode', [14.98, 23.53, 2.12], [20.55, 15.76, 2.46], [12.23, 8.99, 2.27], [0.46, 0.45, 0.39]),
                ('4 KiB decode', [20.29, 19.63, 7.30], [16.03, 15.50, 11.21], [7.74, 7.62, 6.66], [1.07, 1.06, 1.05]),
            )
        ),
    ),
    Chart(
        'base64-python-batch-reusable.svg',
        'Reusable Python Base64 batch buffers',
        tuple(
            panel(title, ['8', '64', '1,024'], encode=encode, decode=decode)
            for title, encode, decode in (
                ('16 B items', [0.43, 0.59, 0.58], [0.38, 0.56, 0.56]),
                ('256 B items', [5.23, 6.57, 6.51], [5.11, 6.67, 6.55]),
                ('4 KiB items', [26.98, 27.85, 17.96], [21.66, 22.75, 17.31]),
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
                returned_encode=[3.09, 3.17, 3.17, 3.03, 2.94, 2.83],
                reusable_encode=[39.99, 20.96, 20.21, 19.49, 13.33, 11.03],
                returned_decode=[4.02, 3.90, 4.01, 3.90, 3.51, 3.38],
                reusable_decode=[29.32, 21.49, 20.95, 18.96, 12.17, 10.59],
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
    'base64-python.svg': 'CPython',
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
    write_csv()


if __name__ == '__main__':
    main()
