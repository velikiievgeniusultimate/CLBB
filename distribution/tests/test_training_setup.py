# SPDX-License-Identifier: GPL-3.0-or-later
"""Offline setup-tool checks. No socket, UI audit or native input is opened."""
import contextlib
import argparse
import copy
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import time
import unittest
from unittest import mock

spec = importlib.util.spec_from_file_location('training_setup', Path(__file__).resolve().parents[2] / 'tools/setup-training-hero.py')
setup = importlib.util.module_from_spec(spec)
spec.loader.exec_module(setup)


def controls(now=1_000_000_000, available=True):
    return {'ready': True, 'hero': 97, 'epoch': 'owned:test:97',
            'sample_start_ns': now - 1_000_000, 'sample_end_ns': now,
            'buttons': [{'kind': 'upgrade', 'slot': 'upgrade:0', 'index': 0, 'available': available, 'key': 49}],
            'native_buttons': [{'kind': 'upgrade', 'slot': 'upgrade:0', 'index': 0, 'available': available,
                                'press_valid': True, 'press': [.71, .84]}],
            'capability_controls': [{'kind': 'upgrade', 'slot': 'upgrade:0', 'available': False}]}


def gm(opened=True):
    rows = [{'path': setup.MAX_LEVEL, 'widget': {'visible': opened, 'onscreen': True, 'center_uv': [.26, .125]}}]
    for _, (overlay, _, desired) in setup.GM.items():
        rows.append({'path': 'UITrainGM.uiFrame.' + overlay,
                     'widget': {'visible': desired, 'onscreen': True, 'center_uv': [.26, .18]}})
    return {'objects': rows}


def level_report(now=1_000_000_000, level=15):
    return {'pid': 123, 'birth': 456, 'hero': 97, 'same_match_at_end': True,
            'executable': '/owned/clbb', 'hero_level': {'Ok': {
                'hero': 97, 'source': 'EntityBase.<m_Level>k__BackingField',
                'declaring_class': 'EntityBase', 'field': '<m_Level>k__BackingField', 'kind': 8,
                'show_declaring_class': 'ShowEntity', 'show_field': 'm_Level', 'show_kind': 8,
                'coherent': True, 'local_owner_verified': True, 'value': level, 'show_level': level,
                'start_ns': now - 3_000_000, 'end_ns': now}}}


class FakeSession:
    instances = []
    fail_down = False
    upgrades = True

    def __init__(self, hero, report, directory):
        self.report = report
        self.calls = []
        self.in_control = False
        self.instances.append(self)

    def expected_hero(self):
        pass

    def controls(self, stage):
        return controls(time.monotonic_ns(), self.upgrades)

    def read(self, key):
        return {'active': []}

    def command(self, command, cleanup=False):
        self.calls.append((command, cleanup))
        self.report['commands'].append({'command': command})
        if command.startswith('read hotkeys.'):
            return '16777270'
        if self.fail_down and command == 'key 49 1':
            raise TimeoutError('Injected lost DOWN reply')
        return 'OK handled=1'

    def send_pointer(self, point):
        pass

    def wait_until(self, deadline):
        pass


class TrainingSetupTests(unittest.TestCase):
    def test_max_level_requires_fresh_typed_owned_actor_and_show_receipts(self):
        good = level_report()
        self.assertEqual(setup.validate_hero_level(good, 97, 123, '456', now_ns=1_002_000_000,
            expected_executable='/owned/clbb')['value'], 15)
        changes = [('pid', 999), ('birth', 999), ('hero', 25), ('same_match_at_end', False),
                   ('executable', '/other/clbb')]
        for key, value in changes:
            data = copy.deepcopy(good); data[key] = value
            with self.subTest(header=key), self.assertRaises(setup.ObservationError):
                setup.validate_hero_level(data, 97, 123, 456, now_ns=1_002_000_000,
                    expected_executable='/owned/clbb')
        for key, value in [('field', '_level'), ('kind', 9), ('local_owner_verified', 1),
                           ('coherent', False), ('show_level', 1), ('value', True),
                           ('end_ns', 700_000_000), ('start_ns', float('nan'))]:
            data = copy.deepcopy(good); data['hero_level']['Ok'][key] = value
            with self.subTest(native=key), self.assertRaises(setup.ObservationError):
                setup.validate_hero_level(data, 97, 123, 456, now_ns=1_002_000_000)
        data = copy.deepcopy(good); data['hero_level'] = {'Err': 'owner changed'}
        with self.assertRaises(setup.ObservationError) as error:
            setup.validate_hero_level(data, 97, 123, 456, now_ns=1_002_000_000)
        self.assertEqual(error.exception.reason, 'hero_level_unknown')
        with self.assertRaises(setup.ObservationError) as error:
            setup.validate_hero_level(level_report(level=1), 97, 123, 456, now_ns=1_002_000_000)
        self.assertEqual(error.exception.reason, 'hero_level_not_maximum')

    def test_unknown_level_after_max_button_stops_before_skill_learning(self):
        for verified in (False, True):
            session = mock.Mock()
            session.read.return_value = {'active': []}
            session.open_gm.return_value = gm()
            if verified:
                session.verify_level.return_value = {'receipt': level_report()}
            else:
                session.verify_level.side_effect = setup.ObservationError('hero_level_not_maximum', 'Observed level1')
            report = {'commands': [], 'samples': [], 'phases': [], 'pressed_keys': []}
            args = argparse.Namespace(hero=97, evidence=Path('/unused'), max_level=True,
                configure_flags=False, verify_flags=False, gm_close_point=(.974, .1), rounds=1, hold_ms=100)
            with mock.patch.object(setup, 'Setup', return_value=session), mock.patch.object(setup, 'learn') as learn:
                if verified:
                    setup.run(args, report)
                    learn.assert_called_once()
                    self.assertTrue(report['max_level']['level_15_verified'])
                else:
                    with self.assertRaises(setup.ObservationError):
                        setup.run(args, report)
                    learn.assert_not_called()
                    session.command.assert_not_called()
                    session.close_gm.assert_not_called()
                    self.assertFalse(report['max_level']['level_15_verified'])
            session.verify_level.assert_called_once_with('after-max-level', 15)
            session.tap.assert_called_once()

    def test_fresh_owner_and_same_epoch_are_required(self):
        good = controls()
        self.assertEqual(setup.validate_controls(good, 97, now_ns=1_002_000_000), 'owned:test:97')
        for change, reason in [({'hero': 25}, 'wrong_hero'), ({'ready': False}, 'unknown_controls'),
                               ({'sample_end_ns': 700_000_000, 'sample_start_ns': 699_000_000}, 'stale_controls'),
                               ({'sample_end_ns': float('nan')}, 'unknown_clock')]:
            sample = dict(good, **change)
            with self.assertRaises(setup.ObservationError) as error:
                setup.validate_controls(sample, 97, now_ns=1_002_000_000)
            self.assertEqual(error.exception.reason, reason)
        with self.assertRaises(setup.ObservationError) as error:
            setup.validate_controls(good, 97, epoch='other', now_ns=1_002_000_000)
        self.assertEqual(error.exception.reason, 'changed_epoch')

    def test_upgrade_joins_real_availability_with_existing_keyboard_binding(self):
        self.assertEqual(setup.upgrade_plan(controls())[0]['key'], 49)
        self.assertEqual(setup.upgrade_plan(controls(available=False)), [])
        bad = controls()
        bad['buttons'][0]['available'] = False
        with self.assertRaises(setup.ObservationError) as error:
            setup.upgrade_plan(bad)
        self.assertEqual(error.exception.reason, 'unmapped_upgrade')
        bad = controls()
        bad['native_buttons'][0]['press_valid'] = False
        with self.assertRaises(setup.ObservationError):
            setup.upgrade_plan(bad)
        bad = controls()
        bad['buttons'][0]['key'] = 0x02000002
        with self.assertRaises(setup.ObservationError) as error:
            setup.upgrade_plan(bad)
        self.assertEqual(error.exception.reason, 'unsupported_upgrade_key')

    def test_gm_closed_or_unknown_is_not_all_flags_off(self):
        self.assertEqual(setup.gm_flags(gm()), {'no_cooldown': True, 'god': True, 'ai': False, 'lanes_closed': True})
        for sample in (gm(False), {'objects': []}):
            with self.assertRaises(setup.ObservationError):
                setup.gm_flags(sample)
        bad = gm()
        bad['objects'][2]['widget']['visible'] = None
        with self.assertRaises(setup.ObservationError):
            setup.gm_flags(bad)

    def test_lost_down_reply_still_releases_that_key_and_ends_session(self):
        FakeSession.instances = []
        FakeSession.fail_down = True
        FakeSession.upgrades = True
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / 'report.json'
            with mock.patch.object(setup, 'Setup', FakeSession), contextlib.redirect_stdout(io.StringIO()):
                code = setup.main(['--hero', '97', '--learn-only', '--output', str(output)])
            data = json.loads(output.read_text())
            self.assertEqual(code, 1)
            self.assertIn('lost DOWN', data['error'])
            calls = FakeSession.instances[0].calls
            self.assertIn(('key 49 0', True), calls)
            self.assertIn(('end', True), calls)
            self.assertFalse(any(command.startswith('write ') for command, _ in calls))

    def test_already_learned_controls_are_observed_without_spamming_keys(self):
        FakeSession.instances = []
        FakeSession.fail_down = False
        FakeSession.upgrades = False
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / 'report.json'
            with mock.patch.object(setup, 'Setup', FakeSession), contextlib.redirect_stdout(io.StringIO()):
                code = setup.main(['--hero', '97', '--learn-only', '--output', str(output)])
            data = json.loads(output.read_text())
            self.assertEqual(code, 0)
            self.assertTrue(data['learning']['available_upgrades_exhausted'])
            self.assertFalse(any(command.startswith('key ') for command, _ in FakeSession.instances[0].calls))
            self.assertEqual(data['status'], 'observation_only')
            self.assertNotIn('max_level', data)

    def test_existing_report_prevents_execution(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'report.json'
            path.write_text('original')
            with mock.patch.object(setup, 'run') as execute, contextlib.redirect_stderr(io.StringIO()):
                with self.assertRaises(SystemExit):
                    setup.main(['--hero', '97', '--learn-only', '--output', str(path)])
            execute.assert_not_called()
            self.assertEqual(path.read_text(), 'original')

    def test_gm_actions_require_explicit_coordinator_close_geometry(self):
        with mock.patch.object(setup, 'run') as execute, contextlib.redirect_stderr(io.StringIO()):
            with self.assertRaises(SystemExit):
                setup.main(['--hero', '97', '--max-level', '--output', '/tmp/uncreated-setup-test.json'])
        execute.assert_not_called()


if __name__ == '__main__':
    unittest.main()
