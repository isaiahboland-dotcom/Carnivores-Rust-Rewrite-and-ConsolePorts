"""Master report assembly for regression_audit.

Reads the immutable results of one audit directory and writes `report.html`
(a copy of the static page) and `report-data.js` (everything the page shows).
Never modifies child reports, capture results or `reviews.json`.
"""
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import re
import shlex
import shutil

from regression_audit_findings import build_findings

PAGE = Path(__file__).with_name('regression_audit_page.html')
REVIEW_STATUSES = ['pending', 'investigating', 'confirmed issue', 'explained difference', 'accepted']
OVERLAY = 'report-config.json'
SWEEPS = [('compact', 'texture-compact', 'Compact sweep'), ('full', 'texture-exhaustive', 'Full sweep')]
CHECKPOINT_KINDS = [
    (r'weapon_\d+_(?P<subject>.+)_(?P<phase>ready|fire|after)', 'weapon'),
    (r'kill_\d+_(?P<subject>.+)_(?P<phase>start|mid|late)', 'kill'),
    (r'(?P<phase>run|die)_\d+_(?P<subject>.+)', 'behaviour'),
    (r'(?P<subject>hunter)_(?P<phase>walk|run)', 'behaviour'),
    (r'(?P<subject>binoculars|map|call_icon|call_picker|weapon_picker)_(?P<phase>open|closed)', 'screen'),
]


def read_json(path):
    try:
        return json.loads(Path(path).read_text())
    except (OSError, ValueError):
        return {}


def relative(root, path):
    return os.path.relpath(path, root).replace(os.sep, '/')


def safe_name(name):
    """Metadata filenames are untrusted, even in private reports."""
    return bool(name) and Path(name).name == name and '\\' not in name


def classify(checkpoint):
    for pattern, area in CHECKPOINT_KINDS:
        match = re.fullmatch(pattern, checkpoint)
        if match:
            return area, match['subject'].replace('_', ' '), match['phase']
    return 'scene', checkpoint.replace('_', ' '), ''


def argument(argv, flag):
    return argv[argv.index(flag) + 1] if flag in argv[:-1] else None


def stage_results(run):
    """Worst recorded result for each stage of one run, and its checkpoints."""
    stages, checkpoints = {}, {}
    try:
        lines = (run / 'events.jsonl').read_text().splitlines()
    except OSError:
        lines = []
    for line in lines:
        try:
            event = json.loads(line)
        except ValueError:
            continue
        stage = event.get('stage')
        if event.get('kind') == 'checkpoint':
            checkpoints[event.get('name')] = stage
        elif event.get('kind') == 'stage_end' and stage:
            previous = stages.get(stage)
            if previous is None or previous[0] == 'passed':
                stages[stage] = (event.get('name'), event.get('detail') or '')
    return stages, checkpoints


def stage_of(checkpoint, *runs):
    for stages, checkpoints in runs:
        if checkpoints.get(checkpoint):
            return checkpoints[checkpoint]
    names = [name for stages, _ in runs for name in stages
             if checkpoint == name or checkpoint.startswith(name + '_')]
    return max(names, key=len) if names else checkpoint


def run_directory(root, step):
    """The single run a capture command produced, or None."""
    out = argument(step['argv'], '--out')
    target = Path(out) if out else None
    if target is None or not target.is_dir():
        return None
    runs = sorted(path.parent for path in target.rglob('metrics.json'))
    return runs[0] if len(runs) == 1 else None


def engine_cell(root, step, run):
    """Coverage state of one engine: capture completion, not image parity."""
    if step is None:
        return dict(state='unsupported', detail='not configured for this game; no native adapter is assumed')
    cell = dict(log=step['log'])
    if step['status'] == 'blocked':
        return dict(cell, state='blocked', detail=step.get('reason', 'prerequisite did not pass'))
    if run is None:
        return dict(cell, state='missing', detail=f'command {step["status"]}; no run was recorded')
    metrics = read_json(run / 'metrics.json')
    shots = len(list((run / 'screenshots').glob('*.png')))
    stages = metrics.get('functional', {}).get('stages', {})
    notes = [f'{count} {name}' for name, count in stages.items() if count]
    if metrics.get('determinism', {}).get('identical') is False:
        notes.append('repeats were not identical')
    cell.update(report=relative(root, run / 'report.html'), captures=shots, detail=', '.join(notes))
    if not shots:
        return dict(cell, state='invalid', detail='run recorded no screenshots; ' + cell['detail'])
    return dict(cell, state='completed' if metrics.get('result') == 'passed' else 'failed')


def gameplay(root, state, data):
    """One group per game and scenario; one item per checkpoint name."""
    captures = {}
    for step in state['steps']:
        argv = step['argv']
        if 'run' in argv and argument(argv, '--scenario') and argument(argv, '--engine'):
            key = (Path(argument(argv, '--scenario')).stem, argument(argv, '--game-id') or '')
            captures.setdefault(key, {})[argument(argv, '--engine')] = step
    pairs = {pair['name']: pair for pair in state['pairs'] if pair['kind'] == 'gameplay'}
    for name, pair in pairs.items():
        if not any(f'{scenario}-{game}' == name for scenario, game in captures):
            captures[(name, '')] = {}
    for (scenario, game), steps in captures.items():
        pair = pairs.get(f'{scenario}-{game}' if game else scenario, {})
        runs = {}
        for engine in ['original', 'rust']:
            if pair.get(engine):
                runs[engine] = root / pair[engine]
            elif engine in steps:
                runs[engine] = run_directory(root, steps[engine])
        cells = {engine: engine_cell(root, steps.get(engine), runs.get(engine))
                 if steps else dict(state='completed' if runs.get(engine) else 'missing', detail='')
                 for engine in ['original', 'rust']}
        events = {engine: stage_results(run) for engine, run in runs.items() if run}
        shots = {engine: {p.stem: p for p in (run / 'screenshots').glob('*.png')} if run else {}
                 for engine, run in runs.items()}
        comparison = read_json(runs['rust'] / 'comparison.json') if runs.get('rust') else {}
        parity = {}
        for key in ['kill_parity', 'behaviour_parity']:
            for difference in (comparison.get(key) or {}).get('differences', []):
                parity.setdefault(difference.get('stage'), []).append(
                    f'{difference.get("what")}: original {difference.get("baseline")} · Rust {difference.get("candidate")}')
        native = cells['original']['state'] != 'unsupported'
        group = dict(kind='gameplay', label=f'{scenario} · {game}' if game else scenario,
                     game=game, scenario=scenario, cells=cells, parity=parity,
                     failed={engine: {stage: why for stage, (result, why) in stages.items() if result == 'failed'}
                             for engine, (stages, _) in events.items()},
                     dirs={engine: relative(root, run / 'screenshots') for engine, run in runs.items() if run},
                     reports={engine: relative(root, run / 'report.html') for engine, run in runs.items() if run},
                     note='Checkpoints are matched by name. Renderers and animal placement differ, '
                          'so no pixel metric is computed for gameplay pictures.')
        if pair.get('pdf'):
            group['reports']['pdf'] = pair['pdf']
        if comparison:
            group['reports']['comparison'] = relative(root, runs['rust'] / 'comparison.json')
        index = len(data['groups'])
        data['groups'].append(group)
        counts = dict(paired=0, original_only=0, rust_only=0)
        for checkpoint in sorted(set(shots.get('original', {})) | set(shots.get('rust', {}))):
            area, subject, phase = classify(checkpoint)
            stage = stage_of(checkpoint, *events.values())
            before, after = shots.get('original', {}).get(checkpoint), shots.get('rust', {}).get(checkpoint)
            types, detail = [], []
            for engine, (stages, _) in events.items():
                result, why = stages.get(stage, ('', ''))
                if result == 'failed':
                    types.append('functional-failure')
                    detail.append(f'{engine} stage {stage} failed: {why}')
                elif result == 'unsupported':
                    detail.append(f'{engine} stage {stage} unsupported: {why}')
            if stage in parity:
                types.append('parity-difference')
                detail.extend(parity[stage])
            if before and after:
                counts['paired'] += 1
            elif native:
                counts['original_only' if before else 'rust_only'] += 1
                types.append('original-only' if before else 'rust-only')
                detail.append('No Rust capture with this checkpoint name.' if before
                              else 'No original capture with this checkpoint name.')
            data['items'].append(dict(
                id=f'gameplay/{scenario}/{game}/{checkpoint}', g=index, area=area, subject=subject,
                phase=phase, name=checkpoint, o=before and before.name, r=after and after.name, d=None,
                pct=None, valid='n/a', types=sorted(set(types)), detail=detail,
                engines='both' if before and after else 'original' if before else 'rust'))
        group['counts'] = counts


def source_games(config):
    return {source: native.get('game', native['id'])
            for native in config.get('original_texture', []) for source in native.get('sources', [])}


def texture_item(kind, index, capture, games, allowed, matched):
    name = capture['file']
    stem = Path(name).stem
    error = capture.get('capture_error')
    pct = capture.get('parity_changed_fraction' if matched else 'changed_fraction')
    types = []
    if error:
        types.append('capture-invalid')
    elif pct is not None:
        types.append('over-tolerance' if pct > allowed else 'within-tolerance')
    if capture.get('source_pose_collapsed'):
        types.append('pose-collapsed')
    measured = pct is not None and not error
    return dict(id=f'{kind}/{stem}', g=index, area='texture', name=stem,
                subject=Path(str(capture.get('asset', '')).replace('\\', '/')).name,
                phase=str(capture.get('clip', '')), side=capture.get('side', ''),
                source=capture.get('source', ''), game=games.get(capture.get('source'), capture.get('source', '')),
                o=name if matched else None, r='rust-' + name if matched else name,
                d='diff-' + name if matched or measured else None,
                pct=pct if measured else None, valid='invalid' if error else 'valid',
                types=types, detail=[str(error)] if error else [],
                engines='both' if matched else 'rust')


def textures(root, state, config, data):
    games = source_games(config)
    for pair in state['pairs']:
        if pair['kind'] != 'matched-texture':
            continue
        directory = root / pair['original']
        review = read_json(directory / 'review.json')
        group = dict(kind='matched', label=pair['name'], status=pair['status'],
                     dirs={engine: relative(root, directory / 'screenshots') for engine in ['original', 'rust', 'diff']},
                     reports={'original': relative(root, directory / 'report.html')},
                     summary=review.get('comparison_summary'), failures=review.get('failures'),
                     parity_failures=review.get('parity_failures'),
                     allowed=review.get('allowed_changed_fraction'),
                     note='Matched pose and camera. Capture validity and pixel parity are separate checks; '
                          'changed-pixel fractions are not an engine accuracy percentage.')
        if pair['status'] != 'complete':
            group['reason'] = pair.get('reason', 'Original comparison report missing; inspect the capture log.')
        append_textures(root, data, group, review, games, matched=True)
    for kind, directory, label in SWEEPS:
        review = read_json(root / directory / 'review.json')
        if not review:
            continue
        group = dict(kind=kind, label=label, status='complete',
                     dirs={engine: relative(root, root / directory / 'screenshots') for engine in ['rust', 'diff']},
                     reports={'rust': relative(root, root / directory / 'report.html')},
                     failures=review.get('failures'), allowed=review.get('allowed_changed_fraction'),
                     note='Rust captures only. No native reference exists for this sweep; '
                          'changed-pixel fractions appear only when the sweep ran against a Rust reference.')
        append_textures(root, data, group, review, games, matched=False)


def append_textures(root, data, group, review, games, matched):
    index = len(data['groups'])
    data['groups'].append(group)
    allowed = review.get('allowed_changed_fraction') or 0
    for capture in review.get('captures', []):
        if safe_name(capture.get('file', '')):
            data['items'].append(texture_item(group['kind'], index, capture, games, allowed, matched))


def excerpt(root, step):
    if step['status'] in ('passed', 'running'):
        return []
    try:
        lines = (root / step['log']).read_text(errors='replace').splitlines()
    except OSError:
        return []
    marked = [line for line in lines if re.search(r'FAILED|panicked|assertion|NOT READABLE|NOT FOUND|^error', line)]
    return [line.replace(str(root), '.')[:300] for line in (marked or lines)[-12:]]


def artifacts(root):
    names = {'report.html', 'metrics.json', 'comparison.json', 'review.json', 'manifest.json'}
    excluded = {'_original', 'native-work', 'c-tests', 'logs', 'screenshots', 'poses', 'models', 'evidence'}
    found = []
    # Never crawl game copies or Wine prefixes for reports or game metadata.
    for directory, children, files in os.walk(root):
        children[:] = [name for name in children if name not in excluded
                       and not (Path(directory) / name).is_symlink()]
        found.extend(relative(root, Path(directory) / name) for name in files
                     if name in names and Path(directory) != root)
    return sorted(found)


def collect(root, state, config=None):
    """Everything the page shows, as plain JSON-serialisable data."""
    root = Path(root)
    if config is None:
        # Findings and game names refreshed by a later rebuild overlay the run's record.
        config = {**read_json(root / 'config.json'), **read_json(root / OVERLAY)}
    data = dict(schema=2, audit=root.name, interrupted=bool(state.get('interrupted')),
                generated=datetime.now(timezone.utc).strftime('%Y-%m-%d %H:%M UTC'),
                statuses=REVIEW_STATUSES, groups=[], items=[],
                steps=[dict(name=step['name'], status=step['status'], exit_code=step.get('exit_code'),
                            seconds=step.get('seconds', 0), log=step.get('log'), reason=step.get('reason', ''),
                            command=shlex.join(step['argv']), cwd=step.get('cwd', ''),
                            excerpt=excerpt(root, step)) for step in state['steps']],
                related=[dict(path=relative(root, path), exists=Path(path).is_file())
                         for path in state.get('related_reports', [])],
                files=artifacts(root), reviews=read_json(root / 'reviews.json').get('reviews', {}))
    gameplay(root, state, data)
    textures(root, state, config, data)
    data['findings'] = build_findings(root, data, config.get('finding', []))
    return data


def write_report(root, state, config=None):
    root = Path(root)
    data = collect(root, state, config)
    # "</" must not end the script element early; U+2028/9 are not valid in scripts.
    text = json.dumps(data, separators=(',', ':')).replace('</', '<\\/')
    text = text.replace('\u2028', '\\u2028').replace('\u2029', '\\u2029')
    temporary = root / 'report-data.js.tmp'
    temporary.write_text('window.AUDIT=' + text + ';\n')
    temporary.replace(root / 'report-data.js')
    shutil.copyfile(PAGE, root / 'report.html.tmp')
    (root / 'report.html.tmp').replace(root / 'report.html')
    return data
