#!/usr/bin/env python3
"""Private, matched isolated captures through a shipped V_SOFT.REN under Wine.

Requires Pillow and NumPy. No game files, media, executable bytes or addresses are
published. Unsupported native code/layouts fail before starting Wine.
"""
import argparse
from contextlib import contextmanager
import fcntl
import hashlib
import html
import json
import math
from pathlib import Path, PurePosixPath
import re
import shutil
import struct
import subprocess
import sys
import uuid

from texture_review_summary import summarize, inject


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def safe_relative(name):
    p = PurePosixPath(name)
    if not name or p.is_absolute() or '..' in p.parts or '\\' in name or ':' in name:
        raise ValueError(f'unsafe relative path: {name}')
    return p


class Executable:
    """Small checked PE32 reader; no disassembler or third-party PE dependency."""
    def __init__(self, data):
        self.data = data
        if data[:2] != b'MZ':
            raise ValueError('not a PE executable')
        pe = self.u32(0x3c)
        if data[pe:pe+4] != b'PE\0\0' or self.u16(pe+4) != 0x14c or self.u16(pe+24) != 0x10b:
            raise ValueError('expected a 32-bit x86 Windows executable')
        self.timestamp = self.u32(pe+8)
        self.base, self.size = self.u32(pe+52), self.u32(pe+80)
        table = pe + 24 + self.u16(pe+20)
        self.sections = []
        for i in range(self.u16(pe+6)):
            at = table + i*40
            virtual_size, rva, raw_size, offset = struct.unpack_from('<4I', data, at+8)
            flags = self.u32(at+36)
            self.sections.append((rva, max(virtual_size, raw_size), offset, raw_size, flags))

    def u32(self, offset):
        return struct.unpack_from('<I', self.data, offset)[0]

    def u16(self, offset):
        return struct.unpack_from('<H', self.data, offset)[0]

    def rva(self, offset):
        for rva, size, raw, length, flags in self.sections:
            if raw <= offset < raw+length:
                return rva+offset-raw
        raise ValueError('code is outside PE sections')

    def writable(self, absolute):
        rva = absolute-self.base
        if not any(start <= rva and rva+4 <= start+size and flags & 0x80000000
                   for start, size, raw, length, flags in self.sections):
            raise ValueError('native global is not within writable executable memory')
        return rva

    def code_offset(self, absolute):
        rva = absolute-self.base
        for start, size, raw, length, flags in self.sections:
            if start <= rva < start+length and flags & 0x20000000:
                return raw+rva-start
        raise ValueError('native function is outside executable code')

    def unique(self, pattern):
        matches = [m.start() for m in re.finditer(pattern, self.data, re.DOTALL)
                   if any(raw <= m.start() < raw+length and flags & 0x20000000
                          for rva, size, raw, length, flags in self.sections)]
        if len(matches) != 1:
            raise ValueError(f'unsupported original executable: expected one native entry point, got {len(matches)}')
        return matches[0]


def native_profile(data):
    """Recognize the shipped C2/IA code and check each global's instruction.

    No guessed globals or caller-supplied absolute addresses are accepted.
    The model layout operand is also checked before permitting native calls.
    """
    pe = Executable(data)
    common = rb'\x55\x8b\xec\x83\xec'
    suffix = rb'\xd9\x45\x20\xd9\xff\x8b\x45\x08\x53\x56\x57'
    try:
        render = pe.unique(common + b'\x14' + suffix)
    except ValueError:
        return dynamic_profile(pe)
    other = pe.unique(common + b'\x18' + suffix)
    morph = pe.unique(rb'\x8b\x4c\x24\x08\x56\x8b\x41\x24\x48\x0f\xaf\x44\x24\x10\xc1\xe0\x08\x99\xf7\x79\x28')
    if data[render+0x34:render+0x3b] != bytes.fromhex('c1 e8 02 c1 e0 10 05'):
        raise ValueError('native renderer light setup does not match')
    if data[render+0x58:render+0x5e] != bytes.fromhex('8b 88 10 40 01 00'):
        raise ValueError('unsupported native model capacity/layout')

    def operand(at, opcode):
        if data[at:at+len(opcode)] != opcode:
            raise ValueError(f'unsupported native viewport instruction at {at:#x}')
        return pe.writable(pe.u32(at+len(opcode)))

    globals_ = [operand(render+offset, bytes.fromhex(opcode)) for offset, opcode in [
        (0x11d, 'd8 0d'), (0x136, 'd8 0d'), (0x12f, 'a1'), (0x141, '8b 0d'),
        (0x154, '3b 3d'), (0x15d, '3b 0d'), (0x1ea, 'a1')]]
    glass = operand(other+0x49, bytes.fromhex('8b 0d'))
    if data[render+0x4e:render+0x50] != bytes.fromhex('c7 05'):
        raise ValueError('unsupported native scanline setup')
    hline_address = pe.u32(render+0x54)
    hline = pe.code_offset(hline_address)
    if data[hline] == 0xe9:
        hline_address += 5+struct.unpack_from('<i', data, hline+1)[0]
        hline = pe.code_offset(hline_address)
    win_w = operand(hline+0xd5, bytes.fromhex('a1'))
    return [pe.timestamp, pe.size, pe.rva(render), pe.rva(morph), *globals_, glass, 1024, 0, 0, 0, win_w, operand(hline+0x10a, bytes.fromhex('03 3d'))]


def dynamic_profile(pe):
    """Recognize the Genesis/MEE optimized ABI and dynamic model buffers."""
    data = pe.data
    render = pe.unique(re.escape(bytes.fromhex('55 8b ec 83 e4 f0 83 ec 44 f3 0f 10 45 0c 53 56 f3 0f 11 4c 24 20')))
    other = pe.unique(re.escape(bytes.fromhex('55 8b ec 83 e4 f0 83 ec 48 f3 0f 10 45 0c 56 f3 0f 11 4c 24 20')))
    morph = pe.unique(re.escape(bytes.fromhex('51 53 56 57 8b f9 85 ff 0f 84')) + b'....' +
                      re.escape(bytes.fromhex('85 d2 0f 84')) + b'....' + re.escape(bytes.fromhex('8b 72 2c 85 f6')))
    if data[render+0x5d:render+0x64] != bytes.fromhex('c1 e8 02 c1 e0 10 05'):
        raise ValueError('unsupported optimized light setup')
    if data[render+0x84:render+0x87] != bytes.fromhex('8b 41 18'):
        raise ValueError('unsupported dynamic model texture offset')
    def operand(at, opcode):
        opcode = bytes.fromhex(opcode)
        if data[at:at+len(opcode)] != opcode:
            raise ValueError(f'unsupported optimized viewport instruction at {at:#x}')
        return pe.writable(pe.u32(at+len(opcode)))
    globals_ = [operand(render+offset, opcode) for offset, opcode in [
        (0x18b, 'f3 0f 59 15'), (0x193, 'f3 0f 59 1d'), (0x177, '8b 3d'),
        (0x1aa, '03 0d'), (0x1b5, '3b 3d'), (0x1be, '3b 0d'), (0x26d, '83 3d')]]
    glass = operand(other+0x71, '8b 15')
    rvertex = operand(render+0xb6, '8b 0d')
    screen = operand(render+0x156, 'a1')
    # HLineTxModel is assigned directly by RenderModelClip. Check its stride
    # and scissor instructions too; modernized builds use a variable DIB pitch.
    if data[render+0x7a:render+0x7c] != bytes.fromhex('c7 05'):
        raise ValueError('unsupported native scanline setup')
    hline_rva = pe.u32(render+0x80)-pe.base
    hline = next((raw+hline_rva-rva for rva, size, raw, length, flags in pe.sections
                  if rva <= hline_rva < rva+length and flags & 0x20000000), None)
    if hline is None:
        raise ValueError('native scanline entry is outside executable code')
    pitch = operand(hline+0x100, '0f af 3d')
    win_w = operand(hline+0xd5, 'a1')
    return [pe.timestamp, pe.size, pe.rva(render), pe.rva(morph), *globals_, glass, 8192, rvertex, screen, pitch, win_w, operand(hline+0x10e, '03 3d')]


def car_payload(data, animation, capacity=1024):
    """Read serialized CAR data, independently of the Rust pose implementation."""
    if len(data) < 52:
        raise ValueError('truncated CAR')
    animations, sounds, vc, fc, texture_bytes = struct.unpack_from('<5I', data, 32)
    if not 0 < vc <= capacity or not 0 < fc <= capacity or not 0 < texture_bytes <= 256*256*2 or texture_bytes % 512:
        raise ValueError(f'model exceeds native capacities ({capacity} vertices/faces, 256x256 texture)')
    pos = 52
    faces = [data[pos+i*64:pos+(i+1)*64] for i in range(fc)]
    pos += fc*64
    # Original CorrectModel: opaque, cutout, then transparent faces.
    def flags(face):
        if len(face) != 64:
            raise ValueError('truncated face')
        value = struct.unpack_from('<H', face, 36)[0]
        if value & 12 == 12:
            raise ValueError('ambiguous native face ordering: both opacity and transparency')
        return value
    faces.sort(key=lambda f: 2 if flags(f) & 8 else 1 if flags(f) & 4 else 0)
    vertices = data[pos:pos+vc*16]
    pos += vc*16
    texture = data[pos:pos+texture_bytes]
    pos += texture_bytes
    if len(vertices) != vc*16 or len(texture) != texture_bytes:
        raise ValueError('truncated mesh')
    selected = None
    for index in range(animations):
        if pos+40 > len(data):
            raise ValueError('truncated animation')
        kps, frames = struct.unpack_from('<iI', data, pos+32)
        pos += 40
        end = pos+frames*vc*6
        if end > len(data) or not frames or kps <= 0:
            raise ValueError('invalid animation')
        if index == animation:
            selected = (frames, kps, frames*1000//kps, data[pos:end])
        pos = end
    if animations and selected is None:
        raise ValueError('selected clip is absent')
    if not animations and animation != 0:
        raise ValueError('rest model only has animation index zero')
    return vc, fc, b''.join(faces), vertices, texture, selected


def validate_recipe(recipe):
    if recipe.get('schema') != 1:
        raise ValueError('unsupported capture recipe')
    values = [*recipe['center'], recipe['distance'], recipe['yaw'], recipe['pitch'],
              recipe['vertical_half_angle_tangent'], *recipe['background'], recipe['near'],
              recipe['far'], *recipe['fog'], *recipe['daylight']]
    if not all(math.isfinite(v) for v in values):
        raise ValueError('non-finite capture recipe')
    w, h = recipe['resolution']
    if not 128 <= w <= 1024 or not 128 <= h <= 768 or w % 2 or h % 2:
        raise ValueError('native capture requires even resolution within 1024x768')
    if (recipe['ambient'], recipe['instance_light'], recipe['brightness']) != (1, 1, 1):
        raise ValueError('native adapter supports neutral full-light recipes only')
    if recipe['fog'][0] < recipe['far'] or recipe['near'] != 16 or recipe['daylight'] != [0, 1, 0]:
        raise ValueError('native adapter requires the isolated no-fog recipe')
    if recipe['distance'] <= 0 or recipe['vertical_half_angle_tangent'] <= 0 or abs(recipe['pitch']) >= math.pi/2:
        raise ValueError('invalid native camera')


def background_word(rgb):
    r, g, b = [(c*31+127)//255 for c in rgb]
    return (r << 10) | (g << 5) | b


def write_jobs(path, profile, captures, reference, token):
    from PIL import Image
    rows = []
    with path.open('xb') as stream:
        stream.write(b'C2TXRV3\0'+struct.pack('<18I', *profile)+struct.pack('<I', len(captures)))
        for index, capture in enumerate(captures):
            recipe = capture['recipe']
            validate_recipe(recipe)
            if capture.get('scenario') != 'isolated-model-v1' or recipe['animation'] != capture['animation']:
                raise ValueError('capture identity does not match its recipe')
            model = reference / safe_relative(capture.get('model_file') or '')
            expected_pose = reference / safe_relative(capture.get('pose_file') or '')
            if not model.is_file() or not expected_pose.is_file():
                raise ValueError('reference needs models and poses: recapture with c2-pc texture-review --keep-models')
            if sha(model) != capture['car_sha256']:
                raise ValueError('reference model hash mismatch')
            vc, fc, faces, vertices, texture, animation = car_payload(model.read_bytes(), capture['animation'], profile[12])
            if expected_pose.stat().st_size != vc*12:
                raise ValueError('reference pose length differs from model')
            with Image.open(reference/'screenshots'/safe_relative(capture['file'])) as image:
                rgb = image.convert('RGB').getpixel((0, 0))
                if image.size != tuple(recipe['resolution']):
                    raise ValueError('reference image resolution mismatch')
            stem = f'tx-{token}-{index:05}'
            name = stem.encode('ascii')
            w, h = recipe['resolution']
            stream.write(name+b'\0'*(128-len(name)))
            stream.write(struct.pack('<3I7f4I', w, h, recipe['time_ms'],
                                     *recipe['center'], recipe['distance'], recipe['yaw'], recipe['pitch'],
                                     recipe['vertical_half_angle_tangent'], background_word(rgb), vc, fc, len(texture)//2))
            stream.write(faces+vertices+texture)
            if animation:
                frames, kps, duration, samples = animation
                if recipe['time_ms'] >= duration or (frames-1)*recipe['time_ms'] > 0x7fffffff//256:
                    raise ValueError('native animation time exceeds supported integer arithmetic')
                stream.write(struct.pack('<3I', frames, kps, duration)+samples)
            else:
                stream.write(struct.pack('<I', 0))
            rows.append((stem, capture, rgb))
    return rows


def ci(directory, name):
    return next((p for p in directory.iterdir() if p.name.lower() == name.lower()), None)


def pin_config(game):
    path = ci(game, 'config.cfg')
    if path is None:
        return
    pins = dict(renderer='0', resolution='800x600', display_mode='1', fps_limit='0')
    lines = [line for line in path.read_text().splitlines()
             if not line.split() or line.split()[0] not in pins]
    path.write_text('\n'.join(lines + [f'{key} {value}' for key, value in pins.items()])+'\n')


def winpath(path):
    return 'Z:' + str(path.resolve()).replace('/', '\\')


@contextmanager
def work_lock(work):
    work.mkdir(parents=True, exist_ok=True)
    with (work/'.texture.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        yield


def run_wine(prefix, hook, game, executable, script, timeout):
    env = dict(__import__('os').environ, WINEPREFIX=str(prefix.resolve()), WINEDEBUG='-all',
               C2H_SCRIPT=winpath(script), C2H_EXIT='500', C2H_FRAME_MS='20')
    def invoke(args, **kwargs):
        return subprocess.run(args, env=env, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE,
                              timeout=timeout, **kwargs)
    # Only this dedicated prefix is stopped, never a user's Wine session.
    def stop():
        subprocess.run(['wineserver', '-k'], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        subprocess.run(['wineserver', '-w'], env=env, timeout=30,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        if not (prefix/'system.reg').exists():
            prefix.mkdir(parents=True, exist_ok=True)
            invoke(['wineboot', '-i'])
        for key, value, setting in [
                ('HKCU\\Software\\Wine\\Explorer', 'Desktop', 'Default'),
                ('HKCU\\Software\\Wine\\Explorer\\Desktops', 'Default', '800x600')]:
            invoke(['wine', 'reg', 'add', key, '/v', value, '/d', setting, '/f'])
        stop()
        result = subprocess.run(['wine', str(hook/'c2launch.exe'), winpath(executable),
                                 'prj=huntdat\\areas\\area2', 'reg=0', 'din=1', 'wep=1', 'dtm=1'],
                                env=env, cwd=game, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=timeout)
        return result.returncode, result.stdout.decode(errors='replace'), result.stderr.decode(errors='replace')
    finally:
        stop()


def capture_results(rows, game, reference, out, review):
    from PIL import Image, ImageChops
    import numpy as np
    records, errors = [], []
    shots, poses = out/'screenshots', out/'poses'
    shots.mkdir(); poses.mkdir()
    gallery = ['<!doctype html><meta charset=utf-8><title>Matched original texture review</title>',
               '<style>body{background:#222;color:#eee;font:16px sans-serif}img{width:32%}</style>',
               '<h1>Matched original texture review</h1><p>Human approval pending. '
               'Left: original executable. Middle: Rust. Right: difference. '
               'Native pose agreement is checked separately from rasterizer differences.</p>']
    parity_failures = 0
    for stem, capture, background in rows:
        item = dict(capture)
        try:
            raw, pose = game/(stem+'.555'), game/(stem+'.pose')
            if not raw.is_file() or not pose.is_file():
                raise ValueError('native capture/pose missing')
            expected = np.frombuffer((reference/safe_relative(capture['pose_file'])).read_bytes(), dtype='<f4')
            actual_bytes = pose.read_bytes()
            actual = np.frombuffer(actual_bytes, dtype='<f4')
            if actual.shape != expected.shape or not np.isfinite(actual).all() or not np.isfinite(expected).all():
                raise ValueError('invalid native pose')
            error = float(np.max(np.abs(expected-actual)))
            bound = max(.001, float(np.max(np.abs(expected)))*1e-6)
            if error > bound:
                raise ValueError(f'native pose mismatch: {error:g}, allowed {bound:g}')
            data = raw.read_bytes()
            w, h = struct.unpack_from('<2I', data)
            if [w, h] != capture['recipe']['resolution'] or len(data) != 8+w*h*2:
                raise ValueError('native surface dimensions/length mismatch')
            pixels = np.frombuffer(data, dtype='<u2', offset=8).reshape((h, w))
            foreground = int(np.count_nonzero(pixels != background_word(background)))
            if foreground <= 8 and not capture.get('source_pose_collapsed'):
                raise ValueError('empty native model capture')
            rgb = np.stack([(pixels >> 10) & 31, (pixels >> 5) & 31, pixels & 31], axis=-1)
            rgb = ((rgb * 255 + 15)//31).astype('uint8')
            # Preserve the exact shared clear colour outside the model. The
            # original stores RGB555; quantizing the whole background would
            # otherwise count as a renderer difference for every empty pixel.
            rgb[pixels == background_word(background)] = background
            original = Image.fromarray(rgb)
            name = safe_relative(capture['file'])
            original.save(shots/name)
            before = Image.open(reference/'screenshots'/name).convert('RGB')
            candidate = np.asarray(before).astype('int16')
            delta = np.abs(rgb.astype('int16')-candidate)
            changed = float(np.mean(np.any(delta > review['channel_tolerance'], axis=-1)))
            parity_failures += int(changed > review['allowed_changed_fraction'])
            shutil.copyfile(reference/'screenshots'/name, shots/('rust-'+str(name)))
            ImageChops.difference(original, before).save(shots/('diff-'+str(name)))
            pose_name = Path(stem+'.f32')
            (poses/pose_name).write_bytes(actual_bytes)
            item.update(status='matched original capture', expected_pose_sha256=sha(reference/safe_relative(capture['pose_file'])), native_pose_max_error=error,
                        native_pose_allowed_error=bound, parity_changed_fraction=changed,
                        foreground_pixels=foreground, pose_file='poses/'+str(pose_name), capture_error=None)
            gallery.append(f'<section><p>{html.escape(capture["source"])} · {html.escape(capture["asset"])} · '
                           f'{html.escape(capture["clip"])} · {capture["side"]} · '
                           f'pose error {error:g}; image delta {changed:.2%}</p>'
                           + ''.join(f'<a href="screenshots/{prefix}{html.escape(str(name), quote=True)}">'
                                     f'<img loading=lazy src="screenshots/{prefix}{html.escape(str(name), quote=True)}"></a>'
                                     for prefix in ['', 'rust-', 'diff-'])+'</section>')
        except (ValueError, OSError) as exc:
            item.update(status='invalid capture', capture_error=str(exc))
            errors.append(f'{capture["file"]}: {exc}')
        records.append(item)
    gallery.extend(f'<p>{html.escape(e)}</p>' for e in errors)
    gallery.append(f'<p>{len(records)} captures; {len(errors)} capture failures; '
                   f'{parity_failures} exceed the image tolerance. Human approval pending.</p>')
    summary = summarize(dict(review, captures=records, engine='original-software'))
    (out/'report.html').write_text(inject('\n'.join(gallery), summary, review['channel_tolerance']))
    return records, errors, parity_failures


def run(args):
    # Validate all prerequisites before copying/launching anything.
    reference, out, source_game = args.reference.resolve(), args.out.resolve(), args.game.resolve()
    if out.exists():
        raise ValueError('output exists; choose a new directory')
    executable = ci(source_game, 'v_soft.ren')
    if executable is None:
        raise ValueError('game has no V_SOFT.REN')
    profile = native_profile(executable.read_bytes())
    review = json.loads((reference/'review.json').read_text())
    if review.get('kind') != 'private-texture-review' or review.get('engine') != 'rust' or review.get('failures') != 0:
        raise ValueError('reference must be a complete Rust texture review')
    captures = [c for c in review['captures'] if not args.source or c['source'] in args.source]
    missing = set(args.source or []) - {c['source'] for c in captures}
    if not captures or missing:
        raise ValueError(f'no captures or unknown source ids: {sorted(missing)}')
    if source_game == args.work.resolve() or source_game in args.work.resolve().parents:
        raise ValueError('work must be outside the original installation')
    if source_game == out or source_game in out.parents or reference == out or reference in out.parents:
        raise ValueError('output must be outside the installation and reference run')
    hook = args.hook_dir.resolve()
    for name in ['c2hook.dll', 'c2launch.exe']:
        if not (hook/name).is_file():
            raise ValueError('build the original hook with original/build.sh first')
    provenance = json.loads((hook/'hook-provenance.json').read_text())
    if sha(hook/'c2hook.dll') != provenance['hook_sha256']:
        raise ValueError('hook differs from its build provenance; rebuild the hook')
    # NumPy/Pillow are for local image analysis, never substitutes for native rendering.
    import numpy
    from PIL import Image
    out.mkdir(parents=True)
    (out/'.gitignore').write_text('*\n')
    token = uuid.uuid4().hex[:12]
    jobs = out/'original-jobs.bin'
    rows = write_jobs(jobs, profile, captures, reference, token)
    work = args.work.resolve()
    with work_lock(work):
        game = work/'game'
        stamp = work/'game-source.json'
        identity = dict(source=str(source_game), executable_sha256=sha(executable))
        if game.exists():
            if not stamp.is_file() or json.loads(stamp.read_text()) != identity:
                raise ValueError('work directory belongs to a different game; choose a new --work')
        else:
            shutil.copytree(source_game, game, ignore=shutil.ignore_patterns(
                '*.log', '*.ini', 'd3d9.dll', 'dxgi.dll', 'opengl32.dll', 'reshade-shaders'))
            stamp.write_text(json.dumps(identity))
        pin_config(game)
        copied_exe = ci(game, 'v_soft.ren')
        if sha(copied_exe) != identity['executable_sha256']:
            raise ValueError('working executable differs from the scanned original')
        shutil.copyfile(jobs, game/('texture-'+token+'.bin'))
        script = out/'original-script.txt'
        script.write_text(f'20 texture texture-{token}.bin\n400 exit\n')
        code, stdout, stderr = run_wine(work/'prefix', hook, game, copied_exe, script, args.timeout)
        log = (game/'c2hook.log').read_text(errors='replace') if (game/'c2hook.log').exists() else ''
        (out/'original.log').write_text(log+'\n'+stdout+'\n'+stderr)
        records, errors, parity_failures = capture_results(rows, game, reference, out, review)
        if code != 0 or 'texture: complete' not in log:
            errors.append(f'original process failed/incomplete (exit {code}); see original.log')
    result = dict(review)
    result.update(engine='original-software', asset_type='original-parity', human_approval='pending',
                  captures=records, animals=len({(c['source'], c['asset']) for c in captures}),
                  failures=len(errors), errors=errors, parity_failures=parity_failures,
                  engine_commit=provenance['engine_commit'], engine_dirty=provenance['engine_dirty'],
                  matched_capture_schema=1, hook_source_sha256=provenance['hook_source_sha256'],
                  reference=str(reference), original_executable_sha256=identity['executable_sha256'],
                  original_hook_sha256=sha(hook/'c2hook.dll'), runner_sha256=sha(Path(__file__)), native_profile=profile,
                  adapter='shipped software renderer under Wine', driver=None)
    # Original evidence includes the exact serialized models consumed by the native calls.
    for capture in records:
        name = capture.get('model_file')
        if name:
            target = out/safe_relative(name)
            target.parent.mkdir(parents=True, exist_ok=True)
            if not target.exists():
                shutil.copyfile(reference/safe_relative(name), target)
    for capture in records:
        capture['original_executable_sha256'] = identity['executable_sha256']
    result['comparison_summary'] = summarize(result)
    (out/'review.json').write_text(json.dumps(result, indent=2)+'\n')
    print(f'{len(records)} original captures, {len(errors)} capture failures, {parity_failures} image differences')
    print(out/'report.html')
    if errors:
        raise ValueError('; '.join(errors[:3]))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reference', required=True, type=Path, help='Rust run captured with --keep-models')
    parser.add_argument('--game', required=True, type=Path, help='owned original game installation')
    parser.add_argument('--source', action='append', help='source id from the Rust run; repeatable')
    parser.add_argument('--out', required=True, type=Path)
    parser.add_argument('--work', required=True, type=Path, help='dedicated private game copy and Wine prefix')
    parser.add_argument('--hook-dir', type=Path, default=Path(__file__).resolve().parents[1]/'pc-ver/target/original-hook')
    parser.add_argument('--timeout', type=int, default=300)
    args = parser.parse_args()
    try:
        run(args)
    except (ValueError, OSError, subprocess.SubprocessError) as exc:
        parser.exit(1, f'Original texture review: {exc}\n')


if __name__ == '__main__':
    main()
