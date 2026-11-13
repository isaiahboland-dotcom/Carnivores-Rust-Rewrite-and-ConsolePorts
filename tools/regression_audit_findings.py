"""Findings for the master regression report.

Derived findings restate recorded results (commands, stages, parity, pixel
statistics, coverage); they never judge a difference acceptable. Declared
findings come from `[[finding]]` entries in the private audit configuration
and keep their evidence pictures inside the audit directory.
"""
from pathlib import Path
import re
import shutil

IMAGES = {'.png', '.jpg', '.jpeg', '.webp', '.bmp'}


def slug(text):
    return re.sub(r'[^A-Za-z0-9_-]+', '-', str(text)).strip('-') or 'finding'


def keep_evidence(root, finding, source, label):
    """Copy one evidence picture into the audit so it outlives the run it came from."""
    source = Path(str(source).replace('{repo}', str(Path(__file__).resolve().parents[1]))
                  .replace('{audit}', str(root)))
    if source.suffix.lower() not in IMAGES or not source.is_file():
        return None
    target = root / 'evidence' / slug(finding) / f'{slug(label)}{source.suffix.lower()}'
    if not target.is_file() or target.stat().st_size != source.stat().st_size:
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)
    return str(target.relative_to(root)).replace('\\', '/')


def declared(root, entries):
    for entry in entries:
        identifier = str(entry.get('id', entry.get('title', 'finding')))
        evidence = []
        for number, pair in enumerate(entry.get('evidence', [])):
            item = dict(label=pair.get('label', ''))
            for role in ['image', 'original', 'rust']:
                if pair.get(role):
                    item[role] = keep_evidence(root, identifier, pair[role], f'{number}-{role}')
                    if item[role] is None:
                        item['missing'] = item.get('missing', []) + [role]
            evidence.append(item)
        yield dict(id='declared/' + slug(identifier), source='declared', severity=entry.get('severity', 'difference'),
                   title=f'{identifier}: {entry.get("title", "")}'.rstrip(': '),
                   detail=[line for line in str(entry.get('summary', '')).splitlines() if line.strip()],
                   state=entry.get('state', ''), references=list(entry.get('references', [])),
                   evidence=evidence, view=entry.get('view'))


def derived(data):
    for step in data['steps']:
        if step['status'] not in ('passed', 'running'):
            yield dict(id='step/' + step['name'], source='derived', severity='failure',
                       title=f'Command {step["status"]}: {step["name"]}',
                       detail=[step['reason']] if step['reason'] else step['excerpt'],
                       links=[dict(label='Log', href=step['log'])] if step['log'] else [])
    for related in data['related']:
        if not related['exists']:
            yield dict(id='related/' + slug(related['path']), source='derived', severity='coverage',
                       title='Related audit report is missing',
                       detail=[related['path'], 'Results cited from that audit can no longer be inspected here.'])
    for index, group in enumerate(data['groups']):
        items = [item for item in data['items'] if item['g'] == index]
        view = dict(tab=group['kind'], group=index)
        if group['kind'] == 'gameplay':
            yield from gameplay(group, items, view)
        else:
            yield from textures(group, items, view)


def gameplay(group, items, view):
    scope = f'{group["scenario"]} · {group["game"]}'
    key = f'{group["scenario"]}/{group["game"]}'
    for engine, cell in group['cells'].items():
        stages = group['failed'].get(engine, {})
        if cell['state'] in ('missing', 'invalid', 'blocked'):
            # A run without captures is one coverage gap, not one finding per stage.
            yield dict(id=f'capture/{key}/{engine}', source='derived', severity='failure',
                       title=f'{scope}: {engine} capture {cell["state"]}',
                       detail=[cell.get('detail', '')] + [f'{stage}: {why}' for stage, why in stages.items()],
                       links=[dict(label='Log', href=cell['log'])] if cell.get('log') else [], view=view)
            continue
        if cell['state'] == 'failed' and not stages:
            # Every stage passed, so the run failed on a whole-run check such as determinism.
            yield dict(id=f'run/{key}/{engine}', source='derived', severity='failure',
                       title=f'{scope}: {engine} run failed without a failed stage',
                       detail=[cell.get('detail', '')], view=view)
        for stage, why in stages.items():
            yield dict(id=f'stage/{key}/{engine}/{stage}', source='derived', severity='failure',
                       title=f'{scope}: {engine} stage {stage} failed', detail=[why],
                       view=dict(view, query=stage))
    for stage, differences in group['parity'].items():
        yield dict(id=f'parity/{key}/{stage}', source='derived', severity='difference',
                   title=f'{scope}: original and Rust differ in {stage}', detail=differences,
                   view=dict(view, query=stage))
    counts = group.get('counts', {})
    unpaired = counts.get('original_only', 0) + counts.get('rust_only', 0)
    if unpaired:
        yield dict(id=f'unpaired/{key}', source='derived', severity='coverage',
                   title=f'{scope}: checkpoints captured by one engine only: {unpaired}',
                   detail=[f'{counts["paired"]} paired, {counts["original_only"]} original only, '
                           f'{counts["rust_only"]} Rust only. Unpaired checkpoints are not comparisons.',
                           'Original only: ' + ', '.join(i['name'] for i in items if i['engines'] == 'original'),
                           'Rust only: ' + ', '.join(i['name'] for i in items if i['engines'] == 'rust')],
                   view=dict(view, engines='unpaired'))


def textures(group, items, view):
    if group.get('reason'):
        yield dict(id=f'capture/{slug(group["label"])}', source='derived', severity='coverage',
                   title=f'{group["label"]}: comparison incomplete', detail=[group['reason']], view=view)
    invalid = [item for item in items if item['valid'] == 'invalid']
    if invalid:
        yield dict(id=f'invalid/{slug(group["label"])}', source='derived', severity='failure',
                   title=f'{group["label"]}: {len(invalid)} invalid captures',
                   detail=sorted({line for item in invalid for line in item['detail']})[:8],
                   view=dict(view, types='capture-invalid'))
    over = [item for item in items if 'over-tolerance' in item['types']]
    if over:
        measured = [item['pct'] for item in items if item['pct'] is not None]
        yield dict(id=f'pixels/{slug(group["label"])}', source='derived', severity='difference',
                   title=f'{group["label"]}: {len(over)} of {len(measured)} valid captures exceed the pixel tolerance',
                   detail=[f'Allowed changed pixels {group.get("allowed") or 0:.2%}; '
                           f'largest {max(measured):.2%}, mean {sum(measured) / len(measured):.2%}.',
                           'Changed-pixel fractions, equal weight per valid capture; not an accuracy percentage.'],
                   view=dict(view, types='over-tolerance', sort='pct-desc'))
    collapsed = sum('pose-collapsed' in item['types'] for item in items)
    if collapsed:
        yield dict(id=f'collapsed/{slug(group["label"])}', source='derived', severity='difference',
                   title=f'{group["label"]}: {collapsed} captures have a collapsed source pose',
                   detail=[], view=dict(view, types='pose-collapsed'))


def build_findings(root, data, entries):
    return list(declared(Path(root), entries)) + list(derived(data))
