# SPDX-License-Identifier: GPL-3.0-or-later
"""Deterministic diagnostic scheduling and receipt classification, without input."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


probe = module("settling_probe", ROOT / "tools/probe-hero-control.py")
analysis = module("settling_analysis", ROOT / "tools/analyze-hero-probes.py")


class Clock:
    def __init__(self):
        self.now = 10.


class FakeProbe:
    def __init__(self, clock, cost):
        self.clock, self.cost = clock, cost
        self.report = {"commands": [], "samples": []}
        self.waits = []

    def wait_until(self, deadline):
        self.waits.append(deadline)
        self.clock.now = max(self.clock.now, deadline)

    def send_pointer(self, pointer, deadline=None):
        if self.clock.now >= deadline:
            raise TimeoutError("No hold budget")
        now = round(self.clock.now * 1e9)
        self.report["commands"].append({"command": f"pointer {pointer[0]} {pointer[1]}", "start_ns": now, "end_ns": now})

    def snapshot(self, stage, deadline=None, goal=None):
        sample = {"stage": stage, "start_ns": round(self.clock.now * 1e9), "pointer_goal": dict(goal), "data": {}}
        self.report["samples"].append(sample)
        if self.clock.now + self.cost >= deadline:
            self.clock.now = deadline
            sample["end_ns"] = round(deadline * 1e9)
            raise TimeoutError("Hold deadline reached while reading diagnostics")
        self.clock.now += self.cost
        sample["end_ns"] = round(self.clock.now * 1e9)
        return sample


def sample(stage="held:1:settled", source_start=3000, matched=4, sent=4):
    aim = {"logical_fresh": 1, "desired_valid": 1, "desired": [3., 4.], "offset": [0., 0.],
           "range": 5., "logical_start_ns": source_start, "accepted_ns": source_start + 10, "matched_seq": matched}
    if sent is not None:
        aim["sent_seq"] = sent
    return {"stage": stage, "start_ns": 4000, "end_ns": 5000,
            "pointer_goal": {"command_end_ns": 2000},
            "data": {"input": {"direct": 1, "accepted": 1}, "aim": aim}}


class ProbeSettlingTests(unittest.TestCase):
    def test_all_supported_hold_lengths_keep_the_absolute_deadline_with_slow_reads(self):
        for hold in (16, 17, 40, 600, 5000):
            for settle in (0, 1, 30, 250):
                for cost in (.0001, .005, .2):
                    with self.subTest(hold=hold, settle=settle, cost=cost):
                        clock = Clock()
                        controller = FakeProbe(clock, cost)
                        with mock.patch.object(probe.time, "monotonic", side_effect=lambda: clock.now):
                            probe.hold_waypoints(controller, controller.report, 10., hold, settle)
                        deadline = 10. + hold / 1000.
                        self.assertAlmostEqual(clock.now, deadline)
                        self.assertTrue(all(at <= deadline for at in controller.waits))
                        for row in controller.report["samples"]:
                            self.assertLess(row["start_ns"], round(deadline * 1e9))
                            self.assertLessEqual(row["end_ns"], round(deadline * 1e9))
                            self.assertLessEqual(row["pointer_goal"].get("settle_wait_ms", 0), min(settle, hold / len(probe.WAYPOINTS) / 3.) + 1e-8)

    def test_default_zero_settling_keeps_only_immediate_snapshots(self):
        clock = Clock(); controller = FakeProbe(clock, .0001)
        with mock.patch.object(probe.time, "monotonic", side_effect=lambda: clock.now):
            probe.hold_waypoints(controller, controller.report, 10., 600, 0)
        self.assertEqual([row["stage"] for row in controller.report["samples"]], [f"held:{i}" for i in range(6)])
        clock = Clock(); controller = FakeProbe(clock, .0001)
        with mock.patch.object(probe.time, "monotonic", side_effect=lambda: clock.now):
            probe.hold_waypoints(controller, controller.report, 10., 600, 30)
        self.assertEqual([row["stage"] for row in controller.report["samples"]], [stage for i in range(6) for stage in (f"held:{i}", f"held:{i}:settled")])

    def test_old_feedback_and_missing_latest_sequence_are_never_confirmed_accuracy(self):
        p = {"skill_down_ns": 1000}
        old = analysis.range_observation(sample(source_start=1500), p)
        self.assertEqual(old["classification"], "previous_or_unproven_goal")
        self.assertFalse(old["exact_current_goal_ack"])
        temporal = analysis.range_observation(sample(sent=None), p)
        self.assertEqual(temporal["classification"], "post_goal_temporal_only")
        self.assertFalse(temporal["exact_current_goal_ack"])
        split_rpc = sample(sent=None)
        split_rpc["data"]["input"]["sent_seq"] = 4
        self.assertFalse(analysis.range_observation(split_rpc, p)["exact_current_goal_ack"])
        unmatched = analysis.range_observation(sample(sent=5), p)
        self.assertEqual(unmatched["classification"], "newest_command_not_acknowledged")
        self.assertFalse(unmatched["exact_current_goal_ack"])
        exact = analysis.range_observation(sample(), p)
        self.assertEqual(exact["classification"], "current_goal_command_ack")
        self.assertTrue(exact["exact_current_goal_ack"])
        self.assertEqual(exact["error_world"], 5.)

    def test_phase_statistics_separate_observations_from_confirmed_current_goal(self):
        immediate = sample(stage="held:1", source_start=1500)
        settled = sample(sent=None)
        p = {"request": {"hero": 118, "key": 81}, "skill_down_ns": 1000, "physical_up_ns": 6000, "samples": [immediate, settled]}
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "probe.json"
            path.write_text(json.dumps(p))
            result = analysis.analyze(path, [])
            self.assertIsNone(result["confirmed_native_range_vector_error_world"])
            self.assertEqual(result["range_feedback_by_phase"]["immediate"]["observed_vector_difference_world"]["n"], 1)
            self.assertEqual(result["range_feedback_by_phase"]["settled"]["post_goal_temporal_difference_world"]["n"], 1)
            self.assertIsNone(result["range_feedback_by_phase"]["settled"]["confirmed_current_goal_vector_error_world"])
            settled["data"]["aim"]["sent_seq"] = 4
            path.write_text(json.dumps(p))
            result = analysis.analyze(path, [])
            self.assertEqual(result["confirmed_native_range_vector_error_world"]["n"], 1)
            self.assertEqual(result["range_feedback_by_phase"]["settled"]["confirmed_current_goal_vector_error_world"]["n"], 1)

    def test_legacy_probe_goal_is_recovered_from_latest_completed_pointer_command(self):
        row = sample(source_start=1500)
        row.pop("pointer_goal")
        p = {"skill_down_ns": 1000, "commands": [{"command": "pointer .6 .5", "end_ns": 2000}, {"command": "pointer .4 .5", "end_ns": 4500}]}
        result = analysis.range_observation(row, p)
        self.assertEqual(result["goal_command_end_ns"], 2000)
        self.assertFalse(result["exact_current_goal_ack"])
