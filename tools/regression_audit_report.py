"""Offline report assembly for regression_audit. Never modifies child reports."""
import html
import json
import os
from pathlib import Path
import shlex
from urllib.parse import quote


def escape(value):
    return html.escape(str(value), quote=True)


def link(root, path, label=None):
    relative = os.path.relpath(path, root)
    return f'<a href="{quote(relative, safe="/")}">{escape(label or relative)}</a>'


def read_json(path):
    try:
        return json.loads(path.read_text())
    except (OSError, ValueError):
        return {'unavailable': 'missing or invalid JSON'}


def images(directory):
    if not directory.is_dir():
        return {}
    return {p.name: p for p in directory.glob('*.png')}


def artifacts(root):
    names = {'report.html', 'metrics.json', 'comparison.json', 'review.json', 'manifest.json'}
    excluded = {'_original', 'native-work', 'c-tests', 'logs', 'screenshots', 'poses', 'models'}
    for directory, children, files in os.walk(root):
        children[:] = [name for name in children if name not in excluded
                       and not (Path(directory) / name).is_symlink()]
        for name in files:
            path = Path(directory) / name
            if name in names and path.parent != root and not path.is_symlink():
                yield path


def picture(root, path, caption):
    if path is None:
        return f'<figure><figcaption>{escape(caption)}</figcaption><p class=missing>Missing capture</p></figure>'
    url = quote(os.path.relpath(path, root), safe='/')
    return (f'<figure><figcaption>{escape(caption)}</figcaption><a href="{url}">'
            f'<img loading="lazy" src="{url}" alt="{escape(caption)}"></a></figure>')


def comparison(root, pair):
    parts = [f'<details><summary>{escape(pair["name"])} — {escape(pair["status"])}</summary>']
    if pair['status'] != 'complete':
        parts.append('<p class=missing>' + escape(pair.get('reason', 'Original comparison report missing; inspect the capture log.')) + '</p></details>')
        return ''.join(parts)
    original, rust = root / pair['original'], root / pair['rust']
    parts.append('<p>' + link(root, original / 'report.html', 'Original report') + ' · ' +
                 link(root, rust / 'report.html', 'Rust report') + '</p>')
    if pair.get('pdf'):
        parts.append('<p>' + link(root, root / pair['pdf'], 'Side-by-side PDF') + '</p>')
    if pair['kind'] == 'matched-texture':
        review = read_json(original / 'review.json')
        parts.append('<p>Matched pose/camera captures. Native capture validity and pixel parity are separate checks.</p>')
        for capture in review.get('captures', []):
            name = capture.get('file', '')
            # Treat metadata filenames as untrusted paths, even for private reports.
            if not name or Path(name).name != name or '\\' in name:
                continue
            paths = [original / 'screenshots' / (prefix + name) for prefix in ['', 'rust-', 'diff-']]
            parts.append('<p>' + escape(f'{capture.get("source", "")} · {capture.get("asset", "")} · {capture.get("clip", "")} · {capture.get("side", "")}') + '</p><div class=pair>')
            parts.extend(picture(root, p if p.is_file() else None, label)
                         for p, label in zip(paths, ['Original', 'Rust', 'Difference']))
            parts.append('</div>')
    else:
        parts.append('<p>Checkpoints matched by name. Gameplay renderers and animal placement differ; these images do not establish pixel parity.</p>')
        before, after = images(original / 'screenshots'), images(rust / 'screenshots')
        if not before or not after:
            parts.append('<p class=missing>Visual coverage incomplete: one or both engines produced no screenshots.</p>')
        for name in sorted(before.keys() | after.keys()):
            parts.append('<p>' + escape(name) + '</p><div class=pair>' +
                         picture(root, before.get(name), 'Original') + picture(root, after.get(name), 'Rust') + '</div>')
    parts.append('</details>')
    return ''.join(parts)


def write_report(root, state):
    parts = ['<!doctype html><html lang=en><meta charset=utf-8><title>Regression audit</title>',
             '<style>body{max-width:1500px;margin:2em auto;padding:0 1em;background:#171b22;color:#e7edf5;font:16px system-ui}'
             'a{color:#94c8ff}table{border-collapse:collapse;width:100%}td,th{padding:.6em;border:1px solid #46505c;text-align:left}'
             '.pair{display:flex;gap:1em}figure{flex:1;min-width:0;margin:0}img{width:100%;image-rendering:auto}'
             'details{padding:1em;border:1px solid #46505c;margin:1em 0}summary{cursor:pointer}.missing{color:#ffb888}'
             'pre{white-space:pre-wrap;overflow-wrap:anywhere}figcaption{font-weight:bold}@media(max-width:700px){.pair{display:block}}</style>',
             '<h1>Regression audit</h1><p>Private local results · human approval pending. '
             'No automatic baseline approval, publication, archival or cleanup.</p>',
             '<p>Tests → builds → captures → original/Rust comparisons → this report. '
             'Only configured games and suites are covered; hardware and emulator checks require separate runs.</p>',
             '<p>' + link(root, root / 'audit.json', 'Audit JSON') + ' · ' +
             link(root, root / 'config.json', 'Configuration and coverage') + '</p>']
    if state.get('interrupted'):
        parts.append('<p class=missing>Audit interrupted; remaining steps were not run.</p>')
    counts = {status: sum(s['status'] == status for s in state['steps'])
              for status in sorted({s['status'] for s in state['steps']})}
    complete = sum(p['status'] == 'complete' for p in state['pairs'])
    parts.append('<p>Command counts: ' + escape(', '.join(f'{key}: {value}' for key, value in counts.items()) or 'none yet') +
                 f'. Comparison groups with captures: {complete}/{len(state["pairs"])}. '
                 'Completion describes available artifacts; inspect each command verdict and capture validity before accepting them.</p>')
    parts.append('<h2>Commands and completion</h2><table><tr><th>Step</th><th>Status</th><th>Seconds</th><th>Log</th></tr>')
    for step in state['steps']:
        command = escape(shlex.join(step['argv']))
        parts.append(f'<tr><td><details><summary>{escape(step["name"])}</summary>'
                     f'<p>Directory: {escape(step["cwd"])}</p><pre>{command}</pre></details></td><td>{escape(step["status"])} '
                     f'(exit {escape(step.get("exit_code"))})</td><td>{step["seconds"]}</td><td>' +
                     link(root, root / step['log'], 'Log') + '</td></tr>')
    parts.append('</table><h2>Original and Rust side by side</h2>')
    if not state['pairs']:
        parts.append('<p class=missing>No original-engine comparisons configured or reached.</p>')
    parts.extend(comparison(root, pair) for pair in state['pairs'])
    parts.append('<h2>All child reports and machine-readable results</h2>')
    # Never crawl game copies / Wine prefixes for reports or game metadata.
    for path in sorted(artifacts(root)):
        parts.append('<details><summary>' + link(root, path) + '</summary>')
        if path.suffix == '.json':
            value = read_json(path)
            # Full detailed metrics are retained in child JSON; texture capture arrays
            # can contain tens of thousands of entries and already have galleries.
            compact = {k: v for k, v in value.items() if k not in {'captures', 'files', 'checkpoints'}}
            parts.append('<pre>' + escape(json.dumps(compact, indent=2)) + '</pre>')
        parts.append('</details>')
    parts.append('<h2>Review and cleanup</h2><p>Review each child report and its screenshots before approving a baseline. '
                 'For textures, use tools/texture-review-archive.py only after your review; --reviewed-by records your attestation, '
                 '--prune optionally cleans verified Rust media. Original reference media must remain. '
                 'Gameplay approval retains the chosen run and a separate approval note. See docs/TEXTURE-REVIEW.md and docs/REGRESSION.md.</p></html>')
    temporary = root / 'report.html.tmp'
    temporary.write_text('\n'.join(parts))
    temporary.replace(root / 'report.html')
