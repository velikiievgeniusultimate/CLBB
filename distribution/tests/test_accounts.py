# SPDX-License-Identifier: GPL-3.0-or-later
import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import unittest
from unittest.mock import patch


source = Path(__file__).resolve().parents[2] / "tools/accounts.py"
spec = importlib.util.spec_from_file_location("clbb_accounts", source)
accounts = importlib.util.module_from_spec(spec)
spec.loader.exec_module(accounts)


class AccountSupervisorTests(unittest.TestCase):
    def test_quit_child_does_not_block_the_next_desktop_launch(self):
        with subprocess.Popen([sys.executable, "-c", "input()"], stdin=subprocess.PIPE) as child:
            with patch.dict(accounts.STARTING, {accounts.PRIMARY: child}, clear=True), \
                 patch.object(accounts, "owned", return_value=None) as owned:
                self.assertTrue(accounts.running(accounts.PRIMARY))
                owned.assert_not_called()
                child.stdin.write(b"\n")
                child.stdin.flush()
                # Make the child waitable while leaving the coordinator responsible for reaping it.
                os.waitid(os.P_PID, child.pid, os.WEXITED | os.WNOWAIT)
                self.assertFalse(accounts.running(accounts.PRIMARY))
                self.assertEqual(child.returncode, 0)
                self.assertNotIn(accounts.PRIMARY, accounts.STARTING)

    def test_an_existing_window_survives_a_coordinator_restart(self):
        with patch.dict(accounts.STARTING, clear=True), patch.object(accounts, "owned", return_value=object()):
            self.assertTrue(accounts.running("secondary"))


if __name__ == "__main__":
    unittest.main()
