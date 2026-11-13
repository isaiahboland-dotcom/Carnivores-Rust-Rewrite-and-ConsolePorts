"""Synthetic audit integration tests: no game assets, GPU or Wine required."""
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import threading
import unittest
from urllib.error import HTTPError
from urllib.request import Request, urlopen

from regression_audit import Audit
from regression_audit_report import PAGE, collect, write_report
from regression_audit_serve import server


def data(root):
    """The data the page loads, parsed back from report-data.js."""
    text = (root / 'report-data.js').read_text()
    return json.loads(text[len('window.AUDIT='):].rstrip().rstrip(';'))


def run_fixture(root, name, checkpoints, events=(), result='passed'):
    """A synthetic child run: screenshots, metrics and stage events."""
    run = root / name
    (run / 'screenshots').mkdir(parents=True)
    for checkpoint in checkpoints:
        (run / 'screenshots' / (checkpoint + '.png')).write_bytes(b'synthetic')
    (run / 'report.html').write_text('child unchanged')
    (run / 'metrics.json').write_text(json.dumps({'result': result, 'functional': {'stages': {'failed': len(events)}}}))
    (run / 'events.jsonl').write_text(''.join(json.dumps(dict(
        kind='stage_end', stage=stage, name='failed', detail=detail)) + '\n' for stage, detail in events))
    return run


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
                          related_reports=[str(root/'earlier/report.html')],
                          game=[dict(id='c2', path='unused', original=True,
                                     scenarios=['behaviours'])])
            output = root / 'audit'
            self.assertEqual(Audit(config, output).run(), 0)
            state = json.loads((output / 'audit.json').read_text())
            self.assertEqual([s['status'] for s in state['steps']], ['passed'] * 4)
            self.assertEqual(state['pairs'][0]['status'], 'complete')
            self.assertEqual((output / 'report.html').read_text(), PAGE.read_text())
            report = data(output)
            group = report['groups'][0]
            self.assertEqual((group['game'], group['scenario']), ('c2', 'behaviours'))
            self.assertTrue(group['reports']['pdf'].endswith('behaviours-c2.pdf'))
            self.assertTrue(group['reports']['comparison'].endswith('comparison.json'))
            self.assertEqual(group['counts'], dict(paired=1, original_only=0, rust_only=0))
            self.assertEqual({cell['state'] for cell in group['cells'].values()}, {'failed'})
            self.assertIn('run/behaviours/c2/rust', {finding['id'] for finding in report['findings']})
            item = report['items'][0]
            self.assertEqual((item['id'], item['o'], item['r'], item['pct']),
                             ('gameplay/behaviours/c2/checkpoint', 'checkpoint.png', 'checkpoint.png', None))
            self.assertEqual(report['related'], [dict(path='../earlier/report.html', exists=False)])
            self.assertIn('related/', ' '.join(f['id'] for f in report['findings']))

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
            report = data(output)
            self.assertEqual({cell['state'] for cell in report['groups'][0]['cells'].values()}, {'blocked'})
            self.assertEqual([step['excerpt'] for step in report['steps']][0], ['evidence'])
            findings = {finding['id']: finding for finding in report['findings']}
            self.assertEqual(findings['step/failed-test']['severity'], 'failure')
            self.assertIn('capture/behaviours/c2/original', findings)
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

    def test_only_runs_matching_steps_and_capture_prerequisites(self):
        with tempfile.TemporaryDirectory() as tmp:
            step = lambda name: dict(name=name, argv=[sys.executable, '-c', 'pass'])
            config = dict(portable_c_tests=False, test=[step('unit'), step('other')], build=[step('build-engines')],
                          capture_requires=['build-engines'], regress=sys.executable, pc=sys.executable,
                          game=[dict(id='c2', path='unused', scenarios=['behaviours', 'kill_animations'])])
            output = Path(tmp) / 'audit'
            Audit(config, output, only=['kill_animations-*']).run()
            names = [s['name'] for s in json.loads((output / 'audit.json').read_text())['steps']]
            self.assertEqual(names, ['build-engines', 'kill_animations-c2-rust'])

    def test_gameplay_items_pair_by_name_and_keep_results_separate(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            run_fixture(root, 'original', ['weapon_01_pistol_fire', 'weapon_01_pistol_ready', 'kill_01_t_rex_mid'],
                        events=[('kill_01_t_rex', 'killed_by: not released')], result='failed')
            rust = run_fixture(root, 'rust', ['weapon_01_pistol_fire', 'kill_01_t_rex_mid', 'end'])
            (rust / 'comparison.json').write_text(json.dumps({'kill_parity': {'same': False, 'differences': [
                dict(stage='kill_01_t_rex', what='killer_clips', baseline=[7], candidate=[6])]}}))
            state = dict(steps=[], pairs=[dict(name='<pair>', status='complete',
                         kind='gameplay', original='original', rust='rust')])
            write_report(root, state)
            report = data(root)
            items = {item['name']: item for item in report['items']}
            self.assertEqual(report['groups'][0]['counts'], dict(paired=2, original_only=1, rust_only=1))
            self.assertEqual(items['weapon_01_pistol_fire']['types'], [])
            self.assertEqual((items['weapon_01_pistol_ready']['engines'], items['weapon_01_pistol_ready']['r']),
                             ('original', None))
            self.assertEqual(items['weapon_01_pistol_ready']['types'], ['original-only'])
            self.assertEqual(items['end']['types'], ['rust-only'])
            kill = items['kill_01_t_rex_mid']
            self.assertEqual((kill['area'], kill['subject'], kill['phase']), ('kill', 't rex', 'mid'))
            self.assertEqual(kill['types'], ['functional-failure', 'parity-difference'])
            # Gameplay pictures never get an invented pixel percentage.
            self.assertTrue(all(item['pct'] is None and item['valid'] == 'n/a' for item in report['items']))
            ids = {finding['id'] for finding in report['findings']}
            self.assertLessEqual({'stage/<pair>//original/kill_01_t_rex', 'parity/<pair>//kill_01_t_rex',
                                  'unpaired/<pair>/'}, ids)
            self.assertEqual((root / 'rust/report.html').read_text(), 'child unchanged')
            self.assertNotIn('</pair', (root / 'report-data.js').read_text().replace('<\\/', ''))

    def test_rust_only_games_are_unsupported_not_missing_or_passing(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            run_fixture(root, 'gameplay/behaviours-apk-rust/behaviours/apk/pc/run', ['warmup'])
            empty = root / 'gameplay/behaviours-mod-original'
            run_fixture(root, 'gameplay/behaviours-mod-original/behaviours/mod/wine/run', [])
            argv = lambda game, engine, out: ['regress', 'run', '--scenario', 'behaviours.toml', '--game-id', game,
                                              '--engine', engine, '--out', str(out)]
            state = dict(pairs=[], steps=[
                dict(name='behaviours-apk-rust', status='passed', log='a.log',
                     argv=argv('apk', 'rust', root / 'gameplay/behaviours-apk-rust')),
                dict(name='behaviours-mod-original', status='failed', log='b.log', argv=argv('mod', 'original', empty)),
                dict(name='behaviours-mod-rust', status='failed', log='c.log',
                     argv=argv('mod', 'rust', root / 'gameplay/absent'))])
            report = collect(root, state)
            cells = {group['game']: {engine: cell['state'] for engine, cell in group['cells'].items()}
                     for group in report['groups']}
            self.assertEqual(cells['apk'], dict(original='unsupported', rust='completed'))
            self.assertEqual(cells['mod'], dict(original='invalid', rust='missing'))
            self.assertEqual(report['items'][0]['types'], [])
            self.assertEqual(report['groups'][0]['counts'], dict(paired=0, original_only=0, rust_only=0))

    def test_matched_textures_separate_validity_from_pixel_parity(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            original = root / 'texture-original-c2'
            (original / 'screenshots').mkdir(parents=True)
            capture = lambda name, **extra: dict(file=name, source='pc-c2', asset='HUNTDAT/<face>.car',
                                                 clip='idle', side='left', **extra)
            (original / 'review.json').write_text(json.dumps(dict(
                failures=1, parity_failures=1, allowed_changed_fraction=.005,
                comparison_summary=dict(count=2, mean=.1, median=.1, p95=.2, maximum=.2, threshold=.005), captures=[
                    capture('over.png', parity_changed_fraction=.2),
                    capture('within.png', parity_changed_fraction=.001),
                    capture('broken.png', parity_changed_fraction=.3, capture_error='pose mismatch'),
                    capture('../escape.png')])))
            sweep = root / 'texture-compact'
            sweep.mkdir()
            (sweep / 'review.json').write_text(json.dumps(dict(allowed_changed_fraction=.005, captures=[
                dict(file='sweep.png', source='apk', asset='a.cat', clip='walk', side='right',
                     changed_fraction=None, source_pose_collapsed=True)])))
            state = dict(steps=[], pairs=[dict(name='native', kind='matched-texture',
                         status='complete', original=original.name, rust='input')])
            config = dict(original_texture=[dict(id='c2', game='carnivores-2', sources=['pc-c2'])])
            report = collect(root, state, config)
            items = {item['name']: item for item in report['items']}
            self.assertEqual(set(items), {'over', 'within', 'broken', 'sweep'})
            over = items['over']
            self.assertEqual((over['o'], over['r'], over['d']), ('over.png', 'rust-over.png', 'diff-over.png'))
            self.assertEqual((over['game'], over['subject'], over['pct']), ('carnivores-2', '<face>.car', .2))
            self.assertEqual(over['types'], ['over-tolerance'])
            self.assertEqual(items['within']['types'], ['within-tolerance'])
            # An invalid capture has no pixel score, whatever number was recorded.
            self.assertEqual((items['broken']['valid'], items['broken']['pct'], items['broken']['types']),
                             ('invalid', None, ['capture-invalid']))
            self.assertEqual((items['sweep']['engines'], items['sweep']['o'], items['sweep']['pct'], items['sweep']['d']),
                             ('rust', None, None, None))
            self.assertEqual([group['kind'] for group in report['groups']], ['matched', 'compact'])
            findings = {finding['id']: finding for finding in report['findings']}
            self.assertIn('1 of 2 valid captures exceed', findings['pixels/native']['title'])
            self.assertIn('invalid/native', findings)
            self.assertIn('collapsed/Compact-sweep', findings)

    def test_declared_findings_keep_evidence_and_reviews_stay_separate(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / 'audit'
            root.mkdir()
            picture = Path(tmp) / 'elsewhere.png'
            picture.write_bytes(b'evidence')
            reviews = dict(schema=1, reviews={'declared/VIBRANCE': dict(status='investigating', note='n')})
            (root / 'reviews.json').write_text(json.dumps(reviews))
            config = dict(finding=[dict(id='VIBRANCE', title='blue cast', summary='one\ntwo', state='paused',
                          evidence=[dict(label='shot', original=str(picture), rust=str(Path(tmp) / 'gone.png'))])])
            report = write_report(root, dict(steps=[], pairs=[]), config)
            finding = report['findings'][0]
            self.assertEqual((finding['id'], finding['source'], finding['detail']),
                             ('declared/VIBRANCE', 'declared', ['one', 'two']))
            evidence = finding['evidence'][0]
            self.assertEqual((root / evidence['original']).read_bytes(), b'evidence')
            self.assertEqual((evidence['rust'], evidence['missing']), (None, ['rust']))
            self.assertEqual(report['reviews'], reviews['reviews'])
            self.assertEqual(json.loads((root / 'reviews.json').read_text()), reviews)
            self.assertEqual(report['statuses'][0], 'pending')

    def test_review_server_writes_only_reviews(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'audit.json').write_text('{}')
            httpd = server(root)
            threading.Thread(target=httpd.serve_forever, daemon=True).start()
            base = f'http://127.0.0.1:{httpd.server_address[1]}/'
            try:
                body = json.dumps(dict(schema=1, reviews={'a': dict(status='accepted')})).encode()
                self.assertEqual(urlopen(Request(base + 'reviews.json', data=body, method='PUT')).status, 204)
                self.assertEqual(json.loads((root / 'reviews.json').read_text())['reviews']['a']['status'], 'accepted')
                for path, payload in [('audit.json', body), ('reviews.json', b'[]'), ('reviews.json', b'not json')]:
                    with self.assertRaises(HTTPError) as refused:
                        urlopen(Request(base + path, data=payload, method='PUT'))
                    refused.exception.close()
                self.assertEqual((root / 'audit.json').read_text(), '{}')
            finally:
                httpd.shutdown()
                httpd.server_close()

    @unittest.skipUnless(shutil.which('node'), 'node is not installed')
    def test_page_script_parses(self):
        page = PAGE.read_text()
        script = page[page.rindex('<script>') + len('<script>'):page.rindex('</script>')]
        with tempfile.TemporaryDirectory() as tmp:
            (Path(tmp) / 'page.js').write_text(script)
            self.assertEqual(subprocess.run(['node', '--check', 'page.js'], cwd=tmp).returncode, 0)


if __name__ == '__main__':
    unittest.main()
