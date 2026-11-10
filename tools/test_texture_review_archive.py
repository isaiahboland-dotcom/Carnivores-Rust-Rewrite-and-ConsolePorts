"""Private media lifecycle tests using synthetic files only."""
import importlib.util
import json
from pathlib import Path
import tempfile
import subprocess
import sys
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('review_archive', Path(__file__).with_name('texture-review-archive.py'))
archive = importlib.util.module_from_spec(spec)
spec.loader.exec_module(archive)


class ArchiveTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.run = self.root / 'run'
        (self.run / 'screenshots').mkdir(parents=True)
        (self.run / 'models').mkdir()
        (self.run / '.gitignore').write_text('*\n')
        (self.run / 'screenshots' / 'match.png').write_bytes(b'matching pixels' * 100)
        (self.run / 'screenshots' / 'duplicate.png').write_bytes(b'matching pixels' * 100)
        (self.run / 'screenshots' / 'miss.png').write_bytes(b'mismatching pixels')
        (self.run / 'screenshots' / 'baseline-miss.png').write_bytes(b'reference pixels')
        (self.run / 'screenshots' / 'diff-miss.png').write_bytes(b'diff pixels')
        (self.run / 'models' / 'animal.car').write_bytes(bytes(range(256)) * 1000)
        (self.run / 'report.html').write_text('complete original gallery')
        (self.run / 'review.json').write_text(json.dumps({
            'kind': 'private-texture-review', 'allowed_changed_fraction': .005,
            'captures': [
                {'file': 'match.png', 'status': 'within tolerance', 'changed_fraction': 0},
                {'file': 'duplicate.png', 'status': 'unreviewed'},
                {'file': 'miss.png', 'status': 'changed', 'changed_fraction': .02},
            ],
        }))
        self.original = {p.relative_to(self.run).as_posix(): p.read_bytes()
                         for p in self.run.rglob('*') if p.is_file()}
        self.target = self.root / 'archives' / 'review.tar.xz'

    def test_verified_archive_prunes_only_matching_rust_media_and_restores_every_byte(self):
        receipt = archive.archive_run(self.run, self.target, 'Synthetic reviewer', prune=True)
        self.assertGreater(receipt['pruned_bytes'], 0)
        self.assertFalse((self.run / 'models' / 'animal.car').exists())
        self.assertFalse((self.run / 'screenshots' / 'match.png').exists())
        for name in ['miss.png', 'baseline-miss.png', 'diff-miss.png']:
            self.assertTrue((self.run / 'screenshots' / name).exists())
        restored = archive.restore_run(self.target, self.root / 'restored')
        for name, expected in self.original.items():
            self.assertEqual((restored / name).read_bytes(), expected, name)
        (restored / 'screenshots' / 'match.png').write_bytes(b'edited')
        self.assertEqual((restored / 'screenshots' / 'duplicate.png').read_bytes(), b'matching pixels' * 100)

    def test_verification_failure_never_prunes_or_publishes(self):
        with patch.object(archive, 'verify', side_effect=ValueError('corruption')):
            with self.assertRaises(ValueError):
                archive.archive_run(self.run, self.target, 'Synthetic reviewer', prune=True)
        self.assertFalse(self.target.exists())
        for name, expected in self.original.items():
            self.assertEqual((self.run / name).read_bytes(), expected)

    def test_original_references_cannot_be_pruned(self):
        with self.assertRaises(ValueError):
            archive.archive_run(self.run, self.target, 'Synthetic reviewer', prune=True, role='original')
        self.assertFalse(self.target.exists())
        archive.archive_run(self.run, self.target, 'Synthetic reviewer', role='original')
        for name, expected in self.original.items():
            self.assertEqual((self.run / name).read_bytes(), expected)

    def test_corrupt_archive_and_existing_restore_destination_are_rejected(self):
        archive.archive_run(self.run, self.target, 'Synthetic reviewer')
        with self.assertRaises(ValueError):
            archive.restore_run(self.target, self.run)
        with self.target.open('ab') as f:
            f.write(b'corruption')
        with self.assertRaises(ValueError):
            archive.restore_run(self.target, self.root / 'restored')
        self.assertFalse((self.root / 'restored').exists())

    def test_keep_typo_or_symlink_never_prunes(self):
        with self.assertRaises(ValueError):
            archive.archive_run(self.run, self.target, 'Synthetic reviewer', prune=True, keep=['missing.png'])
        self.assertTrue((self.run / 'screenshots' / 'match.png').exists())
        (self.run / 'link').symlink_to(self.root)
        with self.assertRaises(ValueError):
            archive.archive_run(self.run, self.root / 'other.tar.xz', 'Synthetic reviewer')

    def baseline_metadata(self):
        path = self.run / 'review.json'
        review = json.loads(path.read_text())
        review.update(engine_commit='a' * 40, engine_dirty=False, failures=0,
                      asset_type='texture-sweep')
        for capture in review['captures']:
            capture.update(source='c2', asset='HUNTDAT/ALLO.CAR', animation=0,
                           scenario='isolated-model-v1', phase=.33, side='left',
                           recipe={'schema': 1, 'time_ms': 330})
        path.write_text(json.dumps(review))

    def test_approved_identity_survives_archive_and_restore(self):
        self.baseline_metadata()
        archive.archive_run(self.run, self.target, 'Synthetic reviewer',
                            asset_type='texture-sweep', baseline_version='v1')
        index = json.loads(archive.index_path(self.target).read_text())
        self.assertEqual(index['baselines'][0]['identity'], dict(
            game='c2', model='HUNTDAT/ALLO.CAR', animation=0,
            scenario='isolated-model-v1', engine_commit='a' * 40,
            baseline_version='v1'))
        self.assertEqual(index['baselines'][0]['recipe'], {'schema': 1, 'time_ms': 330})
        restored = archive.restore_run(self.target, self.root / 'restored')
        self.assertEqual(json.loads((restored / 'review.json').read_text())['engine_commit'], 'a' * 40)

    def test_animation_baselines_require_the_exact_animation_model(self):
        self.baseline_metadata()
        path = self.run / 'review.json'
        review = json.loads(path.read_text())
        review['asset_type'] = 'animation-archive'
        path.write_text(json.dumps(review))
        with self.assertRaisesRegex(ValueError, 'retain'):
            archive.archive_run(self.run, self.target, 'Synthetic reviewer',
                                asset_type='animation-archive', baseline_version='v1')
        for capture in review['captures']:
            capture.update(model_file='models/animal.car',
                           car_sha256=archive.digest(self.run / 'models/animal.car'))
        path.write_text(json.dumps(review))
        archive.archive_run(self.run, self.target, 'Synthetic reviewer',
                            asset_type='animation-archive', baseline_version='v1')
        restored = archive.restore_run(self.target, self.root / 'restored')
        self.assertEqual((restored / 'models/animal.car').read_bytes(),
                         (self.run / 'models/animal.car').read_bytes())

    def test_original_baseline_requires_native_pose_and_executable_provenance(self):
        self.baseline_metadata()
        path = self.run / 'review.json'
        review = json.loads(path.read_text())
        review.update(engine='original-software', asset_type='original-parity')
        path.write_text(json.dumps(review))
        options = dict(asset_type='original-parity', baseline_version='v1')
        with self.assertRaisesRegex(ValueError, 'provenance'):
            archive.archive_run(self.run, self.target, 'Synthetic reviewer', **options)
        review.update(matched_capture_schema=1, original_executable_sha256='b' * 64)
        for capture in review['captures']:
            capture.update(native_pose_max_error=0, native_pose_allowed_error=.001,
                           original_executable_sha256='b' * 64)
        path.write_text(json.dumps(review))
        archive.archive_run(self.run, self.target, 'Synthetic reviewer', **options)
        index = json.loads(archive.index_path(self.target).read_text())
        self.assertEqual(index['baselines'][0]['original_executable_sha256'], 'b' * 64)
        self.assertEqual(index['role'], 'original')

    def test_registered_identity_cannot_be_reused_in_archive_directory(self):
        self.baseline_metadata()
        archive.archive_run(self.run, self.target, 'Synthetic reviewer',
                            asset_type='texture-sweep', baseline_version='v1')
        restored = archive.restore_run(self.target, self.root / 'restored')
        with self.assertRaisesRegex(ValueError, 'already registered'):
            archive.archive_run(restored, self.target.parent / 'duplicate.tar.xz',
                                'Synthetic reviewer', asset_type='texture-sweep', baseline_version='v1')
        archive.archive_run(restored, self.target.parent / 'v2.tar.xz',
                            'Synthetic reviewer', asset_type='texture-sweep', baseline_version='v2')

    def test_find_filters_identity_without_extracting_media(self):
        self.baseline_metadata()
        archive.archive_run(self.run, self.target, 'Synthetic reviewer',
                            asset_type='texture-sweep', baseline_version='v1')
        command = [sys.executable, str(Path(archive.__file__)), 'find', str(self.target.parent),
                   '--game', 'c2', '--animation', '0', '--baseline-version', 'v1']
        found = subprocess.run(command, check=True, capture_output=True, text=True)
        entries = [json.loads(line) for line in found.stdout.splitlines()]
        self.assertEqual(len(entries), 3)
        self.assertEqual(entries[0]['identity']['model'], 'HUNTDAT/ALLO.CAR')
        missing = subprocess.run(command + ['--asset-type', 'original-parity'],
                                 check=True, capture_output=True, text=True)
        self.assertEqual(missing.stdout, '')

    def test_baseline_requires_version_provenance_and_correct_engine(self):
        self.baseline_metadata()
        for options in [dict(asset_type='texture-sweep'),
                        dict(baseline_version='v1'),
                        dict(asset_type='original-parity', baseline_version='v1')]:
            with self.assertRaises(ValueError):
                archive.archive_run(self.run, self.target, 'Synthetic reviewer', **options)
            self.assertFalse(self.target.exists())
        review = json.loads((self.run / 'review.json').read_text())
        review['engine_dirty'] = True
        (self.run / 'review.json').write_text(json.dumps(review))
        with self.assertRaises(ValueError):
            archive.archive_run(self.run, self.target, 'Synthetic reviewer',
                                asset_type='texture-sweep', baseline_version='v1')

    def test_empty_reviewer_is_rejected_and_paths_cannot_escape(self):
        with self.assertRaises(ValueError):
            archive.archive_run(self.run, self.target, '', prune=True)
        for bad in ['../escape', '/absolute', 'C:/escape', 'a\\..\\escape']:
            with self.assertRaises(ValueError):
                archive.safe_name(bad)


if __name__ == '__main__':
    unittest.main()
