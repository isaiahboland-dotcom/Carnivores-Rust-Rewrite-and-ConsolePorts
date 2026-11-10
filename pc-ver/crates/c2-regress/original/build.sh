#!/bin/sh
# Builds c2launch.exe and c2hook.dll (32-bit Windows) with clang and
# lld-link, against Wine's headers and import libraries. No MinGW needed.
#   ./build.sh [OUT_DIR]   (default: ../../../target/original-hook)
set -eu
here=$(cd "$(dirname "$0")" && pwd)
out=${1:-"$here/../../../target/original-hook"}
mkdir -p "$out"
wine_inc=${WINE_INCLUDE:-/usr/include/wine}
wine_lib=${WINE_LIB:-/usr/lib/wine/i386-windows}
cflags="--target=i686-pc-windows-msvc -O2 -ffreestanding -fno-stack-protector -fno-builtin
  -isystem $wine_inc/msvcrt -isystem $wine_inc/windows -D_WIN32 -DWIN32_LEAN_AND_MEAN -Wall -Wno-inconsistent-dllimport -Wno-ignored-attributes"
libs="$wine_lib/libkernel32.a $wine_lib/libuser32.a $wine_lib/libgdi32.a"
# shellcheck disable=SC2086
clang $cflags -c "$here/hook.c" -o "$out/hook.o"
# shellcheck disable=SC2086
clang $cflags -c "$here/script.c" -o "$out/script.o"
# shellcheck disable=SC2086
clang $cflags -c "$here/kill.c" -o "$out/kill.o"
# shellcheck disable=SC2086
clang $cflags -c "$here/behave.c" -o "$out/behave.o"
# shellcheck disable=SC2086
clang $cflags -c "$here/texture.c" -o "$out/texture.o"
# shellcheck disable=SC2086
clang $cflags -c "$here/launch.c" -o "$out/launch.o"
# shellcheck disable=SC2086
lld-link /nologo /dll /safeseh:no /entry:DllMain /nodefaultlib /out:"$out/c2hook.dll" "$out/hook.o" "$out/script.o" "$out/kill.o" "$out/behave.o" "$out/texture.o" $libs
# shellcheck disable=SC2086
lld-link /nologo /safeseh:no /subsystem:console /entry:start /nodefaultlib /out:"$out/c2launch.exe" "$out/launch.o" $libs
echo "$out/c2launch.exe"
echo "$out/c2hook.dll"

# Record the code that built this hook, independently of the Rust reference.
python3 - "$here" "$out" <<'PYCODE'
import hashlib, json, pathlib, subprocess, sys
here, out = map(pathlib.Path, sys.argv[1:])
def git(*args):
    p = subprocess.run(['git', '-C', str(here), *args], capture_output=True, text=True)
    return p.stdout.strip() if p.returncode == 0 else None
revision = git('rev-parse', 'HEAD')
state = git('status', '--porcelain')
h = hashlib.sha256()
for path in sorted([*here.glob('*.c'), *here.glob('*.h'), here/'build.sh']):
    h.update(path.name.encode()+b'\0'+path.read_bytes())
(out/'hook-provenance.json').write_text(json.dumps(dict(
    engine_commit=revision, engine_dirty=state is None or bool(state),
    hook_source_sha256=h.hexdigest(),
    hook_sha256=hashlib.sha256((out/'c2hook.dll').read_bytes()).hexdigest()), indent=2)+'\n')
PYCODE
