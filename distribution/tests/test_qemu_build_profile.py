# SPDX-License-Identifier: MIT
from contextlib import redirect_stdout, redirect_stderr
from importlib.util import module_from_spec, spec_from_file_location
from io import StringIO
from pathlib import Path
from tempfile import TemporaryDirectory
import unittest
from unittest.mock import patch

spec = spec_from_file_location('qemu_profile', Path(__file__).resolve().parents[2] / 'tools/qemu-build-profile.py')
profile = module_from_spec(spec)
spec.loader.exec_module(profile)


class BuildProfileTests(unittest.TestCase):
    def test_relocates_both_gcc_naming_schemes_and_rejects_incomplete_cached_data(self):
        with TemporaryDirectory() as temp:
            root = Path(temp)
            counts = root / 'input'
            counts.mkdir()
            core = root / 'training/core'
            bridge = root / 'training/bridge'
            for name in ('translate-a64', 'cpu-exec'):
                (counts / (str(core).replace('/', '#') + '#' + name + '.gcda')).write_bytes(b'adcg-core')
            objects = counts / bridge.relative_to('/')
            objects.mkdir(parents=True)
            (objects / 'libnb-qemu.cpp.gcda').write_bytes(b'adcg-bridge')
            with patch.object(profile, 'CACHE', root/'cache'), patch.object(profile, 'signature', return_value='gcc'), \
                 patch.object(profile, 'CORE_BUILD', root/'checkout/qemu/_build'), \
                 patch.object(profile, 'BRIDGE_BUILD', root/'checkout/builddir/libnb-qemu'):
                target = profile.install(counts, core, bridge)
                self.assertEqual(len(list(target.rglob('*.gcda'))), 3)
                with redirect_stdout(StringIO()) as found:
                    profile.locate()
                self.assertEqual(found.getvalue().strip(), str(target))
                (target / profile.BRIDGE_BUILD.relative_to('/') / 'libnb-qemu.cpp.gcda').unlink()
                with redirect_stdout(StringIO()) as output, redirect_stderr(StringIO()):
                    profile.locate()
                self.assertEqual(output.getvalue(), '')
                self.assertEqual(profile.install(counts, core, bridge), target)
                self.assertEqual(profile.counter_digest(target), target.name)
