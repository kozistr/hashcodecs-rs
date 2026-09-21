"""Render the checked-in benchmark measurements as dependency-free SVG charts."""

from __future__ import annotations

import csv
import html
import math
from collections.abc import Sequence
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / 'docs' / 'benchmarks'
PYTHON_RUNTIME = 'CPython 3.14.6 free-threaded'

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

OFFICIAL_SERIES = {
    'base64-rust.svg': 'base64',
    'murmur3-rust.svg': 'murmur3',
    'xxh3-rust.svg': 'upstream C',
    'xxh3-rust-batch-remainders.svg': 'upstream C',
    'base64-python.svg': 'CPython',
    'base64-python-lenient.svg': 'CPython',
    'base64-python-str.svg': 'CPython',
    'murmur3-python.svg': 'mmh3',
    'xxh3-python.svg': 'xxhash',
    'base64-python-batch.svg': 'CPython',
}

CHART_FILES = {
    'Rust Base64 throughput': 'base64-rust.svg',
    'Rust MurmurHash3 throughput': 'murmur3-rust.svg',
    'Rust XXH3 throughput': 'xxh3-rust.svg',
    'Rust XXH3 batch remainder throughput': 'xxh3-rust-batch-remainders.svg',
    'Python Base64 throughput': 'base64-python.svg',
    'Python Base64 ASCII string throughput': 'base64-python-str.svg',
    'Lenient Python Base64 throughput': 'base64-python-lenient.svg',
    'Python MurmurHash3 throughput': 'murmur3-python.svg',
    'Python XXH3 throughput': 'xxh3-python.svg',
    'Reusable Python Base64 buffers': 'base64-python-reusable.svg',
    'Python Base64 memoryview inputs': 'base64-python-memoryview.svg',
    'Python Base64 batch throughput': 'base64-python-batch.svg',
    'Reusable Python Base64 batch buffers': 'base64-python-batch-reusable.svg',
    'Large Python Base64 batches': 'base64-python-batch-large.svg',
    'Python Base64 memoryview batch throughput': 'base64-python-batch-memoryview.svg',
    'Mutable Python Base64 inputs': 'base64-python-mutable.svg',
    'Mutable Python MurmurHash3 inputs': 'murmur3-python-mutable.svg',
}


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


def load_charts(path: Path) -> tuple[Chart, ...]:
    """Load measurements in CSV order; an empty value marks an unavailable result."""
    fields = ('chart', 'panel', 'input', 'implementation', 'gib_per_second')
    data: dict[str, dict[str, dict[str, dict[str, float | None]]]] = {}
    with path.open(newline='', encoding='utf-8') as source:
        reader = csv.DictReader(source)
        if reader.fieldnames != list(fields):
            raise ValueError(f'{path}: expected CSV columns {", ".join(fields)}')
        for line, row in enumerate(reader, 2):
            if None in row or any(value is None for value in row.values()):
                raise ValueError(f'{path}:{line}: expected {len(fields)} CSV fields')
            title, panel, category, name, raw = (row[field].strip() for field in fields)
            if not all((title, panel, category, name)):
                raise ValueError(f'{path}:{line}: chart, panel, input and implementation must be nonempty')
            if title not in CHART_FILES:
                raise ValueError(f'{path}:{line}: unknown chart {title!r}')
            value = float(raw) if raw else None
            if value is not None and (not math.isfinite(value) or value <= 0):
                raise ValueError(f'{path}:{line}: throughput must be finite and positive, or empty')
            values = data.setdefault(title, {}).setdefault(panel, {}).setdefault(name, {})
            if category in values:
                raise ValueError(f'{path}:{line}: duplicate measurement: {title}, {panel}, {category}, {name}')
            values[category] = value

    if not data:
        raise ValueError(f'{path}: no benchmark measurements')
    charts = []
    for title, panels in data.items():
        specs = []
        for panel, series in panels.items():
            categories = tuple(next(iter(series.values())))
            if any(values.keys() != set(categories) for values in series.values()):
                raise ValueError(f'{path}: {title}, {panel}: each series must have the same inputs; use empty values')
            if all(value is None for values in series.values() for value in values.values()):
                raise ValueError(f'{path}: {title}, {panel}: no available measurements')
            specs.append(
                Panel(
                    panel,
                    categories,
                    tuple(
                        (name, tuple(values[category] for category in categories)) for name, values in series.items()
                    ),
                )
            )
        charts.append(Chart(CHART_FILES[title], title, tuple(specs)))
    return tuple(charts)


def esc(value: object) -> str:
    return html.escape(str(value), quote=True)


def nice_max(value: float) -> float:
    rough = value / 4
    exponent = 10 ** math.floor(math.log10(rough)) if rough else 1
    step = next(candidate * exponent for candidate in (1, 2, 5, 10) if candidate * exponent >= rough)
    return math.ceil(value / step) * step


def color(name: str, index: int) -> str:
    return COLORS.get(name, FALLBACK_COLORS[index % len(FALLBACK_COLORS)])


def render(chart: Chart) -> str:
    width = 1280
    columns = 1 if len(chart.panels) == 1 else 2
    panel_width = 1180 if columns == 1 else 570
    panel_height = 330
    rows = math.ceil(len(chart.panels) / columns)
    height = 100 + rows * panel_height + 30
    subtitle = 'Throughput (GiB/s), higher is better'
    if '-python' in chart.filename:
        subtitle = f'{PYTHON_RUNTIME} \u2022 {subtitle}'

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
            f'font-size="14">{esc(subtitle)}</text>'
        ),
    ]

    for panel_index, spec in enumerate(chart.panels):
        row, column = divmod(panel_index, columns)

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


def chart_value(charts: Sequence[Chart], filename: str, panel_title: str, category: str, series_name: str) -> float:
    chart = next(chart for chart in charts if chart.filename == filename)
    spec = next(spec for spec in chart.panels if spec.title == panel_title)
    series = next(values for name, values in spec.series if name == series_name)
    value = series[spec.categories.index(category)]
    if value is None:
        raise ValueError(f'missing benchmark value: {filename}, {panel_title}, {category}, {series_name}')
    return value


def render_performance_at_a_glance(charts: Sequence[Chart]) -> str:
    """Render like-for-like Python encode and decode benchmarks for the README."""
    benchmarks = tuple(
        (
            operation,
            tuple(
                (
                    implementation,
                    chart_value(charts, 'base64-python.svg', f'Standard {operation.lower()}', '4 KiB', implementation),
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
        f'<title id="title">{PYTHON_RUNTIME} standard Base64 throughput</title>',
        (
            f'<desc id="desc">On {PYTHON_RUNTIME} with 4 KiB inputs, hashcodecs reaches '
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
            f'Standard \u2022 {PYTHON_RUNTIME} \u2022 4 KiB inputs \u2022 GiB/s, higher is better</text>'
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
                    f'{ours / cpython:.1f}&#215; CPython \u2022 {ours / pybase64:.1f}&#215; pybase64</text>'
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
                'Intel Core Ultra 7 265K \u2022 Windows 10 x64 \u2022 pinned CPU \u2022 15 samples</text>'
            ),
            '</svg>',
        )
    )

    return '\n'.join(chunks) + '\n'


def main() -> None:
    charts = load_charts(OUTPUT / 'results.csv')
    images = {chart.filename: render(chart) for chart in charts}
    images['performance-at-a-glance.svg'] = render_performance_at_a_glance(charts)
    for filename, svg in images.items():
        (OUTPUT / filename).write_text(svg, encoding='utf-8', newline='\n')


if __name__ == '__main__':
    main()
