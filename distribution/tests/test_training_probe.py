# SPDX-License-Identifier: GPL-3.0-or-later
"""Offline process/protocol fixtures; no runtime socket or gameplay is used."""
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

spec = importlib.util.spec_from_file_location("training_probe", Path(__file__).resolve().parents[2] / "tools/probe-training-skill.py")
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)


class TrainingProbeTests(unittest.TestCase):
    def child(self, code):
        return subprocess.Popen([sys.executable, "-c", code], stdout=subprocess.PIPE,
                                stderr=subprocess.DEVNULL, start_new_session=True)

    def test_ready_requires_complete_line_and_exact_identity(self):
        for line, accepted in [("READY hero=25 skill=2510 pid=123", True), ("READY hero=26 skill=2510 pid=123", False)]:
            child = self.child(f"import time;print({line!r},flush=True);time.sleep(30)")
            try:
                if accepted:
                    self.assertEqual(probe.wait_ready(child, 25, 2510, 123, io.BytesIO(), timeout=1), line)
                else:
                    with self.assertRaisesRegex(RuntimeError, "does not match"):
                        probe.wait_ready(child, 25, 2510, 123, io.BytesIO(), timeout=1)
            finally:
                probe.stop_watch(child)
                child.stdout.close()
            self.assertIsNotNone(child.poll())

    def test_partial_ready_times_out_and_only_own_watch_is_stopped(self):
        unrelated = self.child("import time;time.sleep(30)")
        child = self.child("import sys,time;sys.stdout.write('READY hero=25');sys.stdout.flush();time.sleep(30)")
        try:
            with self.assertRaises(TimeoutError):
                probe.wait_ready(child, 25, 2510, 123, io.BytesIO(), timeout=.1)
            probe.stop_watch(child)
            self.assertIsNotNone(child.poll())
            self.assertIsNone(unrelated.poll())
        finally:
            probe.stop_watch(child)
            probe.stop_watch(unrelated)
            child.stdout.close()
            unrelated.stdout.close()

    def test_existing_directory_is_refused_before_any_execution(self):
        with tempfile.TemporaryDirectory() as directory:
            original = Path(directory) / "summary.json"
            original.write_text("original")
            with mock.patch.object(probe, "run") as execute, contextlib.redirect_stderr(io.StringIO()):
                with self.assertRaises(SystemExit) as error:
                    probe.main(["--hero", "25", "--key", "Q", "--skill", "2510", "--output-dir", directory])
            self.assertEqual(error.exception.code, 2)
            execute.assert_not_called()
            self.assertEqual(original.read_text(), "original")

    def test_startup_failure_records_error_without_claiming_a_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "new"
            with mock.patch.object(probe, "run", side_effect=TimeoutError("offline READY timeout")), contextlib.redirect_stdout(io.StringIO()):
                code = probe.main(["--hero", "25", "--key", "Q", "--skill", "2510", "--output-dir", str(output)])
            report = json.loads((output / "summary.json").read_text())
            self.assertEqual(code, 1)
            self.assertEqual(report["status"], "observation_only")
            self.assertIn("READY timeout", report["error"])
            self.assertNotIn("pass", report)
            self.assertNotIn("passed", report)

    def test_late_sequences_require_a_receipt_from_the_current_cast(self):
        samples = []
        for sequence, start in [(2, 900_000_000), (3, 1_010_000_000)]:
            samples.append({"stage": "held:4", "end_ns": 1_400_000_000, "data": {
                "input": {"direction_fresh": 1, "direction_accepted": 1, "direction_matched_seq": sequence, "sent": [.7, .8]},
                "aim": {"direction_sample_start_ns": start, "direction_observed": [1., 0.]}}})
        result = probe.late_delivery_metrics({"skill_down_ns": 1_000_000_000, "physical_up_ns": 1_500_000_000, "samples": samples})
        self.assertEqual(result["sample_count"], 2)
        self.assertEqual(result["confirmed_current_cast_receipts"], 1)
        self.assertEqual(result["matched_sequences"], [3])
        self.assertEqual(result["distinct_confirmed_sent_points"], 1)

    def test_failed_native_setup_never_runs_the_gameplay_probe(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            watch = root / "watch"
            watch.write_text(f"#!{sys.executable}\nimport sys\nprint('unsupported registry',file=sys.stderr)\nraise SystemExit(23)\n")
            watch.chmod(0o755)
            args = type("Args", (), dict(output_dir=root, hero=25, skill=2510, key=81, hold_ms=600, movement=False, cancel=False))()
            report = {}
            with mock.patch.object(probe, "WATCH", watch), mock.patch.object(probe, "selected_runtime", return_value=(123, os.getuid())), mock.patch.object(probe.os, "geteuid", return_value=0), mock.patch.object(probe.subprocess, "run") as execute:
                with self.assertRaisesRegex(RuntimeError, "before READY"):
                    probe.run(args, report)
            execute.assert_not_called()
            self.assertTrue(report["watch_stopped"])
            self.assertIn("unsupported registry", (root / "watch.stderr.txt").read_text())

    def test_fake_end_to_end_keeps_actor_evidence_and_mechanical_checks_separate(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            watch, controller = root / "watch", root / "probe.py"
            watch.write_text(f"#!{sys.executable}\n" + '''import json,sys,time
pid,skill=int(sys.argv[1]),int(sys.argv[2]);path=sys.argv[4]
def receipt(n):return {"Ok":{"hero":25,"same_match":True,"end_ns":time.monotonic_ns(),"values":{"actor.m_iFirstSkillUseCount":n,"actor.m_iSecondSkillUseCount":2,"actor.m_iThirdSkillUseCount":3}}}
with open(path,"x") as f:
 f.write(json.dumps({"event":"header","pid":pid,"hero":25,"skill":skill,"initial_cast_receipt":receipt(7)})+"\\n");f.flush()
 print(f"READY hero=25 skill={skill} pid={pid}",flush=True)
 time.sleep(.15)
 f.write(json.dumps({"event":"summary","same_match":True,"identity_unchanged":True,"final_cast_receipt":receipt(8)})+"\\n")
''')
            watch.chmod(0o755)
            controller.write_text('''import json,sys,time
down=time.monotonic_ns();time.sleep(.01);up=time.monotonic_ns()
output=sys.argv[sys.argv.index("--output")+1]
with open(output,"x") as f:json.dump({"identity":{"pid":123},"request":{"hero":25,"key":81,"movement":False},"status":"observation_only","skill_down_ns":down,"physical_up_ns":up,"end_ns":up,"samples":[],"mechanical_checks":{"release_complete_before_forced_end":True},"cleanup_contacts_empty":True,"cleanup_errors":[]},f)
''')
            output = root / "new-output"
            with mock.patch.object(probe, "WATCH", watch), mock.patch.object(probe, "PROBE", controller), mock.patch.object(probe, "selected_runtime", return_value=(123, os.getuid())), mock.patch.object(probe.os, "geteuid", return_value=0), contextlib.redirect_stdout(io.StringIO()):
                code = probe.main(["--hero", "25", "--key", "Q", "--skill", "2510", "--output-dir", str(output)])
            report = json.loads((output / "summary.json").read_text())
            self.assertEqual(code, 0, report.get("error"))
            self.assertEqual(report["status"], "observation_only")
            self.assertEqual(report["mechanical_checks"], {"release_complete_before_forced_end": True})
            self.assertEqual(report["actor_counter_evidence"][0]["local_actor_counter_delta"]["actor.m_iFirstSkillUseCount"], 1)
            self.assertNotIn("pass", report)
            self.assertNotIn("passed", report)
            self.assertTrue(report["watch_stopped"])
