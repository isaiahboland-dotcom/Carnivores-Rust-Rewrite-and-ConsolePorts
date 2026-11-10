"""Synthetic native-adapter tests. No game bytes, images or installs in fixtures."""
import importlib.util
import json
from pathlib import Path
import struct
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('original_review', Path(__file__).with_name('texture-review-original.py'))
original = importlib.util.module_from_spec(spec)
spec.loader.exec_module(original)


def executable(dynamic=False):
    data = bytearray(0x3000)
    data[:2] = b'MZ'
    struct.pack_into('<I', data, 0x3c, 0x80)
    data[0x80:0x84] = b'PE\0\0'
    struct.pack_into('<HHI', data, 0x84, 0x14c, 2, 123)
    struct.pack_into('<H', data, 0x94, 224)
    struct.pack_into('<H', data, 0x98, 0x10b)
    struct.pack_into('<I', data, 0x80+52, 0x400000)
    struct.pack_into('<I', data, 0x80+80, 0x7000)
    for index, (rva, size, raw, flags) in enumerate([(0x1000, 0x2000, 0x400, 0x60000020),
                                                    (0x4000, 0x2000, 0x2400, 0xc0000040)]):
        at = 0x80+24+224+index*40
        struct.pack_into('<4I', data, at+8, size, rva, 0x1000 if index else 0x2000, raw)
        struct.pack_into('<I', data, at+36, flags)
    render, other, morph = 0x500, 0x1000, 0x1800
    def put(at, code):
        code = bytes.fromhex(code)
        data[at:at+len(code)] = code
    def operand(at, code, index):
        code = bytes.fromhex(code)
        data[at:at+len(code)+4] = code+struct.pack('<I', 0x404000+index*4)
    if dynamic:
        put(render, '55 8b ec 83 e4 f0 83 ec 44 f3 0f 10 45 0c 53 56 f3 0f 11 4c 24 20')
        put(other, '55 8b ec 83 e4 f0 83 ec 48 f3 0f 10 45 0c 56 f3 0f 11 4c 24 20')
        put(morph, '51 53 56 57 8b f9 85 ff 0f 84 00 00 00 00 85 d2 0f 84 00 00 00 00 8b 72 2c 85 f6')
        put(render+0x5d, 'c1 e8 02 c1 e0 10 05')
        put(render+0x84, '8b 41 18')
        offsets = [(0x18b, 'f3 0f 59 15'), (0x193, 'f3 0f 59 1d'), (0x177, '8b 3d'),
                   (0x1aa, '03 0d'), (0x1b5, '3b 3d'), (0x1be, '3b 0d'), (0x26d, '83 3d')]
        operand(other+0x71, '8b 15', 7)
        operand(render+0xb6, '8b 0d', 8)
        operand(render+0x156, 'a1', 9)
    else:
        for at, size in [(render, '14'), (other, '18')]:
            put(at, '55 8b ec 83 ec '+size+' d9 45 20 d9 ff 8b 45 08 53 56 57')
        put(morph, '8b 4c 24 08 56 8b 41 24 48 0f af 44 24 10 c1 e0 08 99 f7 79 28')
        put(render+0x34, 'c1 e8 02 c1 e0 10 05')
        put(render+0x58, '8b 88 10 40 01 00')
        offsets = [(0x11d, 'd8 0d'), (0x136, 'd8 0d'), (0x12f, 'a1'), (0x141, '8b 0d'),
                   (0x154, '3b 3d'), (0x15d, '3b 0d'), (0x1ea, 'a1')]
        operand(other+0x49, '8b 0d', 7)
    for index, (offset, code) in enumerate(offsets):
        operand(render+offset, code, index)
    hline = 0x1c00
    setup = 0x7a if dynamic else 0x4e
    put(render+setup, 'c7 05')
    struct.pack_into('<II', data, render+setup+2, 0x404030, 0x402800)
    operand(hline+0xd5, 'a1', 10)
    if dynamic:
        operand(hline+0x100, '0f af 3d', 11)
    operand(hline+(0x10e if dynamic else 0x10a), '03 3d', 12)
    return data


def recipe():
    return dict(schema=1, animation=0, time_ms=330, center=[0, 50, 0], distance=500,
                yaw=1.57, pitch=0, vertical_half_angle_tangent=.6, near=16, far=6000,
                background=[.18, .2, .24], ambient=1, instance_light=1, brightness=1,
                daylight=[0, 1, 0], fog=[1e9, 2e9], resolution=[960, 720])


class NativeAdapterTests(unittest.TestCase):
    def test_classic_and_dynamic_profiles_locate_checked_code_and_writable_globals(self):
        for dynamic, capacity in [(False, 1024), (True, 8192)]:
            result = original.native_profile(executable(dynamic))
            self.assertEqual(len(result), 18)
            self.assertEqual(result[:4], [123, 0x7000, 0x1100, 0x2400])
            self.assertEqual(result[4:11], list(range(0x4000, 0x401c, 4)))
            self.assertEqual(result[12], capacity)

    def test_unknown_duplicate_and_changed_code_or_read_only_global_are_rejected(self):
        with self.assertRaises(ValueError):
            original.native_profile(b'unrecognized')
        for offset, value in [(0x500+0x11d, 0x90), (0x500+0x58, 0x90)]:
            data = executable(); data[offset] = value
            with self.assertRaises(ValueError): original.native_profile(data)
        data = executable()
        struct.pack_into('<I', data, 0x500+0x11f, 0x401100)
        with self.assertRaises(ValueError): original.native_profile(data)
        data = executable(); data[0x2000:0x2015] = data[0x1800:0x1815]
        with self.assertRaises(ValueError): original.native_profile(data)

    def test_only_matching_neutral_unclipped_recipe_contract_is_accepted(self):
        original.validate_recipe(recipe())
        for key, value in [('ambient', .5), ('brightness', .8), ('near', 64),
                           ('resolution', [1280, 720]), ('resolution', [959, 720]),
                           ('distance', float('nan')), ('schema', 2)]:
            candidate = recipe(); candidate[key] = value
            with self.assertRaises(ValueError): original.validate_recipe(candidate)

    def test_car_parser_preserves_native_animation_samples_and_enforces_capacity(self):
        face = bytearray(64)
        data = bytes(32)+struct.pack('<5I', 1, 0, 1, 1, 512)+face+struct.pack('<3f2h', 1, 2, 3, 0, 0)+bytes(512)
        data += bytes(32)+struct.pack('<iI', 10, 2)+struct.pack('<6h', 8, 16, 24, 16, 24, 32)
        vc, fc, faces, vertices, texture, animation = original.car_payload(data, 0)
        self.assertEqual((vc, fc, len(faces), len(vertices), len(texture)), (1, 1, 64, 16, 512))
        self.assertEqual(animation[:3], (2, 10, 200))
        self.assertEqual(struct.unpack('<6h', animation[3]), (8, 16, 24, 16, 24, 32))
        for broken in [data[:40], data[:-1]]:
            with self.assertRaises(ValueError): original.car_payload(broken, 0)
        with self.assertRaises(ValueError): original.car_payload(data, 1)
        with self.assertRaises(ValueError): original.car_payload(data, 0, capacity=0)

    def test_paths_cannot_escape_and_rgb555_background_is_neutral(self):
        for path in ['../outside', '/absolute', 'C:/x', 'a\\..\\x', '']:
            with self.assertRaises(ValueError): original.safe_relative(path)
        self.assertEqual(original.background_word([255, 255, 255]), 0x7fff)
        self.assertEqual(original.background_word([255, 0, 0]), 31<<10)

    def test_config_pins_only_the_private_copy_renderer_settings(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp)/'config.cfg'
            path.write_text('# comment\nrenderer 1\ncustom keep\nresolution 1920x1080\n')
            original.pin_config(Path(tmp))
            text = path.read_text()
            self.assertIn('renderer 0\n', text)
            self.assertIn('resolution 800x600\n', text)
            self.assertIn('# comment\n', text)
            self.assertIn('custom keep\n', text)
            self.assertNotIn('renderer 1', text)

    def test_pose_mismatch_is_a_capture_failure_even_when_native_image_exists(self):
        from PIL import Image
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); reference = root/'ref'; game = root/'game'; out = root/'out'
            for path in [reference/'poses', reference/'screenshots', game, out]: path.mkdir(parents=True)
            (reference/'poses/pose.f32').write_bytes(struct.pack('<3f', 0, 0, 0))
            (game/'frame.pose').write_bytes(struct.pack('<3f', 1, 0, 0))
            (game/'frame.555').write_bytes(struct.pack('<2I', 128, 128)+bytes(128*128*2))
            capture = dict(file='animal.png', pose_file='poses/pose.f32', recipe=recipe())
            records, errors, differences = original.capture_results(
                [('frame', capture, (0, 0, 0))], game, reference, out,
                dict(channel_tolerance=12, allowed_changed_fraction=.005))
            self.assertEqual(len(errors), 1)
            self.assertIn('native pose mismatch', errors[0])
            self.assertEqual(records[0]['status'], 'invalid capture')


if __name__ == '__main__': unittest.main()
