# SPDX-License-Identifier: GPL-3.0-or-later
"""Offline helper checks only: no runtime socket or gameplay input is opened."""
import argparse
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

spec = importlib.util.spec_from_file_location("hero_probe", Path(__file__).resolve().parents[2] / "tools/probe-hero-control.py")
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)


def sample(contacts, touch=-1, pending=0, movement=-1, at=10):
    return {"stage": "held:0", "end_ns": at, "data": {
        "input": {"touch": touch, "pending_release": pending, "movement_touch": movement},
        "contacts": {"active": contacts}}}


class HeroProbeTests(unittest.TestCase):
    def test_parameter_bounds_and_keyboard_codes(self):
        self.assertEqual(probe.key_code("q"), 81)
        self.assertEqual(probe.key_code("81"), 81)
        self.assertEqual(probe.key_code("SPACE"), 32)
        for value in ("0", "-1", "0x02000002", "Q\nend", "щ"):
            with self.assertRaises(argparse.ArgumentTypeError):
                probe.key_code(value)
        for value in ("15", "5001", "nan", "1.5"):
            with self.assertRaises(argparse.ArgumentTypeError):
                probe.bounded_int(16, 5000)(value)

    def test_release_excludes_only_the_intentionally_held_movement_finger(self):
        self.assertTrue(probe.release_state(sample([0], movement=0), True)["released"])
        self.assertFalse(probe.release_state(sample([0], movement=0), False)["released"])
        self.assertFalse(probe.release_state(sample([0, 1], touch=1, movement=0), True)["released"])
        self.assertFalse(probe.release_state(sample([], pending=1), False)["released"])
        self.assertFalse(probe.release_state(sample(None), False)["released"])
        self.assertFalse(probe.release_state(sample([True]), False)["released"])

    def test_auto_release_requires_ordered_contact_then_release_while_key_is_held(self):
        contact = sample([1], touch=1, at=20)
        empty_before = sample([], at=10)
        empty_after = sample([], at=30)
        self.assertFalse(probe.observations([empty_before, contact], False, 40)["release_before_physical_up_observed"])
        self.assertTrue(probe.observations([contact, empty_after], False, 40)["release_before_physical_up_observed"])
        self.assertFalse(probe.observations([contact, empty_after], False, 25)["release_before_physical_up_observed"])

    def test_existing_report_is_never_overwritten_or_used_to_run_a_probe(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "evidence.json"
            path.write_text("original")
            with mock.patch.object(probe, "run") as execute, contextlib.redirect_stderr(io.StringIO()):
                with self.assertRaises(SystemExit) as error:
                    probe.main(["--hero", "25", "--key", "Q", "--output", str(path)])
            self.assertEqual(error.exception.code, 2)
            execute.assert_not_called()
            self.assertEqual(path.read_text(), "original")

    def test_report_records_error_without_claiming_gameplay_success(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "evidence.json"
            with mock.patch.object(probe, "run", side_effect=RuntimeError("offline injected failure")), contextlib.redirect_stdout(io.StringIO()):
                code = probe.main(["--hero", "25", "--key", "Q", "--output", str(path)])
            report = json.loads(path.read_text())
            self.assertEqual(code, 1)
            self.assertIn("offline injected failure", report["error"])
            self.assertEqual(report["status"], "observation_only")
            self.assertNotIn("pass", report)
            self.assertEqual(report["commands"], [])

    def test_even_successful_mechanical_checks_remain_observations(self):
        def finish(_args, report):
            report["mechanical_checks"] = {"release_complete_before_forced_end": True}
            report["cleanup_contacts_empty"] = True
            report["cleanup_errors"] = []
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "evidence.json"
            with mock.patch.object(probe, "run", side_effect=finish), contextlib.redirect_stdout(io.StringIO()):
                code = probe.main(["--hero", "84", "--key", "W", "--cancel", "--output", str(path)])
            report = json.loads(path.read_text())
            self.assertEqual(code, 0)
            self.assertEqual(report["status"], "observation_only")
            self.assertEqual(report["release_mode"], "explicit_cancel")
            self.assertNotIn("pass", report)
