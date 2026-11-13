"""Synthetic scene-probe validation; no game executable or asset fixtures."""
import importlib.util
from pathlib import Path
import struct
import unittest
from unittest.mock import patch
from test_texture_review_original import executable

spec = importlib.util.spec_from_file_location('scene_native', Path(__file__).with_name('scene-review-original.py'))
scene = importlib.util.module_from_spec(spec)
spec.loader.exec_module(scene)


def fixture():
    data = executable(True)
    data[0x400:0x2400] = bytes(0x2000)
    draw, sky = 0x500, 0x1000
    def put(at, code):
        b = bytes.fromhex(code); data[at:at+len(b)] = b
    def operand(at, code, slot):
        b = bytes.fromhex(code)
        data[at:at+len(b)+4] = b+struct.pack('<I',0x404000+slot*4)
    put(draw,'55 8b ec 83 e4 f8')
    operand(draw+6,'f3 0f 10 05',0)
    put(draw+14,'56 57 c7 05')
    put(sky,'55 8b ec 83 e4 f8 83 ec 2c 53 56 57 e8 00 00 00 00')
    operand(sky+0x11,'f3 0f 10 05',1)
    put(sky+0x19,'33 db e8')
    put(draw+0x9a,'e8')
    struct.pack_into('<i',data,draw+0x9b,sky-(draw+0x9a+5))
    for start,offset,op,slot in [
        (draw,0x2e,'f3 0f 10 05',1),(draw,0x43,'f3 0f 2c 05',2),
        (sky,0x8a,'f3 0f 5c 0d',3),(draw,0x75,'f3 0f 2c 05',4),
        (draw,0x22,'f3 0f 11 05',5),(draw,0x36,'f3 0f 11 0d',6),
        (sky,0x20,'8b 35',7),(sky,0x72,'f3 0f 2a 1d',8),
        (sky,0x29,'a1',9),(draw,0x424,'39 3d',10)]:
        operand(start+offset,op,slot)
    put(sky+0xec,'f2 0f 5c 0d')
    struct.pack_into('<I',data,sky+0xf0,0x404100)
    struct.pack_into('<d',data,0x2500,.15)
    base=[123,0x7000]+[0]*16; base[12]=8192
    return data,base


class SceneProbeTests(unittest.TestCase):
    def test_checked_profile_and_reject_altered_operands(self):
        data,base=fixture()
        with patch.object(scene.native,'native_profile',return_value=base):
            _,p=scene.scene_profile(data)
            self.assertEqual(p[:2],[0x1100,0x1c00])
            self.assertEqual(p[2:7],[0x4008,0x400c,0x4010,0x4000,0x4004])
            for offset in [0x500+0x9b,0x500+0x43,0x1000+0xf0,0x2500]:
                bad=data.copy();bad[offset]^=1
                with self.assertRaises(ValueError):scene.scene_profile(bad)
            bad=data.copy();struct.pack_into('<I',bad,0x500+0x47,0x401000)
            with self.assertRaises(ValueError):scene.scene_profile(bad)

    def test_recipe_rejects_nonfinite_camera_and_unsupported_ranges(self):
        valid=dict(camera=[1000,200,1000,0,0],clock_ms=0,radius=74)
        scene.validate_recipe(dict(schema=1,captures=[valid]))
        for change in [dict(camera=[0,0,0,0,float('nan')]),dict(radius=129),
                       dict(clock_ms=-1),dict(camera=[0,0,0,0,1.6])]:
            with self.assertRaises(ValueError):
                scene.validate_recipe(dict(schema=1,captures=[dict(valid,**change)]))


if __name__=='__main__':unittest.main()
