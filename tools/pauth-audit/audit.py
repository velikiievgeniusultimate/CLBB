#!/usr/bin/env python3
"""Extract the real production QARMA functions and compare with pinned QEMU.

Only --candidate prepares an in-memory candidate for pre-application testing.
Default mode requires the generated kernels to match production exactly.
Never builds/starts/installs the game or reads game/account memory.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import statistics
import subprocess

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
QEMU = ROOT / 'src/nb-qemu/qemu'
SOURCE = QEMU / 'target/arm/pauth_helper.c'
PIN = 'a590cb154d1703b80265a75df4fca4d51b0dc0ba'
ORACLE_SHA = '8e205c37b2e4ad6a8081345ea7695831aa5f25b8eee632d27d7b9f4768f55f7d'
KERNELS = ('pac_sub', 'pac_inv_sub', 'pac_mult')


def run(command, **kw):
    return subprocess.check_output(command, text=True, **kw)


def digest(text):
    return hashlib.sha256(text.encode()).hexdigest()


def function(source, name):
    match = re.search(r'^static[^\n]*\b' + name + r'\(', source, re.M)
    if not match:
        raise ValueError('Missing real function: ' + name)
    start = match.start()
    brace = source.index('{', match.end())
    depth = 1
    end = brace + 1
    while depth:
        depth += (source[end] == '{') - (source[end] == '}')
        end += 1
    return start, end, source[start:end]


def replace(source, name, body):
    start, end, _ = function(source, name)
    return source[:start] + body + source[end:]


def without_kernels(source):
    for name in (*KERNELS, 'rot_cell'):
        try:
            source = replace(source, name, '')
        except ValueError:
            assert name == 'rot_cell'
    return '\n'.join(line for line in source.splitlines() if line.strip())


def extract(source, prefix):
    start = source.index('static uint64_t pac_cell_shuffle')
    end = source.index('static uint64_t pauth_addpac')
    part = source[start:end]
    names = set(re.findall(r'\b(?:pac_[a-z0-9_]+|vector_[a-z0-9_]+|tweak_[a-z0-9_]+|rot_cell|pauth_computepac(?:_[a-z0-9_]+)?)\b', part))
    for name in names:
        part = re.sub(r'\b' + name + r'\b', prefix + '_' + name, part)
    part = part.replace('static uint64_t ' + prefix + '_pauth_computepac',
                        '__attribute__((noinline)) static uint64_t ' + prefix + '_pauth_computepac')
    return part


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate', action='store_true')
    parser.add_argument('--random', type=int, default=2_000_000)
    parser.add_argument('--iterations', type=int, default=1_000_000)
    parser.add_argument('--repeats', type=int, default=7)
    args = parser.parse_args()
    assert args.random >= 0 and args.iterations > 0 and args.repeats > 0
    artifacts = HERE / 'artifacts'
    artifacts.mkdir(exist_ok=True)
    oracle = run(['git', '-C', str(QEMU), 'show', PIN + ':target/arm/pauth_helper.c'])
    assert digest(oracle) == ORACLE_SHA, 'Pinned upstream oracle changed'
    (artifacts / 'original-pauth_helper.c').write_text(oracle)
    generator = artifacts / 'generate'
    subprocess.run(['rustc', '--edition=2024', '-O', str(HERE / 'generate.rs'), '-o', str(generator)], check=True)
    generated = run([str(generator)])
    (artifacts / 'generated-kernels.c').write_text(generated)
    production = SOURCE.read_text()
    start = oracle.index('static uint64_t pac_sub(')
    end = oracle.index('static uint64_t tweak_cell_rot(')
    candidate = oracle[:start] + generated + '\n' + oracle[end:]
    core = function(oracle, 'pauth_computepac')[2]
    core = core.replace('static uint64_t pauth_computepac(',
                        'static inline __attribute__((always_inline)) uint64_t pauth_computepac_core(')
    core = core.replace('ARMPACKey key)\n{',
                        'ARMPACKey key,\n'
                        '                                 uint64_t (*pac_sub)(uint64_t),\n'
                        '                                 uint64_t (*pac_inv_sub)(uint64_t))\n{')
    old_core = function(oracle, 'pauth_computepac')[2]
    assert core[core.index('{'):] == old_core[old_core.index('{'):]
    candidate = replace(candidate, 'pauth_computepac', core + '\n\n' + (HERE / 'dispatch.c').read_text().rstrip())
    if args.candidate:
        previous = json.loads((artifacts / 'report.json').read_text())
        assert previous['pass'] and digest(production) == previous['production_sha256'], 'Production changed since last successful audit'
        production = candidate
    else:
        assert production == candidate, 'Real production is not the generated candidate'
    # Deterministic candidate construction above changes only the three kernels
    # and wraps the byte-identical original full-QARMA body. All add/auth/key
    # handling below this boundary remains byte-for-byte unchanged.
    assert production[production.index('static uint64_t pauth_addpac'):] == oracle[oracle.index('static uint64_t pauth_addpac'):]
    (artifacts / 'candidate-pauth_helper.c').write_text(candidate)
    prelude = r'''
#define _POSIX_C_SOURCE 200809L
#include <stdint.h>
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>
#define MAKE_64BIT_MASK(s,n) ((UINT64_MAX >> (64 - (n))) << (s))
static inline uint64_t extract64(uint64_t x, unsigned s, unsigned n)
{ return (x >> s) & (UINT64_MAX >> (64 - n)); }
static inline uint32_t extract32(uint32_t x, unsigned s, unsigned n)
{ return (x >> s) & (UINT32_MAX >> (32 - n)); }
typedef struct { uint64_t lo, hi; } ARMPACKey;
'''
    harness = prelude + extract(oracle, 'oracle') + extract(production, 'production')
    harness += (HERE / 'harness.c').read_text()
    translation_unit = artifacts / 'extracted-production-and-oracle.c'
    translation_unit.write_text(harness)
    affinity = sorted(os.sched_getaffinity(0)) if hasattr(os, 'sched_getaffinity') else []
    if affinity:
        os.sched_setaffinity(0, {affinity[-1]})
    report = {'pinned_qemu_commit': PIN, 'oracle_sha256': ORACLE_SHA,
              'production_sha256': digest(production), 'generator_sha256': digest((HERE / 'generate.rs').read_text()),
              'vector_generator_sha256': digest((HERE / 'vector-state.rs').read_text()),
              'dispatch_sha256': digest((HERE / 'dispatch.c').read_text()),
              'harness_sha256': digest((HERE / 'harness.c').read_text()),
              'candidate_mode': args.candidate, 'compiler': run(['cc', '--version']).splitlines()[0],
              'rustc': run(['rustc', '--version']).strip(), 'cpu_affinity': affinity[-1:], 'checks': {}}
    for label, flags, count, iterations, repeats in [
        ('O2', ['-O2'], args.random, args.iterations, args.repeats),
        ('O3', ['-O3'], args.random, args.iterations, args.repeats),
        ('ubsan', ['-O2', '-fsanitize=undefined', '-fno-sanitize-recover=all'],
         max(200_000, args.random // 10), 1000, 1),
        ('portable-only', ['-O2'], args.random, args.iterations, args.repeats),
    ]:
        binary = artifacts / ('audit-' + label)
        unit = translation_unit
        if label == 'portable-only':
            unit = artifacts / 'extracted-portable-only.c'
            unit.write_text(harness.replace('#if defined(__x86_64__) && defined(__GNUC__)', '#if 0'))
        command = ['cc', '-std=c11', '-Wall', '-Wextra', '-DCLBB_PAC_C_REFERENCE=1', *flags, str(unit), '-o', str(binary)]
        subprocess.run(command, check=True)
        raw = run([str(binary), str(count), str(iterations), str(repeats)])
        (artifacts / (label + '.jsonl')).write_text(raw)
        records = [json.loads(line) for line in raw.splitlines()]
        summary = {'compile_command': command, 'validation': records[0], 'benchmarks': {}}
        for lanes in [1, 4]:
            medians = {}
            for kind in sorted(set(x['benchmark'] for x in records[1:])):
                samples = [x['cpu_ns_per_pac'] for x in records[1:] if x['lanes'] == lanes and x['benchmark'] == kind]
                medians[kind] = statistics.median(samples)
            summary['benchmarks'][str(lanes)] = {**medians, 'speedup': medians['oracle'] / medians['production']}
        report['checks'][label] = summary
        print(label, json.dumps(summary['benchmarks']), flush=True)
    for label in ['O2', 'O3', 'portable-only']:
        assert report['checks'][label]['benchmarks']['1']['speedup'] > 1.2, 'No convincing speedup'
    portable_assembly = run(['objdump', '-d', str(artifacts / 'audit-portable-only')])
    assert not re.search(r'\b(?:v?pshufb|palignr|phaddw|phaddd|phsubw|phsubd)\b', portable_assembly)
    (artifacts / 'portable-only-disassembly.txt').write_text(portable_assembly)
    report['portable_only_ssse3_instruction_count'] = 0
    host_assembly = run(['objdump', '-d', str(artifacts / 'audit-O2')])
    report['ssse3_instruction_count'] = len(re.findall(r'\bpshufb\b', host_assembly))
    if 'forced-ssse3' in report['checks']['O2']['benchmarks']['1']:
        assert report['ssse3_instruction_count'] > 0
    report['pass'] = True
    (artifacts / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print('PAUTH_PRODUCTION_CONTRACT_PASS', flush=True)


if __name__ == '__main__':
    main()
