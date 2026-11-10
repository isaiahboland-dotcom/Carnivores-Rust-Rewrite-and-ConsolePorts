#!/usr/bin/env python3
"""Package only named host tools and tracked recipe/documentation files."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import subprocess
import tarfile
import zipfile

TOOLS = ('c2-launcher', 'c2-pc', 'c2-vr', 'c2-3ds-builder', 'c2-wiiu-builder')
DOCS = ('LICENSE', 'docs/BUNDLE.md', 'docs/WINDOWS.md', 'docs/3DS.md',
        'docs/WIIU.md', 'docs/MODDING.md')


def collect(root, pc, cooker, platform):
    suffix = '.exe' if platform == 'windows' else ''
    files = {name + suffix: pc / (name + suffix) for name in TOOLS}
    files['c2-cook' + suffix] = cooker / ('c2-cook' + suffix)
    files.update({name: root / name for name in DOCS})
    # Never glob a working recipe directory: it can contain personal instances.
    tracked = subprocess.check_output(
        ['git', '-C', str(root), 'ls-files', '-z', '--', 'recipes'],
    ).decode().split('\0')
    files.update({name: root / name for name in tracked if name.endswith('.toml')})
    if 'recipes/bases/c2-vanilla.toml' not in files:
        raise ValueError('Missing tracked base-game hash list')
    for name, path in files.items():
        if path.is_symlink() or not path.is_file():
            raise ValueError(f'Missing or symlinked input: {name}')
        if not path.stat().st_size:
            raise ValueError(f'Empty input: {name}')
    return files


def package(root, pc, cooker, platform, output):
    files = collect(root, pc, cooker, platform)
    payload = {name: path.read_bytes() for name, path in files.items()}
    payload['manifest.json'] = (json.dumps({
        'revision': subprocess.check_output(
            ['git', '-C', str(root), 'rev-parse', 'HEAD'], text=True).strip(),
        'platform': platform,
        'tracked_changes': bool(subprocess.check_output(
            ['git', '-C', str(root), 'diff', 'HEAD', '--name-only'])),
        'sha256': {name: hashlib.sha256(data).hexdigest()
                   for name, data in sorted(payload.items())},
    }, indent=2) + '\n').encode()
    output.mkdir(parents=True, exist_ok=True)
    archive = output / ('windows.zip' if platform == 'windows' else 'linux.tar')
    if platform == 'windows':
        with zipfile.ZipFile(archive, 'w', zipfile.ZIP_DEFLATED) as bundle:
            for name, data in sorted(payload.items()):
                bundle.writestr(name, data)
        with zipfile.ZipFile(archive) as bundle:
            assert bundle.testzip() is None
            assert set(bundle.namelist()) == set(payload)
    else:
        with tarfile.open(archive, 'w') as bundle:
            for name, data in sorted(payload.items()):
                info = tarfile.TarInfo(name)
                info.size = len(data)
                info.mode = 0o755 if '/' not in name and name.startswith('c2-') else 0o644
                bundle.addfile(info, io.BytesIO(data))
        with tarfile.open(archive) as bundle:
            assert set(bundle.getnames()) == set(payload)
            for member in bundle:
                assert bundle.extractfile(member).read() == payload[member.name]
    return archive


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument('--pc', required=True, type=Path)
    parser.add_argument('--cooker', required=True, type=Path)
    parser.add_argument('--platform', required=True, choices=('windows', 'linux'))
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    print(package(args.root, args.pc, args.cooker, args.platform, args.output))


if __name__ == '__main__':
    main()
