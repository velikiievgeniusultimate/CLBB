# SPDX-License-Identifier: GPL-3.0-or-later
"""Offline raw-native probe checks; all process/time/input observations are fake."""
import argparse
import copy
import contextlib
import importlib.util
import io
import json
from pathlib import Path
from types import SimpleNamespace
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("nativeprobe", ROOT / "tools/probe-native-touch.py")
nativeprobe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(nativeprobe)


def number(value):
    return {"kind": 1, "value": value}


def flag(value):
    return {"value": int(value)}


def text(value):
    return {"present": 1, "size": len(value.encode()), "value": value}


def vector(values):
    return {"count": len(values), "values": [number(v) for v in values]}


def native_frame(now=10_000_000_000, hero=118, active=11810):
    epoch = f"owned-native:{hero}"
    button = {"slot": text("skill:0"), "kind": text("skill"), "index": number(0),
              "skill_id": number(11810), "active": flag(True),
              "press_uv": vector([.768, .914]), "center_uv": vector([.768, .87]),
              "radius_uv": vector([.1, .13]), "geometry_source": text("native_widget_transform")}
    frame = {"ready": flag(True), "hero_id": number(hero), "time_ns": number(now),
             "sample_duration_ms": number(.25), "token": text(epoch), "token_string": flag(True),
             "token_nonempty": flag(True), "game_clock_ms": number(1234), "skill_id": number(0),
             "ui": {"ready": flag(True), "hero_id": number(hero), "time_ns": number(now),
                    "active_slot": {"present": 0, "size": 0, "value": ""},
                    "resolution_px": vector([4000, 3111]),
                    "buttons": [button], "button_count": 1, "cancel_uv": vector([.835, .406])},
             "skill_state": {"ready": flag(True), "hero_id": number(hero), "token": text(epoch),
                             "source": text("ShowSkillData.m_TranID"), "sample_start_ns": number(now - 250_000),
                             "sample_end_ns": number(now), "count": 1,
                             "buttons": [{"slot": text("skill:0"), "base_skill_id": number(11810),
                                          "active_skill_id": number(active), "cast_locked": flag(False),
                                          "state_forbidden": flag(False)}]}}
    return {"fresh": True, "hero": hero, "epoch": epoch, "frame": frame,
            "game_clock_ms": 1234, "plan_revision": 1, "types": {"skill:0": 3},
            "control_holds": {"skill:0": False}}


class FakeClock:
    def __init__(self):
        self.now = 10.

    def monotonic(self):
        return self.now

    def monotonic_ns(self):
        return round(self.now * 1e9)

    def sleep(self, seconds):
        self.now += max(0., seconds)


class FakeProbe:
    def __init__(self, clock, report):
        self.clock, self.report = clock, report
        self.hero, self.raw_until = 118, None
        self.mapped_contacts, self.busy_raw, self.active_slot = [], False, False
        self.native_reads, self.active_after_native_reads = 0, None
        self.last_touch_status = "dispatched"
        self.lose_hold_reply, self.lose_move_reply, self.invisible_hold = False, False, False
        self.expire_held_diagnostic = False
        self.identity_changes_at, self.hero_changes_at = None, None
        self.last_hero_check = None
        self.raw_commands, self.identity_calls = [], 0

    def identity(self):
        self.identity_calls += 1
        if self.identity_changes_at is not None and self.clock.now >= self.identity_changes_at:
            raise RuntimeError("Owned runtime PID/birth changed")

    def read(self, key, deadline=None):
        self.identity()
        if key == "hero.id":
            self.last_hero_check = self.clock.now
            return 25 if self.hero_changes_at is not None and self.clock.now >= self.hero_changes_at else self.hero
        if key == "diagnostics.delivery":
            active = self.busy_raw or (self.raw_until is not None and self.clock.now < self.raw_until)
            return {"raw_gesture_active": False if self.invisible_hold and self.raw_until is not None else active,
                    "last_touch_status": self.last_touch_status}
        if key == "diagnostics.contacts":
            return {"active": list(self.mapped_contacts)}
        if key == "diagnostics.input":
            return {"hero": self.hero, "ready": 1, "enabled": 1, "touch": -1, "pending_release": 0}
        if key == "diagnostics.gestures":
            return {"active": []}
        if key == "diagnostics.cancel":
            return {"available": False, "reason": "unsupported_diagnostic"}
        if key == "diagnostics.native_frame":
            self.native_reads += 1
            if self.expire_held_diagnostic and self.raw_until is not None and self.clock.now < self.raw_until:
                self.expire_held_diagnostic = False
                self.clock.now = self.raw_until
                raise TimeoutError("Diagnostic reached the original native hold deadline")
            frame = native_frame(self.clock.monotonic_ns(), self.hero)
            if self.active_slot or (self.active_after_native_reads is not None
                                    and self.native_reads >= self.active_after_native_reads):
                frame["frame"]["ui"]["active_slot"] = text("skill:0")
            return frame
        raise AssertionError(f"Unexpected offline read: {key}")

    def command(self, command, deadline=None, cleanup=False):
        # The fake is intentionally stricter than a command recorder: tests fail
        # if run_input mutates without a current identity+hero guard.
        self.identity()
        if self.last_hero_check != self.clock.now:
            raise AssertionError("Raw mutation was not preceded by a current hero check")
        self.report["commands"].append({"command": command, "start_ns": self.clock.monotonic_ns()})
        self.raw_commands.append(command)
        if command.startswith("hold "):
            if self.raw_until is not None:
                raise AssertionError("Native hold was restarted")
            self.raw_until = self.clock.now + int(command.split()[-1]) / 1000.
            if self.lose_hold_reply:
                raise TimeoutError("Lost native hold reply")
            return "OK hold"
        if command.startswith("move-held "):
            if self.raw_until is None or self.clock.now >= self.raw_until:
                raise AssertionError("MOVE was sent after the original native hold expired")
            if self.lose_move_reply:
                raise TimeoutError("Lost native MOVE reply")
            return "OK move-held"
        raise AssertionError(f"Native baseline issued forbidden/reset command: {command}")


def arguments(**changes):
    values = {"hero": 118, "slot": "skill:0", "skill": 11810, "hold_ms": 600,
              "moves": [[.6, .5], [.4, .7], [.2, .3]], "cancel_uv": None}
    values.update(changes)
    return SimpleNamespace(**values)


def fake_time(clock):
    return mock.patch.multiple(nativeprobe.time, monotonic=clock.monotonic,
                               monotonic_ns=clock.monotonic_ns, sleep=clock.sleep)


class NativeTouchProbeTests(unittest.TestCase):
    def test_moves_are_bounded_finite_uv_pairs(self):
        self.assertEqual(nativeprobe.parse_moves("[[0.6,0.5],[0.1,1]]"), [[.6, .5], [.1, 1.]])
        valid = json.dumps([[.5, .5]] * 16)
        self.assertEqual(len(nativeprobe.parse_moves(valid)), 16)
        for value in ("[]", "null", "{}", "[[true,false]]", "[[0.5]]", "[[0.5,0.5,0.5]]",
                      "[[NaN,0.5]]", "[[0.5,Infinity]]", "[[-0.1,0.5]]", "[[1.1,0.5]]",
                      json.dumps([[.5, .5]] * 17)):
            with self.subTest(value=value), self.assertRaises((argparse.ArgumentTypeError, ValueError, RuntimeError, TypeError)):
                nativeprobe.parse_moves(value)

    def test_move_schedule_is_inside_the_original_timer_and_cancel_is_only_explicit(self):
        moves = [[.6, .5], [.4, .7], [.2, .3]]
        for hold in (16, 200, 600, 5000):
            result = nativeprobe.make_schedule(hold, moves)
            self.assertEqual([row["kind"] for row in result], ["move"] * 3)
            offsets = [row["offset_ms"] for row in result]
            self.assertEqual(offsets, sorted(set(offsets)))
            self.assertTrue(all(0 < value < hold for value in offsets))
            self.assertAlmostEqual(offsets[0], min(80, hold / 4))
            self.assertAlmostEqual(offsets[-1], hold - min(80, hold / 4))
            self.assertEqual([list(row["uv"]) for row in result], moves)
            cancelled = nativeprobe.make_schedule(hold, moves, [.835, .406])
            self.assertEqual(cancelled[-1]["kind"], "cancel")
            self.assertEqual(list(cancelled[-1]["uv"]), [.835, .406])
            self.assertTrue(all(0 < row["offset_ms"] < hold for row in cancelled))

    def test_native_button_requires_typed_fresh_owned_exact_skill(self):
        data = native_frame()
        selected = nativeprobe.validate_native_button(data, 118, "skill:0", 11810, now_ns=10_001_000_000)
        self.assertEqual(selected["base_skill"], 11810)
        self.assertEqual(list(selected["press"]), [.768, .914])
        self.assertEqual(selected["epoch"], "owned-native:118")
        transformed = native_frame(active=11811)
        self.assertEqual(nativeprobe.validate_native_button(transformed, 118, "skill:0", 11811, now_ns=10_001_000_000)["active_skill"], 11811)
        invalid = []
        value = native_frame(); value["fresh"] = False; invalid.append(value)
        value = native_frame(); value["hero"] = 25; invalid.append(value)
        value = native_frame(); value["frame"]["hero_id"]["kind"] = 0; invalid.append(value)
        value = native_frame(); value["frame"]["ready"]["value"] = -1; invalid.append(value)
        value = native_frame(); value["frame"]["time_ns"]["value"] -= 1_000_000_000; invalid.append(value)
        value = native_frame(); value["frame"]["ui"]["buttons"][0]["active"]["value"] = 0; invalid.append(value)
        value = native_frame(); value["frame"]["ui"]["buttons"][0]["skill_id"]["value"] = 2510; invalid.append(value)
        value = native_frame(); value["frame"]["ui"]["buttons"][0]["press_uv"]["count"] = 0; invalid.append(value)
        value = native_frame(); value["frame"]["ui"]["buttons"].append(copy.deepcopy(value["frame"]["ui"]["buttons"][0])); value["frame"]["ui"]["button_count"] = 2; invalid.append(value)
        value = native_frame(); value["frame"]["skill_state"]["ready"] = flag(False); invalid.append(value)
        value = native_frame(); value["frame"]["skill_state"]["hero_id"] = number(25); invalid.append(value)
        value = native_frame(); value["frame"]["skill_state"]["token"] = text("stale-owned-epoch"); invalid.append(value)
        value = native_frame(); value["frame"]["skill_state"]["sample_end_ns"]["value"] -= 1; invalid.append(value)
        for value in invalid:
            with self.subTest(data=value), self.assertRaises((ValueError, RuntimeError, TypeError)):
                nativeprobe.validate_native_button(value, 118, "skill:0", 11810, now_ns=10_001_000_000)
        with self.assertRaises((ValueError, RuntimeError)):
            nativeprobe.validate_native_button(data, 118, "skill:0", 11810, now_ns=10_001_000_000, epoch="another-owned-epoch")

    def test_busy_mapped_raw_or_native_skill_preflight_never_sends_hold(self):
        for change in ("mapped_contacts", "busy_raw", "active_slot"):
            with self.subTest(change=change):
                clock = FakeClock(); report = {"commands": [], "samples": []}; probe = FakeProbe(clock, report)
                setattr(probe, change, [1] if change == "mapped_contacts" else True)
                with fake_time(clock), self.assertRaises(RuntimeError):
                    nativeprobe.run_input(arguments(), report, probe)
                self.assertEqual(probe.raw_commands, [])

    def test_only_the_exact_absent_text_sentinel_means_no_active_slot(self):
        self.assertEqual(nativeprobe.active_slot({"active_slot": {"present": 0, "size": 0, "value": ""}}), "")
        self.assertEqual(nativeprobe.active_slot({"active_slot": text("")}), "")
        self.assertEqual(nativeprobe.active_slot({"active_slot": text("skill:0")}), "skill:0")
        for value in (None, {}, {"present": 0, "size": 1, "value": ""},
                      {"present": 0, "size": 0, "value": "skill:0"},
                      {"present": False, "size": 0, "value": ""},
                      {"present": 0, "size": False, "value": ""},
                      {"present": 1, "size": 6, "value": "skill:0"}):
            with self.subTest(value=value), self.assertRaises(RuntimeError):
                nativeprobe.active_slot({"active_slot": value})

    def test_a_skill_becoming_active_between_preflight_and_down_aborts_without_hold(self):
        clock = FakeClock(); report = {"commands": [], "samples": []}; probe = FakeProbe(clock, report)
        probe.active_after_native_reads = 2
        with fake_time(clock), self.assertRaises(RuntimeError):
            nativeprobe.run_input(arguments(), report, probe)
        self.assertEqual(probe.native_reads, 2)
        self.assertEqual(probe.raw_commands, [])

    def test_raw_path_uses_one_timed_hold_and_natural_up_without_mapper_resets(self):
        clock = FakeClock(); report = {"commands": [], "samples": []}; probe = FakeProbe(clock, report)
        with fake_time(clock):
            nativeprobe.run_input(arguments(), report, probe)
        self.assertEqual(sum(command.startswith("hold ") for command in probe.raw_commands), 1)
        self.assertEqual(sum(command.startswith("move-held ") for command in probe.raw_commands), 3)
        self.assertTrue(all(command.startswith(("hold ", "move-held ")) for command in probe.raw_commands))
        self.assertAlmostEqual(probe.raw_until, 10.6)
        self.assertGreaterEqual(clock.now, probe.raw_until)
        self.assertTrue(report["hold_accepted"])
        self.assertTrue(report["cleanup_raw_idle"])
        self.assertTrue(report["cleanup_contacts_empty"])
        self.assertEqual(report["cleanup_errors"], [])
        self.assertTrue(report["mechanical_checks"]["all_requested_moves_acknowledged"])
        self.assertGreater(probe.identity_calls, len(probe.raw_commands))

    def test_explicit_cancel_is_a_final_native_move_without_forced_up(self):
        clock = FakeClock(); report = {"commands": [], "samples": []}; probe = FakeProbe(clock, report)
        with fake_time(clock):
            nativeprobe.run_input(arguments(cancel_uv=[.835, .406]), report, probe)
        self.assertEqual(report["move_receipts"][-1]["kind"], "cancel")
        self.assertEqual(report["move_receipts"][-1]["uv"], [.835, .406])
        self.assertTrue(report["mechanical_checks"]["cancel_drag_requested"])
        self.assertNotEqual(report["mechanical_checks"]["cancellation_result"], True)
        self.assertAlmostEqual(probe.raw_until, 10.6)
        self.assertTrue(report["cleanup_raw_idle"])
        self.assertTrue(all(command.startswith(("hold ", "move-held ")) for command in probe.raw_commands))

    def test_idle_raw_finger_without_dispatched_up_is_not_complete_observation(self):
        clock = FakeClock(); report = {"commands": [], "samples": []}; probe = FakeProbe(clock, report)
        probe.last_touch_status = "rejected"
        with fake_time(clock):
            nativeprobe.run_input(arguments(), report, probe)
        self.assertTrue(report["cleanup_raw_idle"])
        self.assertTrue(report["cleanup_contacts_empty"])
        self.assertFalse(report["native_up_dispatch_acknowledged"])
        self.assertFalse(report["mechanical_checks"]["native_up_dispatch_acknowledged"])
        self.assertEqual(report["cleanup_last_touch_status"], "rejected")
        # The public CLI must also retain the distinction, even with a complete
        # synthetic watch and mechanically acknowledged native moves.
        def fake_run(args, target):
            target.update(report, watch_stopped=True, watch_returncode=0,
                          watch_integrity_checks={"complete": True})
        with tempfile.TemporaryDirectory() as directory, mock.patch.object(nativeprobe, "run", fake_run):
            output = Path(directory) / "new-observation"
            with contextlib.redirect_stdout(io.StringIO()):
                code = nativeprobe.main(["--hero", "118", "--slot", "skill:0", "--skill", "11810",
                                         "--expected-pid", "123", "--output-dir", str(output)])
            self.assertEqual(code, 1)
            saved = json.loads((output / "summary.json").read_text())
            self.assertFalse(saved["native_up_dispatch_acknowledged"])

    def test_lost_hold_reply_waits_for_the_original_natural_up_without_retry(self):
        for invisible in (False, True):
            with self.subTest(delivery_idle_despite_lost_reply=invisible):
                clock = FakeClock(); report = {"commands": [], "samples": []}; probe = FakeProbe(clock, report)
                probe.lose_hold_reply, probe.invisible_hold = True, invisible
                with fake_time(clock), self.assertRaises(TimeoutError):
                    nativeprobe.run_input(arguments(), report, probe)
                self.assertGreaterEqual(clock.now, probe.raw_until)
                self.assertEqual(len(probe.raw_commands), 1)
                self.assertTrue(report["cleanup_raw_idle"])
                self.assertTrue(report["cleanup_contacts_empty"])
                self.assertEqual(report["cleanup_errors"], [])

    def test_lost_move_reply_does_not_restart_or_extend_the_native_timer(self):
        clock = FakeClock(); report = {"commands": [], "samples": []}; probe = FakeProbe(clock, report)
        probe.lose_move_reply = True
        with fake_time(clock), self.assertRaises(TimeoutError):
            nativeprobe.run_input(arguments(), report, probe)
        self.assertAlmostEqual(probe.raw_until, 10.6)
        self.assertGreaterEqual(clock.now, probe.raw_until)
        self.assertEqual(len(probe.raw_commands), 2)
        self.assertTrue(report["cleanup_contacts_empty"])
        self.assertEqual(report["cleanup_errors"], [])

    def test_diagnostic_timeout_never_adds_a_new_hold_or_extends_its_deadline(self):
        clock = FakeClock(); report = {"commands": [], "samples": []}; probe = FakeProbe(clock, report)
        probe.expire_held_diagnostic = True
        with fake_time(clock):
            nativeprobe.run_input(arguments(), report, probe)
        self.assertAlmostEqual(probe.raw_until, 10.6)
        self.assertAlmostEqual(clock.now, 10.6)
        self.assertEqual(len(probe.raw_commands), 2)
        self.assertTrue(any(row.get("truncated_at_hold_deadline") for row in report["samples"]))
        self.assertTrue(report["cleanup_raw_idle"])
        self.assertFalse(report["mechanical_checks"]["all_requested_moves_acknowledged"])

    def test_hero_or_process_change_prevents_later_raw_moves(self):
        for changed in ("hero_changes_at", "identity_changes_at"):
            with self.subTest(changed=changed):
                clock = FakeClock(); report = {"commands": [], "samples": []}; probe = FakeProbe(clock, report)
                setattr(probe, changed, 10.05)
                with fake_time(clock), self.assertRaises(RuntimeError):
                    nativeprobe.run_input(arguments(), report, probe)
                self.assertEqual(len(probe.raw_commands), 1)
                self.assertTrue(probe.raw_commands[0].startswith("hold "))
                self.assertTrue(report["cleanup_errors"])

    def test_native_probe_subclass_guards_raw_mutations_and_forbids_mapper_commands(self):
        probe = nativeprobe.NativeTouchProbe.__new__(nativeprobe.NativeTouchProbe)
        probe.hero = 118
        probe.identity = mock.Mock()
        probe.read = mock.Mock(return_value=118)
        clock = FakeClock()
        with fake_time(clock), mock.patch.object(nativeprobe.hero_probe.Probe, "command", return_value="OK hold") as send:
            self.assertEqual(probe.command("hold .7 .8 600"), "OK hold")
            probe.identity.assert_called()
            probe.read.assert_called_with("hero.id", deadline=11.)
            send.assert_called_once()
            self.assertEqual(send.call_args.kwargs["deadline"], 11.)
            self.assertFalse(send.call_args.kwargs["cleanup"])
        for command in ("begin", "end", "pointer .5 .5", "key 81 1", "mouse 2 1"):
            with mock.patch.object(nativeprobe.hero_probe.Probe, "command") as send, self.assertRaises(RuntimeError):
                probe.command(command)
            send.assert_not_called()
        probe.read.return_value = 25
        with mock.patch.object(nativeprobe.hero_probe.Probe, "command") as send, self.assertRaises(RuntimeError):
            probe.command("move-held .6 .5")
        send.assert_not_called()


if __name__ == "__main__":
    unittest.main()
