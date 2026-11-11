"""Synthetic audit integration tests: no game assets, GPU or Wine required."""
import json
from pathlib import Path
import sys
import tempfile
import unittest

from regression_audit import Audit
from regression_audit_report import write_report


class AuditTests(unittest.TestCase):
    def test_complete_capture_compare_pdf_pipeline(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            fake = root / 'fake-engine'
            fake.write_text('#!' + sys.executable + '\n' + '''
import sys, json
from pathlib import Path
args = sys.argv[1:]
def argument(name): return args[args.index(name)+1]
if args[0] == 'run':
    out = Path(argument('--out')) / 'scenario/game/platform/run'
    (out/'screenshots').mkdir(parents=True)
    (out/'screenshots/checkpoint.png').write_bytes(b'synthetic')
    (out/'report.html').write_text('synthetic child report')
    (out/'metrics.json').write_text(json.dumps({'result':'synthetic'}))
elif args[0] == 'compare':
    (Path(args[2])/'comparison.json').write_text(json.dumps({'synthetic': True}))
elif args[0] == 'pdf':
    Path(argument('--out')).write_bytes(b'synthetic PDF')
''')
            fake.chmod(0o755)
            config = dict(portable_c_tests=False, regress=str(fake), pc=str(fake),
                          game=[dict(id='c2', path='unused', original=True,
                                     scenarios=['behaviours'])])
            output = root / 'audit'
            self.assertEqual(Audit(config, output).run(), 0)
            state = json.loads((output / 'audit.json').read_text())
            self.assertEqual([s['status'] for s in state['steps']], ['passed'] * 4)
            self.assertEqual(state['pairs'][0]['status'], 'complete')
            page = (output / 'report.html').read_text()
            self.assertIn('Side-by-side PDF', page)
            self.assertIn('checkpoint.png', page)
            self.assertIn('comparison.json', page)

    def test_failure_continues_and_blocks_dependents_without_reading_console(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / 'audit'
            config = dict(portable_c_tests=False, test=[
                dict(name='failed-test', argv=[sys.executable, '-c', 'print("evidence"); raise SystemExit(7)']),
                dict(name='later-test', argv=[sys.executable, '-c', 'print("later")'])],
                capture_requires=['failed-test'], game=[dict(id='c2', path='missing',
                scenarios=['behaviours'], original=True)])
            self.assertEqual(Audit(config, output).run(), 1)
            state = json.loads((output / 'audit.json').read_text())
            self.assertEqual([s['status'] for s in state['steps']], ['failed', 'passed', 'blocked', 'blocked'])
            self.assertEqual(state['steps'][0]['exit_code'], 7)
            self.assertEqual(state['pairs'][0]['status'], 'missing')
            self.assertIn('evidence', (output / 'logs/failed-test.log').read_text())
            self.assertIn('Expected one Rust', (output / 'report.html').read_text())
            self.assertEqual(state['human_approval'], 'pending')
            with self.assertRaises(FileExistsError):
                Audit(config, output).run()

    def test_timeout_keeps_report_and_runs_next_step(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / 'audit'
            config = dict(portable_c_tests=False, timeout_seconds=.1, test=[
                dict(name='slow', argv=[sys.executable, '-c', 'import time; time.sleep(30)']),
                dict(name='next', argv=[sys.executable, '-c', 'pass'])])
            self.assertEqual(Audit(config, output).run(), 1)
            state = json.loads((output / 'audit.json').read_text())
            self.assertEqual([s['status'] for s in state['steps']], ['timeout', 'passed'])

    def test_pair_gallery_matches_names_and_exposes_unmatched_captures(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for engine in ['original', 'rust']:
                shots = root / engine / 'screenshots'
                shots.mkdir(parents=True)
                (shots / 'same.png').write_bytes(b'synthetic')
                (root / engine / 'report.html').write_text('child unchanged')
            (root / 'rust/screenshots/rust-only.png').write_bytes(b'synthetic')
            state = dict(steps=[], pairs=[dict(name='<pair>', status='complete',
                         kind='gameplay', original='original', rust='rust')])
            write_report(root, state)
            page = (root / 'report.html').read_text()
            self.assertIn('&lt;pair&gt;', page)
            self.assertIn('original/screenshots/same.png', page)
            self.assertIn('rust/screenshots/same.png', page)
            self.assertIn('rust-only.png', page)
            self.assertIn('Missing capture', page)
            self.assertEqual((root / 'rust/report.html').read_text(), 'child unchanged')

    def test_matched_texture_gallery_uses_native_and_rust_captures(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            original = root / 'texture-original-c2'
            (original / 'screenshots').mkdir(parents=True)
            for prefix in ['', 'rust-', 'diff-']:
                (original / 'screenshots' / (prefix + 'face.png')).write_bytes(b'synthetic')
            (original / 'review.json').write_text(json.dumps(dict(captures=[
                dict(file='face.png', source='c2', asset='<face>', clip='idle', side='left'),
                dict(file='../escape.png')])))
            state = dict(steps=[], pairs=[dict(name='native', kind='matched-texture',
                         status='complete', original=original.name, rust='input')])
            write_report(root, state)
            page = (root / 'report.html').read_text()
            self.assertIn('rust-face.png', page)
            self.assertIn('diff-face.png', page)
            self.assertIn('&lt;face&gt;', page)
            self.assertNotIn('src="../escape.png', page)


if __name__ == '__main__':
    unittest.main()
