# SPDX-License-Identifier: GPL-3.0-or-later
"""Offline selector geometry regressions; never opens a runtime or sends input."""
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

spec = importlib.util.spec_from_file_location('training_selector', Path(__file__).resolve().parents[2] / 'tools/choose-training-hero.py')
selector = importlib.util.module_from_spec(spec)
spec.loader.exec_module(selector)


def state(point=None, visible=True, anchor=None):
    cell = {'data_index': 0, 'hero_id': 126, 'anchor_uv': anchor}
    if point is not None:
        cell['button_geometry'] = {'center_uv': point, 'visible': visible, 'onscreen': True}
    return {'order': [126], 'cells': [cell], 'show_start_index': 0, 'picker_object': 100}


def harness(states):
    instance = object.__new__(selector.Selector)
    instance.snapshot = mock.Mock(side_effect=states)
    instance.runtime = mock.Mock()
    instance.runtime.field.return_value = 200
    instance.runtime.i32.return_value = 126
    return instance


def menu_state(visible=True, point=(.6, .18), owner=10000, button=11000):
    def widget(path, at, shown, obj):
        return {"path": path, "object": obj, "class": "UISprite",
                "widget": {"center_uv": list(at), "visible": shown, "onscreen": True}}
    return {"frames": [{"class": "UITrainGM", "object": owner}], "objects": [
        widget("UITrainGM.uiFrame.m_Sprite_F8_Addbgyx", point, visible, button),
        widget("UITrainGM.uiFrame.m_Sprite_GmIcon", (.25, .06), True, 12000)]}


def picker_state(ready=True):
    return {"frames": [{"class": "UIChooseHeroTrain", "object": 20000}], "objects": [
        {"class": "UIChooseHeroTrain", "path": "UIChooseHeroTrain", "object": 20000},
        *([{"class": "UIListWrapper", "path": "UIChooseHeroTrain.grid", "object": 21000},
           {"path": "UIChooseHeroTrain.m_Label_TrainConfirm", "object": 22000,
            "widget": {"center_uv": [.8, .8], "visible": True, "onscreen": True}}] if ready else [])]}


class FakeClock:
    def __init__(self): self.now = 0.
    def monotonic(self): return self.now
    def monotonic_ns(self): return int(self.now * 1e9)
    def sleep(self, seconds): self.now += seconds


class FakeRuntime:
    pid, birth = 123, "456"
    def __init__(self, clock):
        self.clock, self.taps, self.reads = clock, [], []
        self.changed, self.raw_active, self.contacts = False, False, []
    def identity(self):
        if self.changed: raise RuntimeError("Runtime identity changed")
    def idle(self):
        if self.raw_active: raise RuntimeError("Raw gesture still active")
    def rpc(self, command):
        self.reads.append(command)
        if command != "read diagnostics.contacts": raise AssertionError("Unexpected input/read: " + command)
        return json.dumps({"active": self.contacts})
    def tap(self, point):
        self.taps.append(list(point))
        self.clock.sleep(.2)


class OpenHarness:
    def __init__(self, directory, observe):
        self.clock = FakeClock()
        self.runtime = FakeRuntime(self.clock)
        self.instance = object.__new__(selector.Selector)
        self.instance.runtime, self.instance.directory, self.instance.serial = self.runtime, Path(directory), 0
        self.calls, self.observe = [], observe
        self.instance.audit = self.audit
    def audit(self, name, family=None, timeout=20):
        self.calls.append((name, family, timeout))
        self.instance.serial += 1
        self.clock.sleep(min(.01, timeout))
        return self.observe(name, self)
    def open(self):
        with mock.patch.object(selector.time, 'monotonic', self.clock.monotonic), \
             mock.patch.object(selector.time, 'monotonic_ns', self.clock.monotonic_ns), \
             mock.patch.object(selector.time, 'sleep', self.clock.sleep):
            return self.instance.open()
    def report(self):
        return json.loads((self.instance.directory / '001-open-observations.json').read_text())


class TrainingSelectorTests(unittest.TestCase):
    def test_open_waits_for_delayed_picker_without_extra_taps(self):
        with tempfile.TemporaryDirectory() as directory:
            def observe(name, h):
                if name == 'training-menu': return menu_state(visible=False)
                if name == 'training-menu-open': return menu_state()
                polls = sum(call[0].startswith('picker-open') for call in h.calls)
                return picker_state() if polls >= 3 else menu_state()
            h = OpenHarness(directory, observe)
            h.open()
            self.assertEqual(h.runtime.taps, [[.25, .06], [.6, .18]])
            self.assertEqual(len(h.report()['attempts']), 1)
            self.assertTrue(h.report()['picker_ready'])
            self.assertNotIn('training-menu-retry', [c[0] for c in h.calls])

    def test_open_retries_once_using_fresh_same_owner_geometry(self):
        with tempfile.TemporaryDirectory() as directory:
            def observe(name, _h):
                if name == 'training-menu-retry': return menu_state(point=(.61, .19))
                if name == 'picker-open-2': return picker_state()
                return menu_state()
            h = OpenHarness(directory, observe)
            h.open()
            self.assertEqual(h.runtime.taps, [[.6, .18], [.61, .19]])
            self.assertEqual(len(h.report()['attempts']), 2)
            self.assertTrue(h.report()['picker_ready'])

    def test_open_exhaustion_is_bounded_and_never_clicks_elsewhere(self):
        with tempfile.TemporaryDirectory() as directory:
            h = OpenHarness(directory, lambda name, _h: menu_state())
            with self.assertRaisesRegex(RuntimeError, 'after2 bounded attempts'): h.open()
            self.assertEqual(h.runtime.taps, [[.6, .18], [.6, .18]])
            self.assertLess(h.clock.now, 4.6)
            self.assertEqual(len(h.report()['attempts']), 2)
            self.assertIn('error', h.report())
            self.assertTrue(all(command == 'read diagnostics.contacts' for command in h.runtime.reads))

    def test_open_never_retries_when_owner_visibility_or_runtime_changes(self):
        for change in ('owner', 'hidden', 'runtime'):
            with self.subTest(change=change), tempfile.TemporaryDirectory() as directory:
                def observe(name, h):
                    if name == 'training-menu-retry':
                        if change == 'runtime': h.runtime.changed = True
                        return menu_state(owner=10001 if change == 'owner' else 10000, visible=change != 'hidden')
                    return menu_state()
                h = OpenHarness(directory, observe)
                with self.assertRaises(RuntimeError): h.open()
                self.assertEqual(h.runtime.taps, [[.6, .18]])

    def test_open_raw_or_mapped_contact_prevents_any_tap(self):
        for held in ('raw', 'mapped'):
            with self.subTest(held=held), tempfile.TemporaryDirectory() as directory:
                h = OpenHarness(directory, lambda name, _h: menu_state())
                if held == 'raw': h.runtime.raw_active = True
                else: h.runtime.contacts = [1]
                with self.assertRaises(RuntimeError): h.open()
                self.assertEqual(h.runtime.taps, [])

    def test_existing_picker_is_not_toggled_and_partial_picker_gets_no_gm_retry(self):
        with tempfile.TemporaryDirectory() as directory:
            h = OpenHarness(directory, lambda name, _h: picker_state())
            h.open()
            self.assertEqual(h.runtime.taps, [])
        with tempfile.TemporaryDirectory() as directory:
            h = OpenHarness(directory, lambda name, _h: picker_state(ready=False))
            with self.assertRaisesRegex(RuntimeError, 'Existing hero picker'): h.open()
            self.assertEqual(h.runtime.taps, [])

    def test_audit_timeout_cannot_extend_poll_budget_or_add_a_third_tap(self):
        with tempfile.TemporaryDirectory() as directory:
            def observe(name, h):
                if name.startswith('picker-open'):
                    h.clock.sleep(h.calls[-1][2])
                    raise subprocess.TimeoutExpired('readonly-audit', h.calls[-1][2])
                return menu_state()
            h = OpenHarness(directory, observe)
            with self.assertRaisesRegex(RuntimeError, 'after2 bounded attempts'): h.open()
            self.assertEqual(h.runtime.taps, [[.6, .18], [.6, .18]])
            self.assertLess(h.clock.now, 4.6)
            self.assertEqual(sum(bool(o.get('audit_timeout')) for o in h.report()['observations']), 2)

    def test_rendered_widget_is_valid_even_when_gameobject_anchor_is_none(self):
        cell = state([.85, .56], anchor=None)['cells'][0]
        self.assertEqual(selector.observed_cell_geometry(cell), ([.85, .56], True))
        cell['anchor_uv'] = [.1, .1]
        self.assertEqual(selector.observed_cell_geometry(cell), ([.85, .56], True))
        self.assertEqual(selector.observed_cell_geometry({'anchor_uv': [.85, .56]}), (None, False))

    def test_missing_geometry_reobserves_then_clicks_without_scrolling(self):
        instance = harness([state(), state([.85, .56])])
        with mock.patch.object(selector.time, 'sleep'):
            instance.choose(126, 0)
        instance.runtime.swipe.assert_not_called()
        instance.runtime.tap.assert_called_once_with([.85, .56])
        self.assertEqual(instance.snapshot.call_count, 2)

    def test_persistent_missing_geometry_is_bounded_and_sends_no_input(self):
        instance = harness([state()] * 4)
        with mock.patch.object(selector.time, 'sleep'), self.assertRaisesRegex(RuntimeError, 'three reobservations'):
            instance.choose(126, 12)
        self.assertEqual(instance.snapshot.call_count, 4)
        instance.runtime.swipe.assert_not_called()
        instance.runtime.tap.assert_not_called()

    def test_hidden_interior_widget_reobserves_instead_of_overscrolling(self):
        instance = harness([state([.85, .56], visible=False), state([.85, .56])])
        with mock.patch.object(selector.time, 'sleep'):
            instance.choose(126, 1)
        instance.runtime.swipe.assert_not_called()
        instance.runtime.tap.assert_called_once_with([.85, .56])

    def test_partially_clipped_irithel_portrait_has_a_safe_native_hit_point(self):
        point = [.5598946768987085, .20600478849114268]
        snapshot = state(point)
        snapshot["cells"][0]["button_geometry"]["radius_uv"] = [.03435135880955342, .04416760930325836]
        press = selector.visible_cell_press(snapshot["cells"][0])
        self.assertAlmostEqual(press[0], point[0])
        self.assertAlmostEqual(press[1], .23008619889720052)
        self.assertGreater(press[1], .21)
        instance = harness([snapshot])
        instance.choose(126, 0)
        instance.runtime.swipe.assert_not_called()
        instance.runtime.tap.assert_called_once_with(press)

    def test_tiny_clipped_sliver_and_hidden_portraits_are_not_clicked(self):
        snapshot = state([.56, .7763331031330196])
        cell = snapshot["cells"][0]
        cell["button_geometry"]["radius_uv"] = [.03435135880955342, .04416760930325836]
        self.assertIsNone(selector.visible_cell_press(cell))
        cell["button_geometry"]["center_uv"] = [.56, .206]
        cell["button_geometry"]["visible"] = False
        self.assertIsNone(selector.visible_cell_press(cell))
        cell["button_geometry"]["visible"] = True
        cell["button_geometry"]["radius_uv"] = [.034, -1.]
        self.assertIsNone(selector.visible_cell_press(cell))

    def test_known_offscreen_geometry_preserves_correct_scroll_direction(self):
        for first_y, downward in ((.88, False), (.10, True)):
            with self.subTest(first_y=first_y):
                instance = harness([state([.85, first_y], visible=False), state([.85, .56])])
                with mock.patch.object(selector.time, 'sleep'):
                    instance.choose(126, 1)
                instance.runtime.swipe.assert_called_once_with(downward)
                instance.runtime.tap.assert_called_once_with([.85, .56])


if __name__ == '__main__':
    unittest.main()
