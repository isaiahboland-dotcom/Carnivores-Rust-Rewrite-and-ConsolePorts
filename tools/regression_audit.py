#!/usr/bin/env python3
"""Run a private regression audit and build its master report.

Run:      regression_audit.py --config source/regression-audit.toml [--only PATTERN]
Rebuild:  regression_audit.py --report AUDIT_DIR|latest
Review:   regression_audit.py --serve AUDIT_DIR|latest
"""
import argparse
from datetime import datetime, timezone
from fnmatch import fnmatch
import json
import os
from pathlib import Path
import re
import shlex
import signal
import subprocess
import time
import tomllib
import uuid

from regression_audit_report import OVERLAY, write_report


REPO = Path(__file__).resolve().parents[1]


def expand(value, variables):
    # Replace only our tokens: command arguments can contain unrelated braces.
    for key, replacement in variables.items():
        value = value.replace('{' + key + '}', str(replacement))
    return value


class Audit:
    def __init__(self, config, output, repo=REPO, only=()):
        self.config, self.output, self.repo = config, output.resolve(), repo
        self.only = list(only)
        self.needed = {*config.get('capture_requires', []), *config.get('original_requires', [])}
        self.variables = dict(repo=repo, audit=self.output)
        self.steps, self.pairs = [], []
        self.interrupted = False

    def save(self):
        state = dict(schema=1, human_approval='pending', steps=self.steps,
                     pairs=self.pairs, interrupted=self.interrupted,
                     related_reports=[expand(path, self.variables)
                                      for path in self.config.get('related_reports', [])])
        (self.output / 'audit.json').write_text(json.dumps(state, indent=2) + '\n')
        write_report(self.output, state, self.config)

    def selected(self, *names):
        """Whether --only keeps a step; capture prerequisites always run."""
        return not self.only or any(name in self.needed or fnmatch(name, pattern)
                                    for name in names for pattern in self.only)

    def command(self, name, argv, cwd=None, env=None, requires=()):
        if not re.fullmatch(r'[A-Za-z0-9_-]+', name):
            raise ValueError(f'invalid step name: {name}')
        if any(step['name'] == name for step in self.steps):
            raise ValueError(f'duplicate step: {name}')
        argv = [expand(str(arg), self.variables) for arg in argv]
        cwd = expand(str(cwd or self.repo), self.variables)
        log = self.output / 'logs' / (name + '.log')
        step = dict(name=name, argv=argv, cwd=cwd, log=str(log.relative_to(self.output)),
                    status='running', exit_code=None, seconds=0)
        self.steps.append(step)
        unavailable = [n for n in requires if not any(
            s['name'] == n and s['status'] == 'passed' for s in self.steps)]
        if unavailable:
            step.update(status='blocked', reason='prerequisite did not pass: ' + ', '.join(unavailable))
            log.write_text(step['reason'] + '\n')
            self.save()
            return False
        environment = dict(os.environ)
        environment.update({key: expand(str(value), self.variables) for key, value in (env or {}).items()})
        step['env'] = {key: environment[key] for key in (env or {})}
        self.save()
        print(f'Running {name}', flush=True)
        started = time.monotonic()
        process = None
        with log.open('w') as stream:
            try:
                process = subprocess.Popen(argv, cwd=cwd, env=environment, stdout=stream,
                                           stderr=subprocess.STDOUT, start_new_session=True)
                code = process.wait(timeout=self.config.get('timeout_seconds', 7200))
                step.update(exit_code=code, status='passed' if code == 0 else 'failed')
            except (subprocess.TimeoutExpired, KeyboardInterrupt) as exc:
                if process is not None:
                    os.killpg(process.pid, signal.SIGTERM)
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        os.killpg(process.pid, signal.SIGKILL)
                        process.wait()
                step.update(status='interrupted' if isinstance(exc, KeyboardInterrupt) else 'timeout')
                if isinstance(exc, KeyboardInterrupt):
                    self.interrupted = True
            except OSError as exc:
                step.update(status='error', reason=str(exc))
                stream.write(str(exc) + '\n')
        step['seconds'] = round(time.monotonic() - started, 2)
        self.save()
        if self.interrupted:
            raise KeyboardInterrupt
        return step['status'] == 'passed'

    def run(self):
        self.output.mkdir(parents=True, exist_ok=False)
        (self.output / '.gitignore').write_text('*\n')
        (self.output / 'logs').mkdir()
        (self.output / 'config.json').write_text(json.dumps(self.config, indent=2) + '\n')
        self.save()
        try:
            self.testing()
            self.captures()
        except KeyboardInterrupt:
            self.interrupted = True
        finally:
            self.save()
        return 130 if self.interrupted else int(
            any(s['status'] != 'passed' for s in self.steps) or
            any(p['status'] != 'complete' for p in self.pairs))

    def testing(self):
        for step in self.config.get('test', []) + self.config.get('build', []):
            if self.selected(step['name']):
                self.command(step['name'], step['argv'], step.get('cwd'), step.get('env'))
        if self.config.get('portable_c_tests', True) and self.selected('portable-c-tests'):
            directory = self.output / 'c-tests'
            directory.mkdir()
            for source in sorted((self.repo / 'wiiu-ver/perf-probe/tests').glob('*_test.c')):
                variants = [(source.stem, [])]
                if source.stem == 'stream_upload_test':
                    variants.append(('stream_upload_legacy_test', ['-DC2_RANGE_UPLOAD=0']))
                for name, flags in variants:
                    compiled = 'compile-' + name
                    binary = directory / name
                    self.command(compiled, ['cc', '-std=c11', '-Wall', '-Wextra', '-UNDEBUG',
                                            *flags, str(source), '-lm', '-o', str(binary)])
                    self.command(name, [str(binary)], requires=[compiled])

    def captures(self):
        regress = expand(self.config.get('regress', '{repo}/pc-ver/target/release/c2-regress'), self.variables)
        pc = expand(self.config.get('pc', '{repo}/pc-ver/target/release/c2-pc'), self.variables)
        repeat = str(self.config.get('repeat', 5))
        for game in self.config.get('game', []):
            gid = game['id']
            for scenario in game.get('scenarios', []):
                path = self.repo / 'pc-ver/crates/c2-regress/scenarios' / (scenario + '.toml')
                roots = {}
                engines = ['rust', 'original'] if game.get('original', False) else ['rust']
                if not self.selected(f'{scenario}-{gid}', *(f'{scenario}-{gid}-{e}' for e in engines)):
                    continue
                for engine in engines:
                    name = f'{scenario}-{gid}-{engine}'
                    target = self.output / 'gameplay' / name
                    roots[engine] = target
                    argv = [regress, 'run', '--scenario', str(path), '--game-id', gid,
                            '--game', expand(game['path'], self.variables), '--repeat', repeat,
                            '--engine', engine, '--out', str(target), '--c2-pc', pc,
                            '--label', 'audit']
                    if engine == 'rust' and game.get('baseline'):
                        argv += ['--baseline', expand(game['baseline'], self.variables)]
                    prerequisites = list(self.config.get('capture_requires', []))
                    if engine == 'original':
                        prerequisites += self.config.get('original_requires', [])
                    self.command(name, argv, requires=prerequisites)
                if game.get('original', False):
                    self.gameplay_comparison(f'{scenario}-{gid}', roots, regress)
        textures = self.config.get('textures', {})
        natives = [native for native in self.config.get('original_texture', [])
                   if self.selected('texture-original-' + native['id'])]
        for name in ['compact', 'exhaustive', 'original_input']:
            # The native references replay the original_input run, so they keep it.
            if name not in textures or not (self.selected('texture-' + name)
                                            or name == 'original_input' and self.only and natives):
                continue
            argv = [pc, 'texture-review', '--manifest', expand(textures[name], self.variables),
                    '--out', str(self.output / ('texture-' + name))]
            if name == 'original_input':
                argv += ['--keep-models']
            self.command('texture-' + name, argv, requires=self.config.get('capture_requires', []))
        for native in natives:
            name = 'texture-original-' + native['id']
            target = self.output / name
            argv = ['python3', str(self.repo / 'tools/texture-review-original.py'),
                    '--reference', str(self.output / 'texture-original_input'),
                    '--game', expand(native['path'], self.variables), '--out', str(target),
                    '--work', str(self.output / 'native-work' / native['id'])]
            for source in native['sources']:
                argv += ['--source', source]
            self.command(name, argv, requires=['texture-original_input', *self.config.get('original_requires', [])])
            self.pairs.append(dict(name=name, kind='matched-texture',
                                   status='complete' if (target / 'report.html').is_file() else 'missing',
                                   original=str(target.relative_to(self.output)),
                                   rust='texture-original_input'))
            self.save()

    def gameplay_comparison(self, name, roots, regress):
        runs = {engine: sorted(root.rglob('report.html')) if root.exists() else []
                for engine, root in roots.items()}
        pair = dict(name=name, kind='gameplay', status='missing')
        self.pairs.append(pair)
        if any(len(files) != 1 for files in runs.values()):
            pair['reason'] = 'Expected one Rust and one original report from this audit; inspect capture logs.'
            self.save()
            return
        for engine, files in runs.items():
            pair[engine] = str(files[0].parent.relative_to(self.output))
        original, rust = [str(self.output / pair[e]) for e in ['original', 'rust']]
        self.command('compare-' + name, [regress, 'compare', original, rust])
        pdf = self.output / 'comparisons' / (name + '.pdf')
        pdf.parent.mkdir(exist_ok=True)
        self.command('pdf-' + name, [regress, 'pdf', '--out', str(pdf), original, rust])
        before = {p.name for p in (Path(original) / 'screenshots').glob('*.png')}
        after = {p.name for p in (Path(rust) / 'screenshots').glob('*.png')}
        pair.update(status='complete' if before and after and before & after else 'missing',
                    pdf=str(pdf.relative_to(self.output)) if pdf.is_file() else None)
        if pair['status'] == 'missing':
            pair['reason'] = 'Both reports exist but no matching original/Rust screenshots were captured.'
        self.save()


def audit_directory(value):
    """An audit directory, or the newest one for `latest`."""
    if value != 'latest':
        return Path(value).resolve()
    audits = sorted(path for path in (REPO / 'regression-sim-recordings').glob('audit-*')
                    if (path / 'audit.json').is_file())
    if not audits:
        raise SystemExit('No audit found under regression-sim-recordings/.')
    return audits[-1]


def rebuild(directory, config=None):
    state = json.loads((directory / 'audit.json').read_text())
    if config is not None:
        # config.json stays the record of what the run requested.
        overlay = {key: config[key] for key in ['finding', 'original_texture'] if key in config}
        (directory / OVERLAY).write_text(json.dumps(overlay, indent=2) + '\n')
    write_report(directory, state)
    print(f'Master report: {directory / "report.html"}')


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--config', type=Path, help='private TOML audit configuration')
    parser.add_argument('--out', type=Path, help='new output directory (never overwritten)')
    parser.add_argument('--only', action='append', default=[], metavar='PATTERN',
                        help='run only steps matching this glob (repeatable), e.g. "kill_animations-*"; '
                             'capture prerequisites still run')
    parser.add_argument('--dry-run', action='store_true', help='validate configuration and print it without running commands')
    parser.add_argument('--report', metavar='AUDIT', help='rebuild the master report of an existing audit '
                        '(a directory or "latest") without rerunning anything; --config refreshes its declared findings')
    parser.add_argument('--serve', metavar='AUDIT', help='rebuild, then serve the report on localhost so reviews '
                        'are saved to AUDIT/reviews.json')
    parser.add_argument('--port', type=int, default=0, help='port for --serve (default: any free port)')
    args = parser.parse_args()
    config = tomllib.loads(args.config.read_text()) if args.config else None
    if args.report or args.serve:
        directory = audit_directory(args.report or args.serve)
        rebuild(directory, config)
        if args.serve:
            from regression_audit_serve import serve
            serve(directory, args.port)
        return 0
    if config is None:
        parser.error('--config is required to run an audit')
    if args.dry_run:
        print(json.dumps(config, indent=2))
        return 0
    stamp = datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ')
    output = args.out or REPO / 'regression-sim-recordings' / f'audit-{stamp}-{uuid.uuid4().hex[:8]}'
    print(f'Master report: {output.resolve() / "report.html"}', flush=True)
    code = Audit(config, output, only=args.only).run()
    print(f'Review with saved statuses: python3 tools/regression_audit.py --serve {shlex.quote(str(output))}')
    return code


if __name__ == '__main__':
    raise SystemExit(main())
