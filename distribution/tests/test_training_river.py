# SPDX-License-Identifier: GPL-3.0-or-later
"""Offline fixture checks only; no runtime socket or game input is opened."""
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import time
import unittest
from unittest import mock

spec = importlib.util.spec_from_file_location('training_river', Path(__file__).resolve().parents[2] / 'tools/setup-training-river.py')
river = importlib.util.module_from_spec(spec)
spec.loader.exec_module(river)


def report(path=None):
    return {'output': str(path or 'offline.json'), 'identity': {'pid': 123}, 'commands': [],
            'samples': [], 'pressed_keys': [], 'phases': [], 'map_clicks': [],
            'world_receipts': [], 'spawn_requests': [], 'status': 'observation_only'}


def bot_menu():
    return {'objects': [{'path': river.COMBAT_BOT_WIDGET, 'widget':
                        {'visible': True, 'onscreen': True, 'center_uv': [.26, .476]}}]}


class FakeSession:
    instances = []
    fail_spawn = False

    def __init__(self, hero, data, directory):
        self.pid, self.birth = 123, 456
        self.report = data
        data['identity'] = {'pid': self.pid}
        self.calls = []
        self.instances.append(self)

    def expected_hero(self):
        pass

    def controls(self, stage):
        return {}

    def raw_idle(self):
        pass

    def read(self, key):
        if key == 'diagnostics.contacts':
            return {'active': []}
        return {'hero_world': [0., 0.], 'frame_end_ns': time.monotonic_ns()}

    def open_gm(self):
        return bot_menu()

    def tap(self, point):
        self.calls.append(('tap', point))
        if self.fail_spawn:
            raise TimeoutError('Injected lost spawn reply')

    def close_gm(self, point):
        self.calls.append(('close', point))

    def command(self, command, cleanup=False):
        self.calls.append((command, cleanup))
        return 'OK'


class RiverSetupTests(unittest.TestCase):
    def test_world_receipt_requires_finite_coordinates_and_a_fresh_clock(self):
        good = {'hero_world': [3., -3.], 'frame_end_ns': 1_000_000_000}
        self.assertEqual(river.world_position(good, 1_001_000_000), (3., -3.))
        for sample in ({'hero_world': [float('nan'), 0.], 'frame_end_ns': 1_000_000_000},
                       {'hero_world': [0., 0.], 'frame_end_ns': 800_000_000},
                       {'hero_world': [0., 0.], 'frame_end_ns': 1_020_000_000},
                       {'hero_world': [0., 0.]}, {}):
            with self.assertRaises(river.ObservationError):
                river.world_position(sample, 1_001_000_000)

    def test_arrival_waits_for_current_settled_world_samples(self):
        data = report()
        session = FakeSession(118, data, None)
        positions = iter(([20., 20.], [3., -3.], [3.01, -3.], [3.01, -3.]))
        def read(_):
            return {'hero_world': next(positions), 'frame_end_ns': time.monotonic_ns()}
        with mock.patch.object(session, 'read', side_effect=read), mock.patch.object(river.time, 'sleep'):
            arrived = river.wait_world(session, (3., -3.), .9, 'offline')
        self.assertEqual(len(data['world_receipts']), 4)
        self.assertLess(arrived['distance'], .02)

    def test_lost_spawn_reply_keeps_partial_marker_and_never_retries(self):
        FakeSession.instances = []
        FakeSession.fail_spawn = True
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            output = root / 'result.json'
            with mock.patch.object(river, 'ROOT', root), mock.patch.object(river, 'Setup', FakeSession), \
                 mock.patch.object(river, 'teleport', return_value={'world': [3., -3.]}), contextlib.redirect_stdout(io.StringIO()):
                code = river.main(['--hero', '118', '--gm-close-point', '.974', '.1', '--output', str(output)])
            result = json.loads(output.read_text())
            marker = json.loads(next((root / 'logs/hero-bindings').glob('river-fixture-*.json')).read_text())
            self.assertEqual(code, 1)
            self.assertEqual(len(result['spawn_requests']), 1)
            self.assertFalse(result['spawn_requests'][0]['transport_reply_observed'])
            self.assertEqual(marker['status'], 'incomplete_inspect_before_retry')
            self.assertIn(('end', True), FakeSession.instances[0].calls)
            self.assertEqual(sum(c[0] == 'tap' for c in FakeSession.instances[0].calls), 1)

    def test_existing_process_marker_refuses_even_a_different_report(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            marker = root / 'logs/hero-bindings/river-fixture-123-456.json'
            marker.parent.mkdir(parents=True)
            marker.write_text('original')
            output = root / 'another-result.json'
            with mock.patch.object(river, 'ROOT', root), mock.patch.object(river, 'Setup', FakeSession), \
                 mock.patch.object(river, 'teleport') as teleport, contextlib.redirect_stdout(io.StringIO()):
                code = river.main(['--hero', '118', '--gm-close-point', '.974', '.1', '--output', str(output)])
            self.assertEqual(code, 1)
            self.assertEqual(json.loads(output.read_text())['reason'], 'fixture_already_attempted')
            self.assertEqual(marker.read_text(), 'original')
            teleport.assert_not_called()

    def test_unconfirmed_teleport_cannot_spawn_a_bot(self):
        FakeSession.instances = []
        FakeSession.fail_spawn = False
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            output = root / 'result.json'
            with mock.patch.object(river, 'ROOT', root), mock.patch.object(river, 'Setup', FakeSession), \
                 mock.patch.object(river, 'teleport', side_effect=river.ObservationError('teleport_not_confirmed', 'offline')), \
                 contextlib.redirect_stdout(io.StringIO()):
                code = river.main(['--hero', '118', '--gm-close-point', '.974', '.1', '--output', str(output)])
            result = json.loads(output.read_text())
            self.assertEqual(code, 1)
            self.assertEqual(result['spawn_requests'], [])
            self.assertFalse(any(c[0] == 'tap' for c in FakeSession.instances[0].calls))
            self.assertIn(('end', True), FakeSession.instances[0].calls)

    def test_success_records_four_requests_and_five_teleports_without_overclaiming(self):
        FakeSession.instances = []
        FakeSession.fail_spawn = False
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            output = root / 'result.json'
            with mock.patch.object(river, 'ROOT', root), mock.patch.object(river, 'Setup', FakeSession), \
                 mock.patch.object(river, 'teleport', return_value={'world': [0., 0.]}) as teleport, contextlib.redirect_stdout(io.StringIO()):
                code = river.main(['--hero', '118', '--gm-close-point', '.974', '.1', '--output', str(output)])
            result = json.loads(output.read_text())
            self.assertEqual(code, 0)
            self.assertEqual(teleport.call_count, 5)
            self.assertEqual(len(result['spawn_requests']), 4)
            self.assertTrue(all(r['transport_reply_observed'] for r in result['spawn_requests']))
            self.assertFalse(result['mechanical_checks']['native_bot_count_verified'])
            self.assertFalse(result['mechanical_checks']['native_immortality_verified'])
            self.assertEqual(result['status'], 'observation_only')
            self.assertNotIn('pass', result)

    def test_existing_output_prevents_execution(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / 'report.json'
            output.write_text('original')
            with mock.patch.object(river, 'run') as run, contextlib.redirect_stderr(io.StringIO()):
                with self.assertRaises(SystemExit):
                    river.main(['--hero', '118', '--gm-close-point', '.974', '.1', '--output', str(output)])
            run.assert_not_called()
            self.assertEqual(output.read_text(), 'original')


if __name__ == '__main__':
    unittest.main()
