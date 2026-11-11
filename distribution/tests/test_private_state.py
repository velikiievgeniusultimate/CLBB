# SPDX-License-Identifier: GPL-3.0-or-later
import importlib.util
from pathlib import Path
import tempfile
import unittest
ROOT = Path(__file__).resolve().parents[1]
def load(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / (name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module
identity = load('device_identity').identity
guest = load('guest_root').prepare
class PrivateStateTests(unittest.TestCase):
    def test_ids_are_distinct_stable_and_reset_with_state(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            a, b = identity(root/'a'), identity(root/'b')
            self.assertNotEqual(a, b)
            self.assertEqual(a, identity(root/'a'))
            self.assertEqual((root/'a/device/android-id').stat().st_mode & 0o777, 0o600)
            (root/'a/device/android-id').unlink()
            self.assertNotEqual(a, identity(root/'a'))
    def test_invalid_identity_never_uses_a_shared_default(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp)
            identity(path)
            (path/'device/android-id').write_text('_totally_an_androidID')
            with self.assertRaises(ValueError): identity(path)
    def test_guest_fonts_resolve_inside_each_installation(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            prefix = root/'prefix/share/libnb-qemu-guest'
            prefix.mkdir(parents=True)
            (prefix/'libc.so').write_bytes(b'fixture')
            for name in ('one', 'two'):
                state=root/name
                (state/'system/etc').mkdir(parents=True)
                (state/'system/fonts').mkdir()
                (state/'system/fonts/font.ttf').write_bytes(b'font')
                for config in ('fonts.xml','system_fonts.xml','fallback_fonts.xml'):
                    (state/'system/etc'/config).write_text('<familyset/>')
                target=guest(state, root/'prefix')
                self.assertEqual((target/'system/fonts/font.ttf').read_bytes(), b'font')
                self.assertEqual((target/'system/etc/fonts.xml').resolve(), state/'system/etc/fonts.xml')
                self.assertEqual((target/'libc.so').read_bytes(), b'fixture')
                self.assertEqual(target, guest(state, root/'prefix'))
