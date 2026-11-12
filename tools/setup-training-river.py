#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Root test coordinator's bounded training river fixture; no desktop input.

Requests four combat dummies through their observed native GM button, then
returns the player to the center. Every teleport requires a fresh world-position
receipt before any spawn. Existing output and per-process fixture markers stop
accidental reruns. This cannot prove that the match had no manually spawned
bots, nor independently prove their immortality or native entity count.
"""
from __future__ import annotations
import argparse
import importlib.util
import json
import math
from pathlib import Path
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('clbb_river_setup', ROOT / 'tools/setup-training-hero.py')
setup = importlib.util.module_from_spec(spec)
spec.loader.exec_module(setup)
ObservationError = setup.ObservationError
Setup = setup.Setup

TELEPORT_WIDGET = 'UITrainGM.uiFrame.m_Sprite_F9_Addsjyd'
COMBAT_BOT_WIDGET = 'UITrainGM.uiFrame.m_Sprite_F3add_SummonHero'
# Root-observed map points with independent old world receipts from
# logs/hero-bindings/river-four-dummies-setup.json, not hero cursor coordinates.
POINTS = (
    ((.2325, .2690), (3.0126953125, -3.0322265625)),
    ((.1875, .2690), (-2.58203125, 2.5625)),
    ((.2100, .2402), (3.0, 2.5498046875)),
    ((.2100, .2978), (-2.5693359375, -3.01953125)),
)
CENTER = ((.2100, .2690), (.21533203125, -.23480224609375))


def world_position(aim, now=None):
    if not isinstance(aim, dict):
        raise ObservationError('unknown_world_position', 'Aim diagnostics is not an object')
    point = aim.get('hero_world')
    if not isinstance(point, list) or len(point) != 2 or any(
            isinstance(v, bool) or not isinstance(v, (int, float)) or not math.isfinite(v) or abs(v) > 100000 for v in point):
        raise ObservationError('unknown_world_position', 'Missing finite local hero world coordinates')
    end = aim.get('frame_end_ns')
    if isinstance(end, bool) or not isinstance(end, (int, float)) or not math.isfinite(end):
        raise ObservationError('unknown_world_clock', 'Missing world sample timestamp')
    now = time.monotonic_ns() if now is None else now
    if now - end > 100_000_000 or end > now + 10_000_000:
        raise ObservationError('stale_world_position', 'World-position sample is not fresh')
    return tuple(point)


def reserve_marker(session, report):
    # Actor epoch changes on hero switches while the same four bots remain.
    # Key by runtime process birth, not by hero or actor epoch.
    path = ROOT / 'logs/hero-bindings' / f'river-fixture-{session.pid}-{session.birth}.json'
    path.parent.mkdir(parents=True, exist_ok=True)
    try:
        stream = path.open('x+', encoding='utf-8')
    except FileExistsError as error:
        raise ObservationError('fixture_already_attempted',
                               f'{path} exists; inspect it and clear the fixture manually before an intentional rerun') from error
    report['fixture_marker'] = str(path)
    return stream


def write_marker(stream, report, status):
    value = {'pid': report.get('identity', {}).get('pid'), 'status': status,
             'output': report.get('output'), 'spawn_requests': report['spawn_requests'],
             'note': 'Duplicate-run guard, not native bot-count evidence', 'updated_ns': time.monotonic_ns()}
    stream.seek(0)
    json.dump(value, stream, ensure_ascii=False, indent=2)
    stream.write('\n')
    stream.truncate()
    stream.flush()


def wait_world(session, expected, tolerance, stage, timeout=4.):
    deadline = time.monotonic() + timeout
    stable = 0
    previous = None
    while time.monotonic() < deadline:
        session.controls(stage)  # Validates owner/hero/epoch and a fresh sample.
        aim = session.read('diagnostics.aim')
        current = world_position(aim)
        distance = math.dist(current, expected)
        receipt = {'stage': stage, 'ns': time.monotonic_ns(), 'world': current,
                   'expected_world': expected, 'distance': distance, 'frame_end_ns': aim['frame_end_ns']}
        session.report['world_receipts'].append(receipt)
        settled = previous is not None and math.dist(previous, current) < .15
        stable = stable + 1 if distance <= tolerance and settled else 0
        if stable >= 2:
            return receipt
        previous = current
        time.sleep(.08)
    raise ObservationError('teleport_not_confirmed', f'Player did not settle at expected river point {expected}; no bot spawned')


def teleport(session, point, expected, tolerance, stage):
    menu = session.open_gm()
    target = setup.widget(menu, TELEPORT_WIDGET, require_visible=True)['center_uv']
    session.tap(target)
    # The teleport command normally closes GM and arms the map. World receipt
    # below is the independent check that the subsequent observed map click
    # actually teleported the player, rather than merely closing a menu.
    after = session.gm_audit(stage + '-map-armed')
    if setup.widget(after, setup.MAX_LEVEL)['visible']:
        raise ObservationError('teleport_map_not_armed', 'GM did not close after the native teleport button')
    session.tap(point)
    session.report['map_clicks'].append({'stage': stage, 'map_uv': point, 'ns': time.monotonic_ns()})
    return wait_world(session, expected, tolerance, stage)


def run(args, report):
    session = Setup(args.hero, report, args.output.with_suffix('.phases'))
    session.expected_hero()
    session.controls('preflight')
    if session.read('diagnostics.contacts').get('active') != []:
        raise ObservationError('held_contacts', 'Release existing mapped contacts before creating a fixture')
    session.raw_idle()
    initial = world_position(session.read('diagnostics.aim'))
    report['initial_world'] = initial
    marker = reserve_marker(session, report)
    attempted_input = False
    try:
        write_marker(marker, report, 'in_progress')
        for index, (point, expected) in enumerate(POINTS):
            attempted_input = True
            report['phases'].append({'name': f'teleport:{index}', 'start_ns': time.monotonic_ns()})
            receipt = teleport(session, point, expected, args.world_tolerance, f'teleport:{index}')
            menu = session.open_gm()
            spawn = setup.widget(menu, COMBAT_BOT_WIDGET, require_visible=True)['center_uv']
            # Record the attempt before transport: a lost reply must not trigger
            # a blind second spawn or clear the duplicate guard.
            request = {'index': index, 'map_uv': point, 'world_receipt': receipt,
                       'native_field': COMBAT_BOT_WIDGET, 'native_press': spawn,
                       'requested_ns': time.monotonic_ns(), 'transport_reply_observed': False}
            report['spawn_requests'].append(request)
            write_marker(marker, report, 'in_progress')
            session.tap(spawn)
            request['transport_reply_observed'] = True
            write_marker(marker, report, 'in_progress')
            session.close_gm(args.gm_close_point)
        report['phases'].append({'name': 'return-center', 'start_ns': time.monotonic_ns()})
        report['final_world_receipt'] = teleport(session, CENTER[0], CENTER[1], args.world_tolerance, 'return-center')
        report['mechanical_checks'] = {'spawn_requests': len(report['spawn_requests']),
                                      'all_spawn_transport_replies_observed': all(r['transport_reply_observed'] for r in report['spawn_requests']),
                                      'player_center_position_verified': True,
                                      'native_bot_count_verified': False,
                                      'native_immortality_verified': False}
        write_marker(marker, report, 'completed_requests_and_center_receipt')
    except BaseException:
        write_marker(marker, report, 'incomplete_inspect_before_retry')
        raise
    finally:
        errors = []
        if attempted_input:
            try:
                session.raw_idle()
            except Exception as error:
                errors.append(str(error))
            try:
                session.command('end', cleanup=True)
            except Exception as error:
                errors.append(str(error))
            try:
                report['cleanup_contacts_empty'] = session.read('diagnostics.contacts').get('active') == []
            except Exception as error:
                errors.append(str(error))
        report['cleanup_errors'] = errors
        marker.close()


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--hero', required=True, type=setup.bounded(1, 9999))
    parser.add_argument('--gm-close-point', type=float, nargs=2, required=True, metavar=('U', 'V'),
                        help='Coordinator-observed GM close point, verified closed after every spawn')
    parser.add_argument('--world-tolerance', type=float, default=.9)
    parser.add_argument('--output', type=Path, required=True, help='New JSON report; existing files are refused')
    args = parser.parse_args(argv)
    if any(not math.isfinite(v) or not 0 <= v <= 1 for v in args.gm_close_point):
        parser.error('GM close point must be finite normalized coordinates')
    if not math.isfinite(args.world_tolerance) or not .2 <= args.world_tolerance <= 2.:
        parser.error('World tolerance must be .2..2 world units')
    report = {'format': 'clbb-training-river-setup', 'version': 1, 'start_ns': time.monotonic_ns(),
              'output': str(args.output), 'hero': args.hero, 'commands': [], 'samples': [], 'pressed_keys': [],
              'phases': [], 'map_clicks': [], 'world_receipts': [], 'spawn_requests': [], 'status': 'observation_only',
              'limits': 'Four native combat-dummy button requests and player teleport receipts; bot entity count/immortality and absence of manual old bots are not independently established.'}
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
        parser.error(f'Cannot reserve a new report: {error}')
    code = 1 if report.get('error') or report.get('cleanup_errors') else 0
    print(json.dumps({'output': str(args.output), 'spawn_requests': len(report['spawn_requests']),
                      'error': report.get('error'), 'exit_code': code}, ensure_ascii=False))
    return code


if __name__ == '__main__':
    raise SystemExit(main())
