#!/usr/bin/env python3
"""Collect local license evidence without touching runtime or game/account files.

This is an inventory, not a license-compatibility or release-clearance verdict.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / 'legal' / 'third-party'
OUT.mkdir(parents=True, exist_ok=True)
records, gaps = [], []


def run(*args, cwd=ROOT):
    return subprocess.run(args, cwd=cwd, text=True, capture_output=True,
                          env={**os.environ, 'LC_ALL': 'C'})


def collect(base, dest, all_files=False):
    copied = []
    if not base.is_dir():
        return copied
    for directory, dirs, files in os.walk(base):
        dirs[:] = sorted(d for d in dirs if d not in {
            '.git', 'target', 'build', 'builddir', 'node_modules', '__pycache__', 'out'})
        for name in sorted(files):
            if not all_files and not re.match(r'^(licen[cs]e|copying|notice|copyright|authors)([._-]|$)', name, re.I):
                continue
            source = Path(directory) / name
            if source.is_symlink() or not source.is_file():
                continue
            if source.stat().st_size > 8 * 1024 * 1024:
                gaps.append(f'License evidence exceeds copy bound: {source.relative_to(base)}')
                continue
            relative = source.relative_to(base)
            target = OUT / dest / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, target)
            copied.append({'path': str(target.relative_to(ROOT)),
                           'sha256': hashlib.sha256(target.read_bytes()).hexdigest()})
    return copied


locations = {'qemu': 'src/nb-qemu/qemu',
             'nb-qemu-bionic': 'src/nb-qemu/libnb-qemu-guest/bionic'}
for name, pin in sorted(json.loads((ROOT / 'sources.json').read_text()).items()):
    base = ROOT / locations.get(name, 'src/' + name)
    head = run('git', 'rev-parse', 'HEAD', cwd=base)
    evidence = collect(base, 'source/' + name)
    records.append({'name': name, 'scope': 'pinned upstream source checkout; per-file terms retained',
                    'url': pin['remote'], 'pinned_revision': pin['commit'],
                    'checked_out_revision': head.stdout.strip() if head.returncode == 0 else None,
                    'license_expression': None, 'license_files': evidence})
    if not evidence:
        for source in sorted(base.rglob('*')):
            if source.suffix not in {'.c', '.h', '.cpp', '.hpp'} or source.is_symlink():
                continue
            header = source.read_text(errors='replace')[:8192]
            match = re.match(r'\s*(/\*.*?\*/)', header, re.S)
            if not match or 'copyright' not in match[1].lower() or not re.search(r'licen[cs]e|redistribution|permission', match[1], re.I):
                continue
            target = OUT / 'source' / name / 'headers' / (str(source.relative_to(base)) + '.txt')
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(match[1] + '\n')
            evidence.append({'path': str(target.relative_to(ROOT)), 'sha256': hashlib.sha256(target.read_bytes()).hexdigest()})
        gaps.append(f'{name}: source-header notices collected; review per-file terms and required full license texts')

    if head.returncode or head.stdout.strip() != pin['commit']:
        gaps.append(f'{name}: checkout differs from pinned source or is unavailable')

# Exact resolved Rust dependencies, not just the old provenance list.
rust = {}
for manifest in ['mapper/core', 'mapper/runtime', 'launcher', 'dns-adapter', 'webkit-launch-adapter', 'distribution/manager']:
    result = run('cargo', 'metadata', '--offline', '--locked', '--format-version', '1',
                 '--manifest-path', str(ROOT / manifest / 'Cargo.toml'))
    if result.returncode:
        gaps.append(f'{manifest}: offline locked Cargo metadata failed')
        continue
    for package in json.loads(result.stdout)['packages']:
        if not package['source']:
            continue
        key = (package['name'], package['version'])
        if key in rust:
            continue
        evidence = collect(Path(package['manifest_path']).parent,
                           f'rust/{key[0]}-{key[1]}')
        rust[key] = {'name': key[0], 'version': key[1], 'scope': 'resolved Rust dependency (includes build/proc-macro dependencies)',
                     'url': package.get('repository') or package.get('homepage'),
                     'license_expression': package.get('license'), 'license_files': evidence}
        if not package.get('license') or not evidence:
            gaps.append(f'Rust {key[0]} {key[1]}: license expression or text needs review')
records.extend(rust[k] for k in sorted(rust))

# Actual host libraries observed in the current owned runtime. Absence from
# this live observation does not prove that an optional plugin is unused.
mapped = set()
identity = ROOT / 'logs/current-atl-pid.json'
pid = json.loads(identity.read_text())['pid'] if identity.exists() else 0
if pid and Path(f'/proc/{pid}/exe').resolve() == ROOT / 'prefix/bin/android-translation-layer':
    for line in Path(f'/proc/{pid}/maps').read_text().splitlines():
        fields = line.split(None, 5)
        if len(fields) == 6 and fields[5].startswith('/usr/') and '.so' in fields[5]:
            mapped.add(fields[5].removesuffix(' (deleted)'))
else:
    gaps.append('Owned runtime is not running; live optional-library observation was skipped without starting the game.')
# Read ELF dependencies, never execute the runtime or third-party binaries.
cache = run('ldconfig', '-p').stdout
host = {}
for line in cache.splitlines():
    match = re.match(r'\s*(\S+) .*=> (\S+)', line)
    if match and 'x86-64' in line:
        host.setdefault(match[1], match[2])
queue = [ROOT / 'prefix/bin/android-translation-layer', ROOT / 'prefix/lib/libclbb_mapper.so']
seen = set()
while queue and len(seen) < 4096:
    binary = queue.pop()
    if not binary.is_file() or str(binary) in seen:
        continue
    seen.add(str(binary))
    if str(binary).startswith('/usr/'):
        mapped.add(str(binary))
    dynamic = run('readelf', '-d', str(binary))
    for soname in re.findall(r'\(NEEDED\).*?\[(.*?)\]', dynamic.stdout):
        local = ROOT / 'prefix/lib' / soname
        if local.is_file():
            queue.append(local)
        elif (ROOT / 'prefix/lib/art' / soname).is_file():
            queue.append(ROOT / 'prefix/lib/art' / soname)
        elif (ROOT / 'prefix/lib/java/dex/android_translation_layer/natives' / soname).is_file():
            queue.append(ROOT / 'prefix/lib/java/dex/android_translation_layer/natives' / soname)
        elif soname in host:
            queue.append(Path(host[soname]))
        else:
            gaps.append(f'Unresolved ELF dependency: {soname}')
packages = set()
for path in sorted(mapped):
    result = run('pacman', '-Qoq', path)
    if result.returncode == 0:
        packages.update(result.stdout.splitlines())
    else:
        gaps.append(f'Observed host shared library has no package owner: {path}')
for package in sorted(packages):
    result = run('pacman', '-Qi', package)
    fields = {}
    for line in result.stdout.splitlines():
        match = re.match(r'^([A-Za-z ][A-Za-z ]*?)\s*:\s*(.*)$', line)
        if match:
            fields[match[1].strip()] = match[2].strip()
    expression = fields.get('Licenses', '')
    evidence = collect(Path('/usr/share/licenses') / package, 'system/' + package, True)
    for token in re.findall(r'[A-Za-z0-9][A-Za-z0-9.+-]*', expression):
        if token in {'AND', 'OR', 'None'}:
            continue
        reference = {'GPL': 'GPL-2.0-only', 'GPL2': 'GPL-2.0-only', 'LGPL3': 'LGPL-3.0-only'}.get(token, token)
        if reference != token:
            gaps.append(f'System {package}: legacy declaration {token}; confirm exact grant/version from source headers. Collected standard text is a reference, not a relicense.')
        for candidate in [Path('/usr/share/licenses/spdx') / reference,
                          Path('/usr/share/licenses/spdx') / (reference + '.txt')]:
            if candidate.is_file():
                target = OUT / 'common' / candidate.name
                target.parent.mkdir(exist_ok=True)
                shutil.copyfile(candidate, target)
                evidence.append({'path': str(target.relative_to(ROOT)),
                                 'sha256': hashlib.sha256(target.read_bytes()).hexdigest()})
                break
    records.append({'name': package, 'version': fields.get('Version'),
                    'scope': 'host package owning observed or ELF-linked shared libraries; not necessarily bundled',
                    'url': fields.get('URL'), 'license_expression': expression,
                    'license_files': evidence})
    if not evidence:
        gaps.append(f'System {package}: license text not collected automatically')

# Keep private-prefix notices separately: these are extracted package notices,
# not a claim that every development tool belongs in the end-user installer.
prefix_evidence = collect(ROOT / 'prefix/share/licenses', 'prefix', True)
gaps.extend([
    'Project default is GPL-3.0-or-later; preserve every explicit existing file-level and upstream license (see LICENSE_POLICY.md).',
    'Review license compatibility and source headers, including mixed-license upstream subcomponents.',
    'Create corresponding source archives for each distributed binary, with CLBB patches and build instructions; links alone are not the completed source-delivery plan.',
    'Define final installer payload and audit optional plugins/fonts/codecs and private-prefix package versions; live mapped libraries are not a complete dependency closure.',
    'Game APK/resources are proprietary and excluded. This inventory grants no MOONTON redistribution or service permission.',
])
report = {'schema': 1, 'status': 'draft_inventory_not_release_clearance',
          'components': records, 'prefix_license_files': prefix_evidence,
          'remaining_review': gaps}
(ROOT / 'legal/third-party-inventory.json').write_text(json.dumps(report, indent=2) + '\n')
lines = ['CLBB — УПОМИНАНИЯ СТОРОННИХ КОМПОНЕНТОВ / THIRD-PARTY NOTICES',
         'Черновой реестр: это не завершённая проверка готовности к публикации.',
         'Исходные тексты лицензий и уведомлений сохранены в legal/third-party/.',
         'Оригинальные лицензии не заменяются нашей лицензией. APK и данные аккаунтов не включены.',
         '',
         'DRAFT INVENTORY',
         'Original copyright, license, NOTICE and AUTHORS texts are retained in legal/third-party/.',
         'This file does not relicense upstream code and is not a completed release audit.',
         'No game APK, account information, cookies or gameplay recordings are included.', '']
for component in records:
    lines += [component['name'] + (' ' + component['version'] if component.get('version') else ''),
              'Scope / Область: ' + component['scope'],
              'License / Лицензия: ' + (component.get('license_expression') or 'See original file-level terms; review pending')]
    if component.get('url'):
        lines.append('Upstream / Источник: ' + component['url'])
    if component.get('checked_out_revision'):
        lines.append('Revision / Ревизия: ' + component['checked_out_revision'])
    lines.extend('License/notice text / Текст: ' + item['path'] for item in component['license_files'])
    lines.append('')
lines += ['НЕЗАКРЫТЫЕ ПУНКТЫ РЕЛИЗА / REMAINING RELEASE REVIEW', *('- ' + gap for gap in gaps), '']
(ROOT / 'THIRD_PARTY_NOTICES.txt').write_text('\n'.join(lines))
print(json.dumps({'components': len(records), 'rust_packages': len(rust),
                  'observed_host_packages': len(packages), 'remaining_review': len(gaps)}))
