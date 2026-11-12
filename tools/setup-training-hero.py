#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Developer coordinator tool: prepare the current training hero without desktop input.

Only the root test coordinator should run this while it owns the runtime. It
never selects a hero or changes persisted CLBB settings. Without --max-level,
only currently available upgrades are learned; no hero level is inferred.
--configure-flags requests no-CD/God ON, AI OFF and lanes closed, using current
native GM overlay visibility. Unknown state is an error, never a toggle guess.
"""
from __future__ import annotations
import argparse
import importlib.util
import json
import math
import os
from pathlib import Path
import socket
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]

def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

probe_module = load('clbb_setup_probe', ROOT / 'tools/probe-hero-control.py')
Probe = probe_module.Probe

GM = {
    'no_cooldown': ('m_Sprite_Switch5_MyHero', 'm_Sprite_F5add_MyHero', True),
    'god': ('m_Sprite_Switch6_MyHero', 'm_Sprite_F6add_MyHero', True),
    'ai': ('m_Sprite_Switch5_EnemyHero', 'm_Sprite_F5add_EnemyHero', False),
    'lanes_closed': ('m_Sprite_Switch1_Common', 'm_Sprite_F1add_Common', True),
}
MAX_LEVEL = 'UITrainGM.uiFrame.m_Sprite_F2add_MyHero'
WRENCH = 'UITrainGM.uiFrame.m_Sprite_GmIcon'


class ObservationError(RuntimeError):
    def __init__(self, reason, detail):
        self.reason = reason
        super().__init__(f'{reason}: {detail}')


def validate_controls(data, hero, epoch=None, now_ns=None):
    if not isinstance(data, dict) or data.get('ready') is not True:
        raise ObservationError('unknown_controls', 'Fresh diagnostics.controls is required (current mapper build)')
    if type(data.get('hero')) is not int or data['hero'] != hero:
        raise ObservationError('wrong_hero', f'Expected hero {hero}, observed {data.get("hero")}')
    token = data.get('epoch')
    if not isinstance(token, str) or not token:
        raise ObservationError('unknown_epoch', 'No current match ownership token')
    if epoch is not None and token != epoch:
        raise ObservationError('changed_epoch', 'Local hero/match ownership changed')
    start, end = data.get('sample_start_ns'), data.get('sample_end_ns')
    if any(isinstance(v, bool) or not isinstance(v, (int, float)) or not math.isfinite(v) for v in (start, end)):
        raise ObservationError('unknown_clock', 'Missing native sample timestamps')
    now = time.monotonic_ns() if now_ns is None else now_ns
    if start < 0 or end < start or end - start > 100_000_000 or end > now + 10_000_000:
        raise ObservationError('invalid_clock', 'Invalid native sample bracket')
    if now - end > 100_000_000:
        raise ObservationError('stale_controls', 'Native controls are over100ms old')
    for field in ('buttons', 'native_buttons', 'capability_controls'):
        if not isinstance(data.get(field), list) or len(data[field]) > 64:
            raise ObservationError('unknown_controls', f'Invalid {field} array')
    return token


def upgrade_plan(data):
    mapped = {}
    for row in data['buttons']:
        if row.get('kind') != 'upgrade':
            continue
        slot = row.get('slot')
        if slot in mapped:
            raise ObservationError('ambiguous_upgrade', f'Duplicate mapped slot {slot}')
        mapped[slot] = row
    native = {}
    for row in data['native_buttons']:
        if row.get('kind') != 'upgrade':
            continue
        slot = row.get('slot')
        if slot in native:
            raise ObservationError('ambiguous_upgrade', f'Duplicate native slot {slot}')
        native[slot] = row
    capabilities = {row.get('slot') for row in data['capability_controls'] if row.get('kind') == 'upgrade'}
    if not mapped and not native and not capabilities:
        raise ObservationError('unknown_upgrades', 'Upgrade capability is not declared')
    result = []
    for slot, source in native.items():
        if type(source.get('available')) is not bool:
            raise ObservationError('unknown_upgrade_availability', f'{slot} has no boolean availability')
        if source['available'] is not True:
            continue
        index = source.get('index')
        if type(index) is not int or not 0 <= index <= 3 or slot != f'upgrade:{index}':
            raise ObservationError('unknown_upgrade', 'Native slot/index contract changed')
        point = source.get('press')
        if source.get('press_valid') is not True or not isinstance(point, list) or len(point) != 2 or any(
                isinstance(v, bool) or not isinstance(v, (int, float)) or not math.isfinite(v) or not 0 <= v <= 1 for v in point):
            raise ObservationError('unknown_upgrade_geometry', f'Visible {slot} has no valid press position')
        button = mapped.get(slot)
        if button is None or button.get('available') is not True:
            raise ObservationError('unmapped_upgrade', f'Native {slot} is available but its mapped button is unavailable')
        code = button.get('key')
        if type(code) is not int or not 0 < code <= 0x01FFFFFF:
            raise ObservationError('unsupported_upgrade_key', f'{slot} needs a valid keyboard binding; got {code}')
        result.append({'slot': slot, 'index': index, 'key': code, 'native_press': point})
    return sorted(result, key=lambda row: row['index'])


def widget(data, suffix, require_visible=False):
    rows = [row for row in data.get('objects', []) if row.get('path', '').endswith(suffix)]
    if len(rows) != 1 or not isinstance(rows[0].get('widget'), dict):
        raise ObservationError('unknown_gm_widget', f'Expected one native widget {suffix}')
    value = rows[0]['widget']
    if type(value.get('visible')) is not bool or type(value.get('onscreen')) is not bool:
        raise ObservationError('unknown_gm_visibility', suffix)
    if require_visible and (not value['visible'] or not value['onscreen']):
        raise ObservationError('hidden_gm_target', suffix)
    point = value.get('center_uv')
    if not isinstance(point, list) or len(point) != 2 or any(not isinstance(v, (float, int)) or isinstance(v, bool)
            or not math.isfinite(v) or not 0 <= v <= 1 for v in point):
        raise ObservationError('unknown_gm_geometry', suffix)
    return value


def gm_flags(data):
    widget(data, MAX_LEVEL, require_visible=True)  # Menu closed is not equivalent to all flags OFF.
    result = {}
    for name, (overlay, _, _) in GM.items():
        value = widget(data, 'UITrainGM.uiFrame.' + overlay)
        if value['onscreen'] is not True:
            raise ObservationError('unknown_gm_visibility', overlay)
        result[name] = value['visible']
    return result


class Setup(Probe):
    def __init__(self, hero, report, directory):
        super().__init__(hero, report)
        if not self.socket_path.exists():
            owner = self.proc.stat().st_uid
            self.socket_path = Path(f'/run/user/{owner}/clbb/mapper-{self.pid}.sock')
        self.directory = directory
        self.directory.mkdir(parents=True, exist_ok=False)
        self.serial = 0
        self.epoch = None

    def expected_hero(self):
        value = self.read('hero.id')
        if value != self.hero:
            raise ObservationError('wrong_hero', f'Expected hero {self.hero}, observed {value}')

    def controls(self, stage):
        data = self.read('diagnostics.controls')
        self.epoch = validate_controls(data, self.hero, self.epoch)
        self.report['samples'].append({'stage': stage, 'ns': time.monotonic_ns(), 'controls': data})
        return data

    def gm_audit(self, stage):
        self.identity()
        self.expected_hero()
        self.serial += 1
        path = self.directory / f'{self.serial:03d}-{stage}.gm.json'
        binary = ROOT / 'mapper/runtime/target/debug/training-ui-audit'
        subprocess.run([str(binary), str(self.pid), str(path), 'Train'], cwd=ROOT,
                       check=True, capture_output=True, timeout=20)
        data = json.loads(path.read_text())
        self.identity()
        self.expected_hero()
        if data.get('pid') != self.pid or str(data.get('process_birth')) != str(self.birth):
            raise ObservationError('wrong_gm_owner', 'GM inventory belongs to another runtime')
        if data.get('truncated') is not False or data.get('topology_unchanged') is not True:
            raise ObservationError('unstable_gm_topology', 'GM frame changed during native inventory')
        return data

    def raw_idle(self):
        deadline = time.monotonic() + 2
        while time.monotonic() < deadline:
            data = self.read('diagnostics.delivery')
            if not isinstance(data, dict) or type(data.get('raw_gesture_active')) is not bool:
                raise ObservationError('unknown_raw_delivery', 'Native raw gesture state is unavailable')
            if data['raw_gesture_active'] is False:
                return
            time.sleep(.025)
        raise ObservationError('raw_input_busy', 'Previous raw UI gesture did not finish')

    def tap(self, point):
        self.expected_hero()
        self.raw_idle()
        self.command(f'tap {point[0]:.12f} {point[1]:.12f}')
        self.raw_idle()
        time.sleep(.20)

    def open_gm(self):
        data = self.gm_audit('gm-before')
        if widget(data, MAX_LEVEL)['visible']:
            return data
        self.tap(widget(data, WRENCH, require_visible=True)['center_uv'])
        data = self.gm_audit('gm-open')
        widget(data, MAX_LEVEL, require_visible=True)
        return data

    def close_gm(self, point):
        if point is None:
            raise ObservationError('unknown_gm_close', 'Supply the coordinator-observed --gm-close-point; no guessed native anchor is used')
        self.tap(point)
        data = self.gm_audit('gm-closed')
        if widget(data, MAX_LEVEL)['visible']:
            raise ObservationError('gm_did_not_close', 'GM remains visible after the supplied close point')


def learn(session, rounds, hold_ms):
    reserved = {probe_module.key_code(session.command('read ' + key)) for key in
                ('hotkeys.settings', 'hotkeys.confine', 'hotkeys.fullscreen')}
    empty = 0
    for round_index in range(rounds):
        plan = upgrade_plan(session.controls(f'learn-round:{round_index}'))
        if not plan:
            empty += 1
            if empty >= 2:
                return {'available_upgrades_exhausted': True, 'rounds': round_index + 1}
            session.wait_until(time.monotonic() + .12)
            continue
        empty = 0
        for expected in plan:
            # Each previous level-up can change the availability of all slots.
            current = upgrade_plan(session.controls(f'before:{expected["slot"]}'))
            row = next((item for item in current if item['slot'] == expected['slot']), None)
            if row is None:
                continue
            if row['key'] in reserved:
                raise ObservationError('reserved_upgrade_key', 'Upgrade binding overlaps a host shortcut')
            code = row['key']
            session.report['pressed_keys'].append(code)  # Cleanup includes lost DOWN replies.
            session.command(f'key {code} 1')
            session.wait_until(time.monotonic() + hold_ms / 1000.)
            session.command(f'key {code} 0')
            session.wait_until(time.monotonic() + .10)
    final = upgrade_plan(session.controls('learn-boundary'))
    if final:
        raise ObservationError('upgrade_round_limit', 'Available upgrade buttons remain after bounded rounds')
    return {'available_upgrades_exhausted': True, 'rounds': rounds}


def run(args, report):
    session = Setup(args.hero, report, args.evidence)
    session.expected_hero()
    session.controls('preflight')  # Older runtimes fail here before any game input.
    report['phases'].append({'name': 'preflight', 'ns': time.monotonic_ns()})
    if session.read('diagnostics.contacts').get('active') != []:
        raise ObservationError('held_contacts', 'Do not interrupt another control gesture')
    began = False
    try:
        if args.max_level or args.configure_flags or args.verify_flags:
            report['phases'].append({'name': 'gm_open', 'ns': time.monotonic_ns()})
            gm = session.open_gm()
            if args.configure_flags:
                for name, (_, target, desired) in GM.items():
                    observed = gm_flags(gm)
                    if observed[name] != desired:
                        session.tap(widget(gm, 'UITrainGM.uiFrame.' + target, require_visible=True)['center_uv'])
                        gm = session.gm_audit('flag-' + name)
                        if gm_flags(gm)[name] != desired:
                            raise ObservationError('gm_flag_not_changed', name)
                report['gm_flags'] = gm_flags(gm)
            if args.verify_flags:
                report['gm_flags'] = gm_flags(gm)
                if any(report['gm_flags'][name] != desired for name, (_, _, desired) in GM.items()):
                    raise ObservationError('gm_fixture_mismatch', 'Observed GM flags differ from the requested test fixture')
            if args.max_level:
                session.tap(widget(gm, MAX_LEVEL, require_visible=True)['center_uv'])
                report['max_level'] = {'requested': True, 'native_max_button_observed': True,
                                       'level_15_verified': False, 'reason': 'No independent native hero-level field is currently exposed'}
            session.close_gm(args.gm_close_point)
            report['phases'].append({'name': 'gm_closed', 'ns': time.monotonic_ns()})
        if args.verify_flags and not args.max_level and not args.configure_flags:
            return
        began = True
        session.command('begin')
        session.in_control = True
        session.send_pointer((.5, .5))
        report['phases'].append({'name': 'learn_start', 'ns': time.monotonic_ns()})
        report['learning'] = learn(session, args.rounds, args.hold_ms)
        report['phases'].append({'name': 'learn_end', 'ns': time.monotonic_ns()})
    finally:
        errors = []
        if began:
            for code in sorted(set(report['pressed_keys'])):
                try:
                    session.command(f'key {code} 0', cleanup=True)
                except Exception as error:
                    errors.append(str(error))
            try:
                session.command('end', cleanup=True)
            except Exception as error:
                errors.append(str(error))
            session.in_control = False
            try:
                report['cleanup_contacts_empty'] = session.read('diagnostics.contacts').get('active') == []
            except Exception as error:
                errors.append(str(error))
        report['cleanup_errors'] = errors


def bounded(low, high):
    return probe_module.bounded_int(low, high)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--hero', type=bounded(1, 9999), required=True)
    parser.add_argument('--max-level', action='store_true', help='Press native GM maximum-level control before learning')
    parser.add_argument('--learn-only', action='store_true', help='Explicitly avoid all GM actions; learn currently available skills only')
    parser.add_argument('--configure-flags', action='store_true', help='Request noCD/God ON, AI OFF and lanes closed from native toggle state')
    parser.add_argument('--verify-flags', action='store_true', help='Verify that GM fixture; without other actions, do not learn skills')
    parser.add_argument('--gm-close-point', type=float, nargs=2, metavar=('U', 'V'), help='Root-coordinator-observed close point; verified closed after click')
    parser.add_argument('--rounds', type=bounded(1, 12), default=8)
    parser.add_argument('--hold-ms', type=bounded(100, 300), default=100)
    parser.add_argument('--output', type=Path, required=True, help='New result JSON, never overwrites')
    args = parser.parse_args(argv)
    if args.learn_only and (args.max_level or args.configure_flags or args.verify_flags):
        parser.error('--learn-only cannot perform GM actions')
    if args.gm_close_point is not None and any(not math.isfinite(v) or not 0 <= v <= 1 for v in args.gm_close_point):
        parser.error('GM close point must be finite normalized coordinates')
    if (args.max_level or args.configure_flags or args.verify_flags) and args.gm_close_point is None:
        parser.error('GM actions require --gm-close-point until native close geometry is verified')
    args.evidence = args.output.with_suffix('.phases')
    report = {'format': 'clbb-training-hero-setup', 'version': 1, 'start_ns': time.monotonic_ns(),
              'request': {'hero': args.hero, 'max_level': args.max_level, 'configure_flags': args.configure_flags,
                          'verify_flags': args.verify_flags, 'rounds': args.rounds, 'hold_ms': args.hold_ms},
              'commands': [], 'samples': [], 'phases': [], 'pressed_keys': [], 'status': 'observation_only',
              'limits': 'No persisted settings change. Upgrade availability exhaustion is not proof of hero level15 or combat correctness.'}
    try:
        with args.output.open('x', encoding='utf-8') as stream:
            try:
                run(args, report)
            except (Exception, KeyboardInterrupt) as error:
                report['error'] = f'{type(error).__name__}: {error}'
                if isinstance(error, ObservationError):
                    report['reason'] = error.reason
            finally:
                report['end_ns'] = time.monotonic_ns()
                json.dump(report, stream, ensure_ascii=False, indent=2)
                stream.write('\n')
    except OSError as error:
        parser.error(f'Cannot reserve new report: {error}')
    code = 1 if report.get('error') or report.get('cleanup_errors') else 0
    print(json.dumps({'output': str(args.output), 'status': report['status'], 'error': report.get('error'), 'exit_code': code}))
    return code


if __name__ == '__main__':
    raise SystemExit(main())
