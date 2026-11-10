#!/usr/bin/env python3
"""Summarize existing private texture comparisons without rendering or changing thresholds."""
import argparse
import html
import json
import math
from pathlib import Path
import re
import statistics

START = '<!-- texture-comparison-summary:start -->'
END = '<!-- texture-comparison-summary:end -->'


def distribution(values, threshold):
    values = sorted(values)
    if not values:
        return dict(count=0, mean=None, median=None, p95=None, maximum=None,
                    threshold=threshold, exceeding=0)
    # Nearest-rank percentile: includes the worst capture for small samples.
    return dict(count=len(values), mean=statistics.fmean(values),
                median=statistics.median(values), p95=values[math.ceil(.95*len(values))-1],
                maximum=values[-1], threshold=threshold,
                exceeding=sum(v > threshold for v in values))


def summarize(review):
    threshold = review['allowed_changed_fraction']
    groups = {}
    values = []
    excluded = 0
    field = 'parity_changed_fraction' if review.get('engine', '').startswith('original') else 'changed_fraction'
    for capture in review['captures']:
        value = capture.get(field)
        if (capture.get('capture_error') or capture.get('status') == 'invalid capture'
                or not isinstance(value, (int, float)) or not math.isfinite(value)
                or not 0 <= value <= 1):
            excluded += 1
            continue
        values.append(value)
        groups.setdefault(capture['source'], []).append(value)
    result = distribution(values, threshold)
    result.update(excluded=excluded, metric=field, weighting='equal weight per valid capture',
                  by_source={key: distribution(v, threshold) for key, v in sorted(groups.items())})
    return result


def summary_html(summary, channel_tolerance=None):
    def percent(value):
        return '—' if value is None else f'{value:.2%}'
    text = [START, '<section id="comparison-summary"><h2>Pixel difference summary</h2>',
            '<p>Changed pixels per screenshot, averaged across valid captures. '
            'Each capture has equal weight. The average is descriptive; '
            'individual capture thresholds and failures remain unchanged.</p>']
    if channel_tolerance is not None:
        text.append(f'<p>A pixel changes when any channel differs by more than {channel_tolerance}/255.</p>')
    text.append('<table style="border-spacing:1em .4em;text-align:right"><thead><tr>'
                '<th>Source</th><th>Captures</th><th>Average reached</th><th>Median</th>'
                '<th>95th percentile</th><th>Maximum</th><th>Pass threshold</th>'
                '<th>Above threshold</th></tr></thead><tbody>')
    for key, row in [('Overall', summary), *summary.get('by_source', {}).items()]:
        text.append('<tr><th style="text-align:left">'+html.escape(key)+'</th>'
                    f'<td>{row["count"]}</td>'
                    + ''.join(f'<td>{percent(row[field])}</td>' for field in ['mean','median','p95','maximum','threshold'])
                    + f'<td>{row["exceeding"]}/{row["count"]}</td></tr>')
    text.append('</tbody></table>')
    if summary.get('excluded'):
        text.append(f'<p>{summary["excluded"]} captures excluded: failed or without a valid comparison.</p>')
    text.append('<p>A small average can hide localized defects. Differences between renderers '
                'may contribute, but this statistic alone does not establish their cause.</p></section>')
    text.append(END)
    return '\n'.join(text)


def inject(report, summary, channel_tolerance=None):
    block = summary_html(summary, channel_tolerance)
    if START in report:
        return re.sub(re.escape(START)+r'.*?'+re.escape(END), lambda _: block, report, flags=re.S)
    match = re.search(r'</h1>', report, flags=re.I)
    if match:
        return report[:match.end()]+'\n'+block+report[match.end():]
    return block+'\n'+report


def refresh(run):
    run = run.resolve()
    if (run/'archive.json').exists():
        raise ValueError('archived runs are immutable; restore to a new directory first')
    metadata = run/('review.json' if (run/'review.json').exists() else 'index.json')
    review = json.loads(metadata.read_text())
    if review.get('human_approval') != 'pending':
        raise ValueError('only pending, unapproved reports may be refreshed')
    if review.get('kind') == 'private-original-texture-index':
        captures = []
        thresholds, tolerances = set(), set()
        for entry in review['runs']:
            child = run.parent/entry['run']
            data = json.loads((child/'review.json').read_text())
            captures.extend(data['captures'])
            thresholds.add(data['allowed_changed_fraction'])
            tolerances.add(data['channel_tolerance'])
        if len(thresholds) != 1 or len(tolerances) != 1:
            raise ValueError('combined reports must share comparison thresholds')
        data = dict(engine='original-software', captures=captures,
                    allowed_changed_fraction=thresholds.pop(), channel_tolerance=tolerances.pop())
    elif review.get('kind') == 'private-texture-review':
        data = review
    else:
        raise ValueError('not a texture review')
    summary = summarize(data)
    report = run/'report.html'
    updated = inject(report.read_text(), summary, data['channel_tolerance'])
    review['comparison_summary'] = summary
    report.write_text(updated)
    metadata.write_text(json.dumps(review, indent=2)+'\n')
    return summary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('runs', nargs='+', type=Path)
    args = parser.parse_args()
    for run in args.runs:
        summary = refresh(run)
        mean = 'no comparisons' if summary['mean'] is None else f'{summary["mean"]:.2%}'
        print(f'{run}: average {mean}, threshold {summary["threshold"]:.2%}, {summary["count"]} comparisons')


if __name__ == '__main__':
    main()
