# SPDX-License-Identifier: GPL-3.0-or-later
from concurrent.futures import ThreadPoolExecutor
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import unittest
from unittest.mock import Mock, patch


source = Path(__file__).resolve().parents[2] / "tools/accounts.py"
spec = importlib.util.spec_from_file_location("clbb_accounts", source)
accounts = importlib.util.module_from_spec(spec)
spec.loader.exec_module(accounts)


class AccountSupervisorTests(unittest.TestCase):
    def write_run(self, state, start, marker):
        logs = state / "logs"
        run = logs / f"qemu-launch-{start}-100"
        run.mkdir(parents=True)
        (run / "runtime.log").write_bytes(marker)
        (logs / "current-atl-pid.json").write_text(json.dumps({
            "boot_id": Path("/proc/sys/kernel/random/boot_id").read_text().strip(),
            "launcher_pid": 100, "start_monotonic_ns": start,
        }))

    def test_quit_child_does_not_block_the_next_desktop_launch(self):
        with subprocess.Popen([sys.executable, "-c", "input()"], stdin=subprocess.PIPE) as child:
            with patch.dict(accounts.STARTING, {accounts.PRIMARY: accounts.Launch(child, 1)}, clear=True), \
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

    def test_shortcut_during_teardown_starts_one_new_window(self):
        with tempfile.TemporaryDirectory() as directory, \
             subprocess.Popen([sys.executable, "-c", "input()"], stdin=subprocess.PIPE) as closing:
            root = Path(directory)
            self.write_run(root, 120, b"runtime ready\n\0[CLBB_EXIT] desktop_close\n")
            replacement = Mock()
            replacement.poll.return_value = None
            waiting = threading.Barrier(3, timeout=5)
            real_wait = closing.wait

            def wait_for_close(*args, **kwargs):
                waiting.wait()
                return real_wait(*args, **kwargs)

            with patch.object(accounts, "ROOT", root), \
                 patch.object(accounts, "registry", return_value={"selected": accounts.PRIMARY}), \
                 patch.object(accounts, "names", return_value={accounts.PRIMARY: "Test"}), \
                 patch.object(accounts, "owned", return_value=None), \
                 patch.dict(accounts.STARTING, {accounts.PRIMARY: accounts.Launch(closing, 100)}, clear=True), \
                 patch.object(closing, "wait", side_effect=wait_for_close), \
                 patch.object(accounts.subprocess, "Popen", return_value=replacement) as spawn:
                with ThreadPoolExecutor(2) as pool:
                    first = pool.submit(accounts.launch_from_desktop)
                    second = pool.submit(accounts.launch_from_desktop)
                    try:
                        waiting.wait()
                        self.assertFalse(spawn.called)
                    finally:
                        closing.stdin.write(b"\n")
                        closing.stdin.flush()
                    first.result(timeout=3)
                    second.result(timeout=3)
                spawn.assert_called_once()
                self.assertIs(accounts.STARTING[accounts.PRIMARY].child, replacement)

    def test_bootstrap_live_game_and_updater_ignore_duplicate_shortcuts(self):
        for start, marker in ((50, b"[CLBB_EXIT] desktop_close\n"), (120, b"game ready\n"),
                              (120, b"[CLBB_EXIT] update_requested\n")):
            with self.subTest(start=start, marker=marker), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                self.write_run(root, start, marker)
                child = Mock()
                child.poll.return_value = None
                with patch.object(accounts, "ROOT", root), \
                     patch.object(accounts, "registry", return_value={"selected": accounts.PRIMARY}), \
                     patch.dict(accounts.STARTING, {accounts.PRIMARY: accounts.Launch(child, 100)}, clear=True), \
                     patch.object(accounts.subprocess, "Popen") as spawn:
                    accounts.launch_from_desktop()
                    child.wait.assert_not_called()
                    spawn.assert_not_called()


if __name__ == "__main__":
    unittest.main()
