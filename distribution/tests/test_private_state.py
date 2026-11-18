# SPDX-License-Identifier: GPL-3.0-or-later
import importlib.util
import os
import shutil
import subprocess
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
    def test_launcher_uses_only_the_prepared_identity_and_stops_on_invalid_state(self):
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            runtime, state = base/'runtime', base/'profile'
            (runtime/'distribution').mkdir(parents=True)
            (runtime/'prefix/bin').mkdir(parents=True)
            for filename in ('env.sh', 'run-mlbb.sh', 'prepare-runtime.py'):
                shutil.copy2(ROOT.parent/filename, runtime/filename)
            shutil.copy2(ROOT/'device_identity.py', runtime/'distribution/device_identity.py')
            (runtime/'distribution/guest_root.py').write_text('def prepare(*args): pass\n')
            for filename in ('prepare-native-plugins.py', 'prepare-system-fonts.py'):
                (runtime/filename).write_text('import os\nos.write(1,b"runtime prepared\\n")\n')
            probe = runtime/'prefix/bin/android-translation-layer'
            probe.write_text('#!/bin/sh\nprintf %s "$CLBB_ANDROID_ID" > "$CLBB_STATE_ROOT/seen"\n')
            probe.chmod(0o755)
            env = dict(os.environ, CLBB_STATE_ROOT=str(state), CLBB_ANDROID_ID='inherited')
            value = identity(state)
            result = subprocess.run(['bash',runtime/'run-mlbb.sh'],env=env,capture_output=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual((state/'seen').read_text(), value)
            self.assertNotIn(value.encode(), result.stdout + result.stderr)
            self.assertEqual((result.stdout + result.stderr).count(b'runtime prepared'), 2)
            (state/'seen').unlink()
            (state/'device/android-id').write_text('invalid')
            failure = subprocess.run(['bash',runtime/'run-mlbb.sh'],env=env,capture_output=True)
            self.assertNotEqual(failure.returncode, 0)
            self.assertFalse((state/'seen').exists())
            self.assertIn(b'Invalid local Android identity', failure.stderr)

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
