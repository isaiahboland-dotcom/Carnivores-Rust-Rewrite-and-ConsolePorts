import importlib.util
import json
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest
import zipfile

spec = importlib.util.spec_from_file_location('release_bundle', Path(__file__).with_name('release_bundle.py'))
bundle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bundle)


class BundleTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        subprocess.run(['git', 'init', '-q', str(self.root)], check=True)
        for name in (*bundle.DOCS, 'recipes/bases/c2-vanilla.toml', 'recipes/example.toml'):
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text('synthetic fixture')
        subprocess.run(['git', '-C', str(self.root), 'add', '.'], check=True)
        subprocess.run(['git', '-C', str(self.root), '-c', 'user.name=Test', '-c',
                        'user.email=test@example.invalid', 'commit', '-qm', 'fixture'], check=True)
        for name in ('source/HUNTDAT/private.CAR', 'recipes/instances/private.toml', 'recipes/private.CAR'):
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text('must not distribute')
        self.pc = self.root / 'bin'
        self.pc.mkdir()
        for name in (*bundle.TOOLS, 'c2-cook'):
            for suffix in ('', '.exe'):
                (self.pc / (name + suffix)).write_bytes(b'synthetic binary')

    def test_archives_exclude_private_content_and_preserve_executables(self):
        for platform in ('linux', 'windows'):
            archive = bundle.package(self.root, self.pc, self.pc, platform, self.root / 'out')
            if platform == 'windows':
                with zipfile.ZipFile(archive) as packed:
                    names = packed.namelist()
                    manifest = json.loads(packed.read('manifest.json'))
            else:
                with tarfile.open(archive) as packed:
                    names = packed.getnames()
                    manifest = json.load(packed.extractfile('manifest.json'))
                    self.assertEqual(packed.getmember('c2-launcher').mode, 0o755)
            self.assertEqual(set(manifest['sha256']), set(names) - {'manifest.json'})
            self.assertFalse(any('private' in name or 'source/' in name for name in names))
            self.assertIn('recipes/bases/c2-vanilla.toml', names)

    def test_missing_required_tool_fails(self):
        (self.pc / 'c2-cook').unlink()
        with self.assertRaisesRegex(ValueError, 'c2-cook'):
            bundle.package(self.root, self.pc, self.pc, 'linux', self.root / 'out')

    def test_symlinked_input_fails(self):
        path = self.pc / 'c2-pc'
        path.unlink()
        try:
            path.symlink_to(self.pc / 'c2-cook')
        except OSError:
            self.skipTest('symlinks unavailable')
        with self.assertRaisesRegex(ValueError, 'symlinked'):
            bundle.collect(self.root, self.pc, self.pc, 'linux')


if __name__ == '__main__':
    unittest.main()
