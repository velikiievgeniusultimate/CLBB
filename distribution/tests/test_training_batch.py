# SPDX-License-Identifier: GPL-3.0-or-later
"""Offline batch orchestration checks: no runtime socket or gameplay input."""
import contextlib
import argparse
import copy
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import time
from types import SimpleNamespace
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("training_batch", ROOT / "tools/visit-training-heroes.py")
batch = importlib.util.module_from_spec(spec)
spec.loader.exec_module(batch)


def controls(hero=130):
    now = time.monotonic_ns()
    buttons = []
    for index, key in enumerate((81, 87, 69, 82)):
        buttons.append({"slot": f"skill:{index}", "kind": "skill", "index": index,
                        "skill_id": 1_013_010 + index * 20, "key": key,
                        "available": True, "cast_mode": 0, "press": [.7, .8]})
    return {"ready": True, "hero": hero, "epoch": f"owned:test:{hero}", "sample_start_ns": now - 1_000_000,
            "sample_end_ns": now, "buttons": buttons, "native_buttons": copy.deepcopy(buttons),
            "capability_controls": [], "types": {f"skill:{i}": v for i, v in enumerate((1, 2, 3, 2))}, "holds": {}}


def snapshot(hero=130):
    return {"hero": hero, "controls": controls(hero), "contacts": {"active": []},
            "delivery": {"raw_gesture_active": False},
            "input": {"hero": hero, "ready": 1, "enabled": 1, "touch": -1, "pending_release": 0},
            "gestures": {"active": []}}


def audit(hero=130):
    data = {"hero": hero, "pid": 123, "birth": 456, "same_match_at_end": True,
            "types": {f"skill:{i}": v for i, v in enumerate((1, 2, 3, 2))}, "buttons": [], "variants": []}
    for index, kind in enumerate((1, 2, 3, 2)):
        base = 1_013_010 + index * 20
        data["buttons"].append({"kind": "skill", "slot": f"skill:{index}", "base_skill": base, "active": True, "template": kind})
        data["variants"].append({"slot": f"skill:{index}", "base": base, "active": base + 1_000_000})
    # Common controls cannot become hero skill probes even if they have a type.
    data["buttons"].append({"kind": "extra", "slot": "extra:2", "base_skill": 20020, "active": True, "template": 2})
    return data


def cast_lock(hero=130):
    now = time.monotonic_ns()
    return {"hero": hero, "epoch": f"owned:test:{hero}", "start_ns": now - 1_000_000,
            "end_ns": now, "buttons": [{"slot": f"skill:{i}", "form": 2_013_010 + i * 20,
                                         "locked": False, "state_forbidden": False} for i in range(4)]}


class FakeAnchor:
    def __init__(self):
        self.pid, self.birth, self.executable = 123, "456", Path("/owned/clbb")
        self.hero, self.changed, self.held = 25, False, False
        self.reader = SimpleNamespace(read=self.read)

    def identity(self):
        if self.changed:
            raise batch.BatchAbort("Selected runtime process changed")

    proof = identity

    def describe(self):
        return {"pid": self.pid, "birth_ticks": self.birth, "executable": str(self.executable)}

    def selected_runtime(self):
        self.identity()
        return self.pid, 1000

    def read(self, key):
        if key == "diagnostics.cast_lock":
            return cast_lock(self.hero)
        if key == "display.width":
            return 4000
        if key == "display.height":
            return 3111
        raise AssertionError(f"Unexpected offline read: {key}")

    def preflight(self, expected=None):
        self.identity()
        value = snapshot(self.hero)
        if self.held:
            value["contacts"]["active"] = [1]
        batch.validate_idle_state(value, expected)
        return value


def coordinator(directory, basic=False):
    anchor = FakeAnchor()
    args = SimpleNamespace(heroes=[25, 97], output_dir=Path(directory), basic_probes=basic,
                           gm_close_point=(.974, .1))
    report = {"heroes": [], "status": "observation_only"}
    instance = batch.Coordinator(args, report, io.StringIO(), anchor=anchor)
    calls = []

    def choose(hero, _directory):
        calls.append(("choose", hero))
        anchor.hero = hero

    instance.choose = mock.Mock(side_effect=choose)
    instance.setup = mock.Mock(side_effect=lambda hero, _directory: calls.append(("setup", hero)))
    instance.audit = mock.Mock(side_effect=lambda hero, _directory, name="contract": audit(hero))
    instance.probe = mock.Mock(side_effect=lambda hero, _plan, _directory: calls.append(("probe", hero)))
    instance.return_center = mock.Mock(side_effect=lambda hero, _directory: calls.append(("center", hero)))
    return instance, anchor, report, calls


class TrainingBatchTests(unittest.TestCase):
    def test_hero_list_is_bounded_and_never_accepts_duplicate_or_shell_text(self):
        self.assertEqual(batch.parse_heroes("25, 97,126"), [25, 97, 126])
        for value in ("", "25,25", "0", "10000", "25,,97", "25;touch x", "25,$(true)", "-1"):
            with self.subTest(value=value), self.assertRaises((argparse.ArgumentTypeError, ValueError, RuntimeError, TypeError, SystemExit)):
                batch.parse_heroes(value)

    def test_preflight_requires_fresh_owned_hero_and_empty_mapped_and_raw_contacts(self):
        self.assertEqual(batch.validate_idle_state(snapshot(), expected_hero=130), 130)
        for update in ({"hero": 25}, {"contacts": {"active": [1]}}, {"contacts": {}},
                       {"delivery": {"raw_gesture_active": True}}, {"delivery": {}},
                       {"input": {"hero": 130, "ready": 0, "enabled": 1, "touch": -1, "pending_release": 0}},
                       {"input": {"hero": 130, "ready": 1, "enabled": 0, "touch": -1, "pending_release": 0}},
                       {"input": {"hero": 25, "ready": 1, "enabled": 1, "touch": -1, "pending_release": 0}},
                       {"input": {"hero": 130, "ready": 1, "enabled": 1, "touch": 1, "pending_release": 0}},
                       {"input": {"hero": 130, "ready": 1, "enabled": 1, "touch": -1, "pending_release": 1}},
                       {"gestures": {"active": [{"key": 81}]}}):
            value = snapshot(); value.update(update)
            with self.subTest(update=update), self.assertRaises((ValueError, RuntimeError)):
                batch.validate_idle_state(value, expected_hero=130)
        stale = snapshot()
        stale["controls"]["sample_start_ns"] -= 1_000_000_000
        stale["controls"]["sample_end_ns"] -= 1_000_000_000
        with self.assertRaises((ValueError, RuntimeError)):
            batch.validate_idle_state(stale, expected_hero=130)
        wrong_controls = snapshot()
        wrong_controls["controls"]["hero"] = 25
        with self.assertRaises((ValueError, RuntimeError)):
            batch.validate_idle_state(wrong_controls, expected_hero=130)

    def test_probe_plan_uses_actual_large_native_ids_not_hero_arithmetic(self):
        plan = batch.plan_probes(audit(), controls())
        rows = {row["slot"]: row for row in plan}
        self.assertEqual(set(rows), {f"skill:{i}" for i in range(4)})
        for index, key in enumerate((81, 87, 69, 82)):
            row = rows[f"skill:{index}"]
            self.assertFalse(row.get("skip_reason"))
            self.assertEqual(row["skill"], 2_013_010 + index * 20)
            actual_key = ord(row["key"]) if isinstance(row["key"], str) else row["key"]
            self.assertEqual(actual_key, key)
            self.assertEqual(row["hold_ms"], 300 if index == 0 else 650)

    def test_unknown_type_unavailable_skill_and_user_binding_are_skipped_explicitly(self):
        data, current = audit(), controls()
        data["types"].pop("skill:0")
        data["buttons"][0]["template"] = None
        data["buttons"][1]["active"] = False
        current["buttons"][2]["key"] = 88
        rows = {row["slot"]: row for row in batch.plan_probes(data, current)}
        for slot in ("skill:0", "skill:1", "skill:2"):
            self.assertTrue(rows[slot].get("skip_reason"), slot)
        self.assertFalse(rows["skill:3"].get("skip_reason"))

    def test_existing_output_directory_is_rejected_before_constructing_coordinator(self):
        with tempfile.TemporaryDirectory() as directory:
            original = Path(directory) / "summary.json"
            original.write_text("existing evidence")
            with mock.patch.object(batch, "Coordinator") as coordinator, contextlib.redirect_stderr(io.StringIO()):
                with self.assertRaises(SystemExit) as error:
                    batch.main(["--heroes", "25,97", "--output-dir", directory])
            self.assertEqual(error.exception.code, 2)
            coordinator.assert_not_called()
            self.assertEqual(original.read_text(), "existing evidence")

    def test_default_audit_only_still_visits_prepares_and_returns_each_hero(self):
        with tempfile.TemporaryDirectory() as directory:
            instance, anchor, report, calls = coordinator(directory)
            with contextlib.redirect_stdout(io.StringIO()):
                instance.run()
            self.assertEqual(calls, [("choose", 25), ("setup", 25), ("center", 25),
                                     ("choose", 97), ("setup", 97), ("center", 97)])
            instance.probe.assert_not_called()
            self.assertTrue(report["sequence_finished"])
            self.assertEqual(report["status"], "observation_only")
            for brief in report["heroes"]:
                item = json.loads(Path(brief["path"]).read_text())
                self.assertTrue(brief["workflow_finished"])
                self.assertTrue(item["skipped"])
                self.assertTrue(all(row["skip_reason"] == "basic_probes_not_requested" for row in item["skipped"]))
                self.assertNotIn("passed", item)

    def test_process_change_after_setup_aborts_before_audit_or_next_hero(self):
        with tempfile.TemporaryDirectory() as directory:
            instance, anchor, report, calls = coordinator(directory)
            instance.setup.side_effect = lambda _hero, _directory: setattr(anchor, "changed", True)
            with contextlib.redirect_stdout(io.StringIO()), self.assertRaisesRegex(batch.BatchAbort, "process changed"):
                instance.run()
            instance.audit.assert_not_called()
            instance.probe.assert_not_called()
            instance.return_center.assert_not_called()
            self.assertEqual(instance.choose.call_count, 1)
            self.assertEqual(len(report["heroes"]), 1)
            self.assertFalse(report["heroes"][0]["workflow_finished"])
            self.assertNotIn("sequence_finished", report)

    def test_failed_probe_cleanup_stops_without_teleport_or_second_hero(self):
        with tempfile.TemporaryDirectory() as directory:
            instance, anchor, report, calls = coordinator(directory, basic=True)
            instance.probe.side_effect = batch.BatchAbort("Child cleanup failure")
            with contextlib.redirect_stdout(io.StringIO()), self.assertRaisesRegex(batch.BatchAbort, "cleanup failure"):
                instance.run()
            self.assertEqual(instance.probe.call_count, 1)
            instance.return_center.assert_not_called()
            self.assertEqual(instance.choose.call_count, 1)
            self.assertFalse(report["heroes"][0]["workflow_finished"])
            item = json.loads(Path(report["heroes"][0]["path"]).read_text())
            self.assertIn("cleanup failure", item["error"])
            self.assertFalse(item.get("workflow_finished", False))

    def test_preflight_held_contact_refuses_first_hero_change(self):
        with tempfile.TemporaryDirectory() as directory:
            instance, anchor, report, calls = coordinator(directory)
            anchor.held = True
            with contextlib.redirect_stdout(io.StringIO()), self.assertRaisesRegex(batch.BatchAbort, "contacts"):
                instance.run()
            self.assertEqual(calls, [])
            instance.choose.assert_not_called()
            self.assertEqual(report["heroes"], [])

    def test_real_probe_guard_rejects_bad_cleanup_even_when_child_returns_zero(self):
        with tempfile.TemporaryDirectory() as directory:
            args = SimpleNamespace(gm_close_point=(.974, .1))
            instance = batch.Coordinator(args, {"heroes": []}, io.StringIO(), anchor=FakeAnchor())
            instance.hero_report = {"probes": []}
            original = batch.probe_tool.selected_runtime

            def fake_child(argv):
                output = Path(argv[argv.index("--output-dir") + 1])
                output.mkdir()
                (output / "summary.json").write_text(json.dumps({"cleanup_errors": ["lost release"],
                    "cleanup_contacts_empty": False, "watch_stopped": True,
                    "mechanical_checks": {"release_complete_before_forced_end": True},
                    "watch_integrity_checks": {"same_match": True}}))
                return 0

            with mock.patch.object(batch.probe_tool, "main", side_effect=fake_child), self.assertRaisesRegex(batch.BatchAbort, "cleanup/integrity"):
                instance.probe(25, batch.plan_probes(audit(25), controls(25))[0], Path(directory))
            self.assertIs(batch.probe_tool.selected_runtime, original)
            self.assertEqual(len(instance.hero_report["probes"]), 1)

    def test_readonly_audit_requires_matching_pid_and_native_same_match_receipt(self):
        with tempfile.TemporaryDirectory() as directory:
            instance = batch.Coordinator(SimpleNamespace(), {"heroes": []}, io.StringIO(), anchor=FakeAnchor())
            instance.hero_report = {"audit_paths": []}
            for index, change in enumerate(({"pid": 999}, {"same_match_at_end": False})):
                def fake_child(command, **_kwargs):
                    value = audit(25)
                    value.update(change)
                    Path(command[2]).write_text(json.dumps(value))
                    return SimpleNamespace(returncode=0)
                with mock.patch.object(batch.subprocess, "run", side_effect=fake_child), self.assertRaisesRegex(batch.BatchAbort, "identity/match"):
                    instance.audit(25, Path(directory), f"bad-owner-{index}")
            self.assertEqual(instance.hero_report["audit_paths"], [])

    def test_setup_cannot_substitute_upgrade_exhaustion_for_native_max_level(self):
        with tempfile.TemporaryDirectory() as directory:
            instance = batch.Coordinator(SimpleNamespace(gm_close_point=(.974, .1)), {}, io.StringIO(), anchor=FakeAnchor())
            instance.hero_report = {}
            original = batch.setup_tool.Setup
            def fake_setup(argv):
                output = Path(argv[argv.index('--output') + 1])
                output.write_text(json.dumps({'cleanup_contacts_empty': True,
                    'learning': {'available_upgrades_exhausted': True},
                    'max_level': {'requested': True, 'level_15_verified': False}}))
                return 0
            with mock.patch.object(batch.setup_tool, 'main', side_effect=fake_setup), self.assertRaisesRegex(batch.BatchAbort, 'level15'):
                instance.setup(25, Path(directory))
            self.assertIs(batch.setup_tool.Setup, original)
            self.assertNotIn('setup', instance.hero_report)

    def test_later_audit_independently_checks_native_level_even_after_setup(self):
        with tempfile.TemporaryDirectory() as directory:
            instance = batch.Coordinator(SimpleNamespace(), {}, io.StringIO(), anchor=FakeAnchor())
            instance.hero_report = {'audit_paths': []}
            for index, level in enumerate((None, 1, 15)):
                def fake_child(command, **_kwargs):
                    now = time.monotonic_ns()
                    value = audit(25); value['executable'] = '/owned/clbb'
                    value['hero_level'] = {'Err': 'unknown'} if level is None else {'Ok': {
                        'hero': 25, 'value': level, 'show_level': level,
                        'source': 'EntityBase.<m_Level>k__BackingField', 'declaring_class': 'EntityBase',
                        'field': '<m_Level>k__BackingField', 'kind': 8, 'show_declaring_class': 'ShowEntity',
                        'show_field': 'm_Level', 'show_kind': 8, 'coherent': True, 'local_owner_verified': True,
                        'start_ns': now - 3_000_000, 'end_ns': now}}
                    Path(command[2]).write_text(json.dumps(value))
                    return SimpleNamespace(returncode=0)
                with mock.patch.object(batch.subprocess, 'run', side_effect=fake_child):
                    if level == 15:
                        self.assertEqual(instance.audit(25, Path(directory), f'level-{index}')['hero_level']['Ok']['value'], 15)
                    else:
                        with self.assertRaises(batch.setup_tool.ObservationError):
                            instance.audit(25, Path(directory), f'level-{index}')
                        self.assertEqual(instance.hero_report['audit_paths'], [])
            self.assertEqual(len(instance.hero_report['audit_paths']), 1)

    def test_cached_probe_plan_requires_current_mapped_identity_and_a_fresh_exact_form(self):
        plan = batch.plan_probes(audit(), controls())[0]
        self.assertIsNone(batch.probe_plan_status(plan, snapshot(), cast_lock()))
        bad_variants = []
        for change in ({"buttons": []}, {"hero": 25}, {"epoch": "different-owned-epoch"}, {}):
            value = cast_lock()
            if change:
                value.update(change)
            else:
                value["start_ns"] -= 1_000_000_000
                value["end_ns"] -= 1_000_000_000
            bad_variants.append(value)
        wrong_form = cast_lock()
        wrong_form["buttons"][0]["form"] += 1
        bad_variants.append(wrong_form)
        duplicate = cast_lock()
        duplicate["buttons"].append(copy.deepcopy(duplicate["buttons"][0]))
        bad_variants.append(duplicate)
        for value in bad_variants:
            with self.subTest(cast_lock=value):
                self.assertTrue(batch.probe_plan_status(plan, snapshot(), value))
        for change in ({"key": 88}, {"skill_id": 1013011}, {"available": False}):
            value = snapshot()
            value["controls"]["buttons"][0].update(change)
            with self.subTest(mapped=change):
                self.assertTrue(batch.probe_plan_status(plan, value, cast_lock()))
        changed_type = snapshot()
        changed_type["controls"]["types"]["skill:0"] = 3
        self.assertTrue(batch.probe_plan_status(plan, changed_type, cast_lock()))

    def test_final_probe_guard_never_launches_a_child_with_missing_variant_or_changed_binding(self):
        for reason in ("missing_variant", "changed_binding"):
            with self.subTest(reason=reason), tempfile.TemporaryDirectory() as directory:
                anchor = FakeAnchor()
                instance = batch.Coordinator(SimpleNamespace(), {"heroes": []}, io.StringIO(), anchor=anchor)
                instance.hero_report = {"probes": []}
                if reason == "missing_variant":
                    anchor.reader.read = lambda _key: {**cast_lock(25), "buttons": []}
                else:
                    good = anchor.preflight
                    def changed(expected=None):
                        value = good(expected)
                        value["controls"]["buttons"][0]["key"] = 88
                        return value
                    anchor.preflight = changed
                with mock.patch.object(batch.probe_tool, "main") as child, self.assertRaises(batch.BatchAbort):
                    instance.probe(25, batch.plan_probes(audit(25), controls(25))[0], Path(directory))
                child.assert_not_called()

    def test_missing_current_variant_refreshes_readonly_contract_then_skips_old_plan(self):
        with tempfile.TemporaryDirectory() as directory:
            instance, anchor, report, calls = coordinator(directory, basic=True)
            anchor.reader.read = lambda _key: {**cast_lock(anchor.hero), "buttons": []}
            with contextlib.redirect_stdout(io.StringIO()):
                instance.run()
            instance.probe.assert_not_called()
            self.assertTrue(report["sequence_finished"])
            for brief in report["heroes"]:
                item = json.loads(Path(brief["path"]).read_text())
                self.assertTrue(any(row["phase"].startswith("contract-refresh-") for row in item["phases"]))
                self.assertEqual(len(item["skipped"]), 4)
                self.assertTrue(all(row.get("skip_reason") and row["skip_reason"] != "basic_probes_not_requested"
                                    for row in item["skipped"]))


if __name__ == "__main__":
    unittest.main()
