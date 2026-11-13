#!/usr/bin/env python3
"""Private Genesis native scene/sky diagnostic; fixed camera and clock.

Requires an existing dedicated texture-review work copy/prefix, a freshly built
hook, and a Rust scene-review recipe. Never runs in the owned installation.
Unsupported executable instruction layouts fail before Wine is started.
"""
import argparse
import importlib.util
import json
import math
from pathlib import Path
import shutil
import struct
import subprocess

spec = importlib.util.spec_from_file_location('texture_native', Path(__file__).with_name('texture-review-original.py'))
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)


def scene_profile(data):
    pe = native.Executable(data)
    base = native.native_profile(data)
    if base[12] != 8192:
        raise ValueError('scene probe currently supports Genesis dynamic software renderer only')
    draw = pe.unique(bytes.fromhex('55 8b ec 83 e4 f8 f3 0f 10 05') + b'....' + bytes.fromhex('56 57 c7 05'))
    sky = pe.unique(bytes.fromhex('55 8b ec 83 e4 f8 83 ec 2c 53 56 57 e8') + b'....' +
                    bytes.fromhex('f3 0f 10 05') + b'....' + bytes.fromhex('33 db e8'))

    def operand(start, offset, opcode):
        op = bytes.fromhex(opcode)
        at = start + offset
        if data[at:at+len(op)] != op:
            raise ValueError('unsupported scene instruction layout')
        return pe.writable(pe.u32(at+len(op)))

    call = draw + 0x9a
    if data[call] != 0xe8 or pe.rva(call)+5+struct.unpack_from('<i', data, call+1)[0] != pe.rva(sky):
        raise ValueError('DrawScene does not call recognized sky renderer')
    beta = operand(draw, 0x2e, 'f3 0f 10 05')
    if beta != operand(sky, 0x11, 'f3 0f 10 05'):
        raise ValueError('inconsistent scene/sky camera pitch')
    # Independently check the sky tilt constant used by this shipped fork.
    at = sky + 0xec
    if data[at:at+4] != bytes.fromhex('f2 0f 5c 0d'):
        raise ValueError('sky tilt instruction differs')
    tilt = pe.u32(at+4) - pe.base
    raw = next((off+tilt-rva for rva, size, off, length, flags in pe.sections if rva <= tilt and tilt+8 <= rva+length), None)
    if raw is None or struct.unpack_from('<d', data, raw)[0] != 0.15:
        raise ValueError('sky tilt constant differs')
    profile = [pe.rva(draw), pe.rva(sky),
               operand(draw, 0x43, 'f3 0f 2c 05'), operand(sky, 0x8a, 'f3 0f 5c 0d'),
               operand(draw, 0x75, 'f3 0f 2c 05'), operand(draw, 6, 'f3 0f 10 05'), beta,
               operand(draw, 0x22, 'f3 0f 11 05'), operand(draw, 0x36, 'f3 0f 11 0d'),
               operand(sky, 0x20, '8b 35'), operand(sky, 0x72, 'f3 0f 2a 1d'),
               operand(sky, 0x29, 'a1'), base[5], operand(draw, 0x424, '39 3d')]
    return base, profile


def validate_recipe(recipe):
    captures = recipe.get('captures', [])
    if recipe.get('schema') != 1 or not 1 <= len(captures) <= 100:
        raise ValueError('unsupported recipe schema or capture count')
    for c in captures:
        cam = c['camera']
        if len(cam) != 5 or not all(math.isfinite(v) for v in cam) or abs(cam[4]) >= 1.5:
            raise ValueError('invalid camera')
        if not isinstance(c['radius'], int) or not 16 <= c['radius'] <= 128:
            raise ValueError('invalid radius')
        if not isinstance(c['clock_ms'], int) or not 0 <= c['clock_ms'] <= 0xffffffff:
            raise ValueError('invalid sky clock')


def run(args):
    recipe = json.loads(args.manifest.read_text())
    validate_recipe(recipe)
    work, out, hook = args.work.resolve(), args.out.resolve(), args.hook_dir.resolve()
    game = work/'game'
    if not (work/'prefix/system.reg').exists() or not (game/'trophy00.sav').exists():
        raise ValueError('use an initialized dedicated regression work copy and fresh regression profile')
    if out.exists() or out == game or game in out.parents:
        raise ValueError('output must be a new directory outside the game copy')
    exe = native.ci(game, 'v_soft.ren')
    base, profile = scene_profile(exe.read_bytes())
    map_path = Path(recipe['map'])
    copied_map = native.ci(game/'HUNTDAT/AREAS', 'AREA2.MAP')
    copied_rsc = native.ci(game/'HUNTDAT/AREAS', 'AREA2.RSC')
    if native.sha(map_path) != native.sha(copied_map) or native.sha(map_path.with_suffix('.RSC')) != native.sha(copied_rsc):
        raise ValueError('Rust/native map resources differ')
    provenance = json.loads((hook/'hook-provenance.json').read_text())
    if native.sha(hook/'c2hook.dll') != provenance['hook_sha256']:
        raise ValueError('hook build provenance mismatch')
    out.mkdir(parents=True)
    (out/'.gitignore').write_text('*\n')
    with native.work_lock(work):
        for pattern in ['scene-*.555', 'sky-*.555', 'state-*.bin']:
            if list(game.glob(pattern)):
                raise ValueError('work copy contains prior scene captures; archive them first')
        # Match the Rust viewer's fixed-day brightness and default options.
        regress = Path(__file__).resolve().parents[1]/'pc-ver/target/release/c2-regress'
        subprocess.run([str(regress), 'original-profile', str(game/'trophy00.sav'),
                        '--res', '4'], check=True, stdout=subprocess.DEVNULL)
        native.pin_config(game)
        jobs = bytearray(b'C2SCN01\0' + struct.pack('<18I14II', *base, *profile, len(recipe['captures'])))
        for c in recipe['captures']:
            x,y,z,yaw,pitch = c['camera']
            jobs.extend(struct.pack('<5fII', x,y,z,-yaw,-pitch,c['clock_ms'],c['radius']))
        (out/'jobs.bin').write_bytes(jobs)
        shutil.copyfile(out/'jobs.bin', game/'scene-jobs.bin')
        script = out/'script.txt'
        script.write_text('20 scene scene-jobs.bin\n400 exit\n')
        code, stdout, stderr = native.run_wine(work/'prefix', hook, game, exe, script, 120)
        log = (game/'c2hook.log').read_text(errors='replace')
        (out/'native.log').write_text(log+'\n'+stdout+'\n'+stderr)
        if code != 0 or 'scene: complete' not in log:
            raise ValueError(f'native capture incomplete (exit {code}); see native.log')
        import numpy as np
        from PIL import Image
        states = []
        for i,c in enumerate(recipe['captures']):
            state = (game/f'state-{i:03}.bin').read_bytes()
            actual = struct.unpack('<7fIIifI', state)
            expected = (*c['camera'][:3], -c['camera'][3], -c['camera'][4])
            if any(abs(a-b)>1e-4 for a,b in zip(actual[:5],expected)) or actual[7:9] != (c['clock_ms'],c['radius']) or actual[10:] != (500.0,0):
                raise ValueError('native state does not match requested camera/clock/viewport/herd')
            states.append(actual)
            shutil.move(game/f'state-{i:03}.bin', out/f'state-{i:03}.bin')
            for kind in ['scene','sky']:
                file = f'{kind}-{i:03}.555'
                data = (game/file).read_bytes()
                if struct.unpack_from('<II',data) != (800,600) or len(data) != 8+800*600*2:
                    raise ValueError('invalid native surface output')
                p = np.frombuffer(data[8:], dtype='<u2').reshape(600,800).astype('uint32')
                rgb = np.stack([(p>>10)&31,(p>>5)&31,p&31],axis=-1)
                Image.fromarray(((rgb*255+15)//31).astype('uint8')).save(out/f'{kind}-{i:03}.png')
                shutil.move(game/file,out/file)
    result = dict(recipe=recipe,native_state=states,executable_sha256=native.sha(exe),
                  map_sha256=native.sha(copied_map),rsc_sha256=native.sha(copied_rsc),hook=provenance,
                  scope='fixed day, environment without animals or weapon; native DrawScene and RenderSkyPlane',
                  native_profile=profile,resolution=[800,600],vertical_half_angle_tangent=0.6)
    (out/'review.json').write_text(json.dumps(result,indent=2)+'\n')
    print(f'{len(states)} validated native scene/sky pairs: {out}')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['manifest','work','out','hook-dir']:
        parser.add_argument('--'+name, required=True, type=Path)
    try:
        run(parser.parse_args())
    except (ValueError,OSError) as exc:
        parser.exit(1,f'Scene review: {exc}\n')
