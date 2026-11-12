# SPDX-License-Identifier: GPL-3.0-or-later
"""Atomic diagnostic transport checks; no runtime socket or input is opened."""
import copy
import importlib.util
import json
from pathlib import Path
import unittest
from unittest import mock

spec = importlib.util.spec_from_file_location("atomic_hero_probe", Path(__file__).resolve().parents[2] / "tools/probe-hero-control.py")
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)


def payload():
    inputs = {"hero": 25, "epoch": "match", "skill": 2520, "touch": 7,
              "direction_accepted": 1, "direction_fresh": 1, "direction_matched_seq": 9007199254740993}
    aim = {"skill": 2520, "touch": 7, "sent_seq": 9007199254740993,
           "sent_time_ns": 2500, "direction_sample_start_ns": 3000, "direction_sample_end_ns": 3500,
           "direction_observed": [.6, .8], **{name: inputs[name] for name in
                ("direction_accepted", "direction_fresh", "direction_matched_seq")}}
    return {"schema": 1, "atomic": True, "snapshot_ns": 4000, "hero": 25, "epoch": "match",
            "source": {"present": True, "fresh": True, "hero": 25, "epoch": "match", "owner_matches_view": True},
            "input": inputs, "aim": aim, "contacts": {"rotation": False, "next": 8, "active": [2, 7]},
            "cast": {"touch": 7, "start_ns": 1000, "key": 87, "held": 1, "mode": 0},
            "cancel": {"available": True, "controller": {"pending": False, "touch": 7}}}


def reader(controller_reply):
    instance = probe.Probe.__new__(probe.Probe)
    instance.hero = 25
    instance.report = {"samples": [], "commands": []}
    instance.in_control = False
    instance.last_input_at = 0.
    instance.controller_supported = None
    legacy = {"input": payload()["input"], "aim": payload()["aim"], "contacts": payload()["contacts"],
              "cancel": payload()["cancel"], "templates": {"hero": 25}, "cast_lock": {"hero": 25}, "gestures": {}}

    def command(value, deadline=None):
        if value == "read diagnostics.controller":
            if isinstance(controller_reply, BaseException):
                raise controller_reply
            return controller_reply
        return json.dumps(legacy[value.removeprefix("read diagnostics.")])
    instance.command = mock.Mock(side_effect=command)
    return instance


class AtomicProbeSnapshotTests(unittest.TestCase):
    def test_combined_reply_preserves_existing_payloads_and_uses_no_split_control_rpc(self):
        original = payload(); instance = reader(json.dumps(original))
        goal = {"command_end_ns": 2000}
        result = instance.snapshot("held:0:settled", deadline=123., goal=goal)
        self.assertTrue(result["atomic"])
        self.assertEqual(result["pointer_goal"], goal)
        for name in probe.CONTROLLER_DIAGNOSTICS:
            self.assertEqual(result["data"][name], original[name])
        self.assertEqual(result["controller"]["source"], original["source"])
        self.assertEqual(result["controller"]["snapshot_ns"], 4000)
        self.assertEqual(result["data"]["aim"]["sent_seq"], 9007199254740993)
        self.assertEqual([call.args[0] for call in instance.command.call_args_list],
                         ["read diagnostics.controller", "read diagnostics.cast_lock", "read diagnostics.templates", "read diagnostics.gestures"])
        self.assertTrue(all(call.kwargs["deadline"] == 123. for call in instance.command.call_args_list))
        self.assertIn("end_ns", result)

    def test_legacy_empty_endpoint_is_explicitly_nonatomic_and_cached_for_same_runtime(self):
        instance = reader("")
        first = instance.snapshot("held:0"); second = instance.snapshot("held:1")
        for result in (first, second):
            self.assertFalse(result["atomic"])
            self.assertEqual(result["controller"], {"available": False, "reason": "unsupported_diagnostic"})
            self.assertEqual(set(result["data"]), set(probe.DIAGNOSTICS))
            self.assertNotIn("cast", result["data"])
        self.assertEqual([call.args[0] for call in instance.command.call_args_list].count("read diagnostics.controller"), 1)

    def test_malformed_or_failed_combined_reply_never_silently_falls_back(self):
        for reply in ("null", "{}", "1", "malformed", TimeoutError("bounded read expired")):
            with self.subTest(reply=reply):
                instance = reader(reply)
                with self.assertRaises((RuntimeError, json.JSONDecodeError, TimeoutError)):
                    instance.snapshot("held:0")
                self.assertEqual([call.args[0] for call in instance.command.call_args_list], ["read diagnostics.controller"])
                self.assertFalse(instance.report["samples"][0]["atomic"])
                self.assertIn("end_ns", instance.report["samples"][0])

    def test_foreign_or_internally_split_controller_scope_is_rejected(self):
        for field in ("hero", "epoch", "touch", "skill", "direction_matched_seq", "direction_fresh", "cast_touch"):
            with self.subTest(field=field):
                value = copy.deepcopy(payload())
                if field == "hero":
                    value["hero"] = 84
                elif field == "epoch":
                    value["input"]["epoch"] = "other-match"
                elif field == "cast_touch":
                    value["cast"]["touch"] = 6
                else:
                    value["aim"][field] += 1
                instance = reader(json.dumps(value))
                with self.assertRaises(RuntimeError):
                    instance.snapshot("held:0")
                self.assertEqual(instance.command.call_count, 1)

    def test_source_absence_or_foreign_source_is_recorded_without_promoting_it_to_current(self):
        for source in ({"present": False, "fresh": False, "hero": None, "epoch": None, "owner_matches_view": None},
                       {"present": True, "fresh": True, "hero": 84, "epoch": "foreign", "owner_matches_view": False}):
            value = payload(); value["source"] = source
            result = reader(json.dumps(value)).snapshot("held:0")
            self.assertTrue(result["atomic"])
            self.assertEqual(result["controller"]["source"], source)
            self.assertEqual(result["data"]["input"]["hero"], 25)

    def test_float_sequence_or_boolean_identity_cannot_masquerade_as_exact_integer_receipt(self):
        for category, field, value in (("aim", "direction_matched_seq", 9007199254740992.),
                                       ("input", "touch", True), ("cast", "touch", True)):
            row = payload(); row[category][field] = value
            instance = reader(json.dumps(row))
            with self.assertRaises(RuntimeError):
                instance.snapshot("held:0")
            self.assertEqual(instance.command.call_count, 1)
