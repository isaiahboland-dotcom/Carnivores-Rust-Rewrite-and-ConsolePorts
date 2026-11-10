#!/usr/bin/env python3
"""Lossless, verified local texture-review archives. No network or uploads.

Archive after human review, optionally pruning Rust media except mismatches.
Original-engine references are never pruned. Restore into a NEW directory.
Uses Python's solid tar.xz compression; PNGs are already losslessly compressed.
"""
import argparse
import hashlib
import html
import json
import math
import os
from pathlib import Path, PurePosixPath
import shutil
import tarfile
import tempfile


def digest(path):
    h = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def safe_name(name):
    p = PurePosixPath(name)
    if p.is_absolute() or '..' in p.parts or '\\' in name or ':' in name:
        raise ValueError(f'unsafe archive path: {name}')
    return p


def verify(archive, index):
    """Stream every payload through SHA-256; validate links without extracting."""
    seen = {}
    with tarfile.open(archive, 'r|xz') as tar:
        for item in tar:
            safe_name(item.name)
            if item.name not in index['files'] or item.name in seen:
                raise ValueError(f'unexpected/duplicate archive member: {item.name}')
            expected = index['files'][item.name]
            if item.islnk():
                safe_name(item.linkname)
                actual = seen.get(item.linkname)
                if actual is None:
                    raise ValueError(f'forward/unknown hard link: {item.linkname}')
            elif item.isfile():
                h = hashlib.sha256()
                size = 0
                with tar.extractfile(item) as stream:
                    for block in iter(lambda: stream.read(1024 * 1024), b''):
                        size += len(block)
                        h.update(block)
                actual = {'size': size, 'sha256': h.hexdigest()}
            else:
                raise ValueError(f'unsupported archive member: {item.name}')
            if actual != expected:
                raise ValueError(f'archive payload does not match index: {item.name}')
            seen[item.name] = actual
    if seen.keys() != index['files'].keys():
        raise ValueError('archive is incomplete')
    return digest(archive)


def index_path(archive):
    return Path(str(archive) + '.index.json')


def archive_run(run, archive, reviewer, notes='', prune=False, role='rust', keep=(),
                asset_type=None, baseline_version=None):
    run, archive = run.resolve(), archive.resolve()
    if not archive.name.endswith('.tar.xz'):
        raise ValueError('archive output must end in .tar.xz')
    if archive == run or run in archive.parents:
        raise ValueError('archive must be outside the run being archived')
    if archive.exists() or index_path(archive).exists():
        raise ValueError('archive/index already exists; choose a new name')
    review_path = run / 'review.json'
    review = json.loads(review_path.read_text())
    if review.get('kind') != 'private-texture-review':
        raise ValueError('not a texture-review run')
    if not reviewer.strip():
        raise ValueError('an explicit human reviewer is required')
    original = role == 'original' or review.get('engine', 'rust') != 'rust'
    if original and prune:
        raise ValueError('original-engine reference images must remain on disk')
    if (run / 'archive.json').exists():
        raise ValueError('run already archived; restore the full run to archive again')
    baselines = []
    if asset_type or baseline_version:
        if asset_type not in ('texture-sweep', 'animation-archive', 'original-parity'):
            raise ValueError('choose an explicit asset type')
        if review.get('asset_type') != asset_type:
            raise ValueError('asset type must match the recorded capture coverage')
        if not baseline_version or not baseline_version.strip():
            raise ValueError('baseline version is required')
        if asset_type == 'original-parity' and review.get('engine', 'rust') == 'rust':
            raise ValueError('original parity requires an original-engine run')
        if asset_type == 'original-parity':
            native_hash = review.get('original_executable_sha256', '')
            if (review.get('matched_capture_schema') != 1 or len(native_hash) != 64
                    or any(c not in '0123456789abcdef' for c in native_hash)):
                raise ValueError('original parity requires validated native-capture provenance')
        if not review.get('captures'):
            raise ValueError('an approved baseline must contain captures')
        if review.get('failures', 0):
            raise ValueError('a failed run cannot become an approved baseline')
        commit = review.get('engine_commit')
        if (not isinstance(commit, str) or len(commit) != 40
                or any(c not in '0123456789abcdef' for c in commit)
                or review.get('engine_dirty', True)):
            raise ValueError('approved baselines require a recorded clean engine commit')
        model_hashes = {}
        for capture in review['captures']:
            if capture.get('capture_error') or capture.get('status') == 'invalid capture':
                raise ValueError('invalid captures cannot become approved baselines')
            if asset_type == 'original-parity':
                error, allowed_error = capture.get('native_pose_max_error'), capture.get('native_pose_allowed_error')
                if (not isinstance(error, (int, float)) or not isinstance(allowed_error, (int, float))
                        or not math.isfinite(error) or not math.isfinite(allowed_error)
                        or not 0 <= error <= allowed_error
                        or capture.get('original_executable_sha256') != native_hash):
                    raise ValueError('original parity requires checked native pose agreement')
            recipe = capture.get('recipe')
            if not isinstance(recipe, dict) or recipe.get('schema') != 1:
                raise ValueError('approved captures require a supported replay recipe')
            if asset_type == 'animation-archive':
                model_file = capture.get('model_file')
                if not model_file or not (run / safe_name(model_file)).is_file():
                    raise ValueError('animation archives must retain their serialized animation models')
                if model_file not in model_hashes:
                    model_hashes[model_file] = digest(run / model_file)
                if model_hashes[model_file] != capture.get('car_sha256'):
                    raise ValueError('animation model does not match capture hash')
            identity = dict(game=capture['source'], model=capture['asset'],
                            animation=capture['animation'],
                            scenario=capture['scenario'], engine_commit=commit,
                            baseline_version=baseline_version)
            baselines.append(dict(identity=identity, asset_type=asset_type,
                                  file=capture['file'], clip=capture.get('clip'), phase=capture['phase'],
                                  side=capture['side'], recipe=capture.get('recipe'),
                                  car_sha256=capture.get('car_sha256'),
                                  model_file=capture.get('model_file'), pose_file=capture.get('pose_file'),
                                  native_pose_max_error=capture.get('native_pose_max_error'),
                                  original_executable_sha256=capture.get('original_executable_sha256')))
    # Refuse to reuse a versioned identity in this private archive directory.
    # Multiple sides/phases in one archive intentionally share an identity.
    identities = {json.dumps([b['asset_type'], b['identity']], sort_keys=True)
                  for b in baselines}
    if identities and archive.parent.exists():
        for existing_index in archive.parent.glob('*.tar.xz.index.json'):
            prior = json.loads(existing_index.read_text())
            if any(json.dumps([b['asset_type'], b['identity']], sort_keys=True) in identities
                   for b in prior.get('baselines', [])):
                raise ValueError(f'baseline identity already registered: {existing_index}; choose a new version')
    files = {}
    for path in sorted(run.rglob('*')):
        if path.is_symlink():
            raise ValueError(f'symlink in run: {path}')
        if path.is_file():
            name = path.relative_to(run).as_posix()
            safe_name(name)
            files[name] = {'size': path.stat().st_size, 'sha256': digest(path)}
    # Original report/media/metadata are archived verbatim before local pruning.
    index = {'schema': 2, 'format': 'tar.xz', 'private': True,
             'reviewed_by': reviewer, 'review_notes': notes,
             'role': 'original' if original else 'rust', 'files': files,
             'baselines': baselines,
             'human_approval': 'approved' if baselines else 'review-recorded'}
    archive.parent.mkdir(parents=True, exist_ok=True)
    # Add narrow privacy patterns even outside the usual ignored recordings root.
    ignore = archive.parent / '.gitignore'
    existing = ignore.read_text() if ignore.exists() else ''
    patterns = ['*.tar.xz', '*.tar.xz.index.json', '*.tar.xz.partial']
    missing = [p for p in patterns if p not in existing.splitlines()]
    if missing:
        with ignore.open('a') as stream:
            stream.write(('\n' if existing and not existing.endswith('\n') else '') + '\n'.join(missing) + '\n')
    temporary = Path(str(archive) + '.partial')
    if temporary.exists():
        raise ValueError('a partial archive already exists')
    # Content-identical files become tar hard links: lossless deduplication,
    # particularly useful for duplicate animation frames and model copies.
    known = {}
    try:
        with tarfile.open(temporary, 'w|xz', preset=6, format=tarfile.PAX_FORMAT) as tar:
            for name, info in files.items():
                path = run / name
                if digest(path) != info['sha256']:
                    raise ValueError(f'run changed during archiving: {name}')
                member = tar.gettarinfo(str(path), arcname=name)
                key = (info['size'], info['sha256'])
                if key in known:
                    member.type = tarfile.LNKTYPE
                    member.linkname = known[key]
                    member.size = 0
                    tar.addfile(member)
                else:
                    known[key] = name
                    with path.open('rb') as stream:
                        tar.addfile(member, stream)
        index['archive_sha256'] = verify(temporary, index)
        # Recheck ALL files before publishing the archive or pruning anything.
        for name, info in files.items():
            if digest(run / name) != info['sha256']:
                raise ValueError(f'run changed during archive verification: {name}')
        with temporary.open('rb') as stream:
            os.fsync(stream.fileno())
        # An atomic hard link refuses an existing archive, including races.
        current = {p.relative_to(run).as_posix() for p in run.rglob('*') if p.is_file()}
        if current != files.keys():
            raise ValueError('files were added or removed during archiving')
        os.link(temporary, archive)
        temporary.unlink()
        with index_path(archive).open('x') as stream:
            stream.write(json.dumps(index, indent=2) + '\n')
            stream.flush()
            os.fsync(stream.fileno())
    except BaseException:
        temporary.unlink(missing_ok=True)
        raise
    retained = set(keep)
    known_captures = {c['file'] for c in review['captures']}
    if not retained <= known_captures:
        # The archive is safe, but no pruning is allowed with a mistyped keep list.
        raise ValueError('a --keep capture is not in this run; archive saved, no files pruned')
    for capture in review['captures']:
        if (capture.get('status') in ('changed', 'invalid capture', 'baseline missing/unreadable')
                or capture.get('capture_error')
                or (capture.get('changed_fraction') or 0) > review.get('allowed_changed_fraction', 0)):
            retained.add(capture['file'])
    kept_media = set()
    for name in retained:
        safe_name(name)
        for candidate in (name, 'diff-' + name, 'baseline-' + name):
            kept_media.add('screenshots/' + candidate)
    removed_bytes = 0
    if prune:
        for name, info in files.items():
            if (name.startswith('screenshots/') and name not in kept_media) or name.startswith('models/'):
                # Refuse to remove a file modified since verification.
                if digest(run / name) != info['sha256']:
                    raise ValueError(f'file changed before prune: {name}; stop, archive is safe')
                (run / name).unlink()
                removed_bytes += info['size']
        # Replace the local gallery so it never silently points at pruned images.
        # The original, complete gallery is preserved inside the archive.
        report = ['<!doctype html><meta charset=utf-8><title>Archived texture review</title>',
                  '<h1>Archived texture review</h1>',
                  '<p>Full gallery and media are in the verified lossless archive. '
                  'Restore with texture-review-archive.py to review all captures.</p>',
                  f'<p>Archive: {html.escape(str(archive))}</p>',
                  f'<p>Reviewed by: {html.escape(reviewer)}. Retained {len(retained)} captures for review.</p>']
        for name in sorted(retained):
            report.append(f'<h2>{html.escape(name)}</h2>')
            for candidate in (name, 'baseline-' + name, 'diff-' + name):
                path = run / 'screenshots' / candidate
                if path.exists():
                    report.append(f'<img style="max-width:46%" src="screenshots/{html.escape(candidate, quote=True)}">')
        (run / 'report.html').write_text('\n'.join(report))
    receipt = {k: v for k, v in index.items() if k != 'files'}
    receipt.update(archive=str(archive), file_count=len(files),
                   original_bytes=sum(f['size'] for f in files.values()),
                   archive_bytes=archive.stat().st_size, pruned_bytes=removed_bytes,
                   retained_captures=sorted(retained), pruned=prune)
    (run / 'archive.json').write_text(json.dumps(receipt, indent=2) + '\n')
    return receipt


def restore_run(archive, destination):
    archive, destination = archive.resolve(), destination.resolve()
    index = json.loads(index_path(archive).read_text())
    if digest(archive) != index['archive_sha256']:
        raise ValueError('archive checksum mismatch')
    verify(archive, index)
    if destination.exists():
        raise ValueError('restore destination must be a new directory')
    destination.parent.mkdir(parents=True, exist_ok=True)
    # Never expose a partial restore as a completed run.
    with tempfile.TemporaryDirectory(prefix='.texture-restore-', dir=destination.parent) as temporary:
        temp = Path(temporary)
        with tarfile.open(archive, 'r|xz') as tar:
            for item in tar:
                name = safe_name(item.name)
                path = temp.joinpath(*name.parts)
                path.parent.mkdir(parents=True, exist_ok=True)
                if item.islnk():
                    target = temp.joinpath(*safe_name(item.linkname).parts)
                    # Copy rather than link restored files: editing one capture
                    # must not modify a content-identical but unrelated capture.
                    shutil.copyfile(target, path)
                elif item.isfile():
                    with tar.extractfile(item) as source, path.open('xb') as out:
                        shutil.copyfileobj(source, out, 1024 * 1024)
                else:
                    raise ValueError('unsupported member')
        for name, info in index['files'].items():
            if digest(temp / name) != info['sha256']:
                raise ValueError(f'restored checksum mismatch: {name}')
        (temp / '.gitignore').write_text('*\n')
        os.rename(temp, destination)
    return destination


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    archive = commands.add_parser('archive')
    archive.add_argument('run', type=Path)
    archive.add_argument('--out', type=Path, required=True, help='new .tar.xz archive path')
    archive.add_argument('--reviewed-by', required=True, help='human reviewer, supplied after review')
    archive.add_argument('--notes', default='')
    archive.add_argument('--role', choices=['rust', 'original'], default='rust')
    archive.add_argument('--prune', action='store_true', help='after verification remove Rust models and matching captures')
    archive.add_argument('--keep', action='append', default=[], help='capture filename to retain for further review')
    archive.add_argument('--asset-type', choices=['texture-sweep', 'animation-archive', 'original-parity'])
    archive.add_argument('--baseline-version', help='explicit approved version, e.g. v1')
    find = commands.add_parser('find', help='find approved captures in private archive indexes')
    find.add_argument('directory', type=Path)
    for field in ('game', 'model', 'animation', 'scenario', 'engine-commit', 'baseline-version', 'asset-type'):
        find.add_argument('--' + field)
    restore = commands.add_parser('restore')
    restore.add_argument('archive', type=Path)
    restore.add_argument('--out', type=Path, required=True)
    check = commands.add_parser('verify')
    check.add_argument('archive', type=Path)
    args = parser.parse_args()
    if args.command == 'archive':
        print(json.dumps(archive_run(args.run, args.out, args.reviewed_by, args.notes, args.prune, args.role, args.keep, args.asset_type, args.baseline_version), indent=2))
    elif args.command == 'find':
        for path in sorted(args.directory.rglob('*.tar.xz.index.json')):
            index = json.loads(path.read_text())
            for entry in index.get('baselines', []):
                values = dict(entry['identity'], asset_type=entry['asset_type'])
                if all(getattr(args, key) is None or str(values[key]) == getattr(args, key)
                       for key in values):
                    print(json.dumps(dict(index=str(path), **entry)))
    elif args.command == 'restore':
        print(restore_run(args.archive, args.out))
    else:
        index = json.loads(index_path(args.archive).read_text())
        if verify(args.archive, index) != index['archive_sha256']:
            raise ValueError('archive checksum mismatch')
        print('Verified every archived file and archive SHA-256.')


if __name__ == '__main__':
    main()
