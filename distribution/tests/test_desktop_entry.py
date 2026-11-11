# SPDX-License-Identifier: GPL-3.0-or-later
import importlib.util
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
spec = importlib.util.spec_from_file_location('desktop_entry', Path(__file__).resolve().parents[1] / 'desktop_entry.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

class DesktopEntryTests(unittest.TestCase):
    def test_entry_is_valid_and_kde_cache_is_refreshed_on_reregistration(self):
        with tempfile.TemporaryDirectory(prefix='clbb desktop ') as temp:
            home = Path(temp)
            launcher = home / '.local/bin/clbb'
            launcher.parent.mkdir(parents=True)
            launcher.write_text('#!/bin/sh\nexit 0\n')
            launcher.chmod(0o755)
            calls = []
            def refresh(args, **kwargs):
                calls.append(args)
                return subprocess.CompletedProcess(args, 0, '', '')
            with patch.object(module.shutil, 'which', side_effect=lambda name: '/usr/bin/'+name), patch.object(module.subprocess, 'run', side_effect=refresh):
                desktop = module.register(home, home / 'data')
                self.assertIn('kbuildsycoca6', calls[-1][0])
                self.assertFalse(any('kbuildsycoca5' in row[0] for row in calls))
                desktop.unlink()
                module.register(home, home / 'data')
            self.assertTrue(desktop.is_file())
            self.assertEqual(desktop.stat().st_mode & 0o777, 0o644)
            if module.shutil.which('desktop-file-validate'):
                subprocess.run(['desktop-file-validate', str(desktop)], check=True)

    def test_missing_launcher_is_an_explicit_failure(self):
        with tempfile.TemporaryDirectory() as temp:
            with self.assertRaisesRegex(RuntimeError, 'launcher missing'):
                module.register(temp, Path(temp) / 'data')
