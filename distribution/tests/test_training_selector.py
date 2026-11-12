# SPDX-License-Identifier: GPL-3.0-or-later
"""Offline selector geometry regressions; never opens a runtime or sends input."""
import importlib.util
from pathlib import Path
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


class TrainingSelectorTests(unittest.TestCase):
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
