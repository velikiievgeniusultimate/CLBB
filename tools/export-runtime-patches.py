#!/usr/bin/env python3
"""Export owned runtime changes with explicit new-file allowlists and apply checks.

Only source trees and patches are read/written. No build, install, game launch,
Git index mutation or wildcard inclusion of untracked files is performed.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
SPECS = [
    ('libopensles-standalone', 'src/libopensles-standalone', 'opensles-runtime.patch', []),
    ('atl', 'src/atl', 'atl-mlbb-api.patch', [
        'src/api-impl-jni/generated_headers/android_graphics_SurfaceTexture.h',
        'src/api-impl-jni/graphics/atl_surface_texture.c',
        'src/api-impl-jni/graphics/atl_surface_texture.h',
        'src/api-impl-jni/widgets/webview_cookie_policy.c',
        'src/api-impl-jni/widgets/webview_cookie_policy.h',
        'src/api-impl-jni/widgets/webview_diagnostics.c',
        'src/api-impl-jni/widgets/webview_diagnostics.h',
        'src/api-impl-jni/widgets/webview_session.c',
        'src/api-impl-jni/widgets/webview_session.h',
        'src/api-impl/android/media/audiofx/AcousticEchoCanceler.java',
        'src/api-impl/android/media/audiofx/NoiseSuppressor.java',
        'src/api-impl/android/media/audiofx/AutomaticGainControl.java',
        'src/api-impl/android/net/http/SslCertificate.java',
        'src/api-impl/android/net/http/SslError.java',
        'src/api-impl/android/webkit/JavascriptInterface.java',
        'src/api-impl/android/webkit/SslErrorHandler.java',
        'src/api-impl/android/widget/VideoView.java',
        'src/main-executable/clbb_mapper.c',
        'src/main-executable/clbb_mapper.h',
        'src/main-executable/restart.c',
        'src/main-executable/restart.h',
        'src/web-process-extension/atl-android-request.c',
    ]),
    ('nb-qemu', 'src/nb-qemu', 'bridge-runtime.patch', [
        'libnb-qemu/ClbbStartupTrace.h',
        'libnb-qemu/ExecutionBackend.h', 'libnb-qemu/GuestCore.h',
        'libnb-qemu/GuestCpu.h', 'libnb-qemu/GuestCpu.cpp',
        'libnb-qemu/QemuExecutionBackend.cpp',
        'libnb-qemu/ClbbDnsWire.h', 'libnb-qemu/DnsBridge.cpp',
        'libnb-qemu/DnsBridge.h', 'libnb-qemu/GLES_bio.h',
        'libnb-qemu/libGLESv2.itf.c',
    ]),
    ('qemu', 'src/nb-qemu/qemu', 'qemu-runtime.patch', [
        'target/arm/rsqrt-estimate-table.h',
        'linux-user/clone-signal-mask.h',
    ]),
    ('nb-qemu-bionic', 'src/nb-qemu/libnb-qemu-guest/bionic', 'bionic-runtime.patch', [
        'libc/bionic/standalone_system_properties.c',
        'libc/dns/net/clbb_dns.c', 'libc/private/clbb_dns.h',
        'libc/private/clbb_dns_wire.h',
    ]),
    ('art_standalone', 'src/art_standalone', 'art-runtime.patch', [
        'art/runtime/clbb_boot_park_trace.h',
    ]),
]


def git(repo, *args):
    return subprocess.check_output(['git', '-C', str(repo), *args])


def make_patch(repo, approved):
    patch = git(repo, 'diff', 'HEAD', '--binary', '--no-ext-diff', '--ignore-submodules=all')
    untracked = set(git(repo, 'ls-files', '--others', '--exclude-standard', '-z').decode().split('\0'))
    included = []
    for relative in approved:
        if relative not in untracked:
            continue
        path = repo / relative
        if not path.is_file() or path.is_symlink():
            raise RuntimeError('Expected regular owned source: ' + relative)
        result = subprocess.run(['git', 'diff', '--no-index', '--binary', '--', '/dev/null', relative],
                                cwd=repo, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        if result.returncode != 1:
            raise RuntimeError('Unable to export owned source: ' + relative)
        patch += result.stdout
        included.append(relative)
    paths = re.findall(rb'^diff --git a/([^\n]+) b/[^\n]+$', patch, re.M)
    if len(paths) != len(set(paths)):
        raise RuntimeError('Duplicate patch paths')
    return patch, [path.decode() for path in paths], included


def verify(repo, patch, paths, temporary):
    patch_file = temporary / 'change.patch'
    patch_file.write_bytes(patch)
    subprocess.run(['git', '-C', str(repo), 'apply', '--reverse', '--check', str(patch_file)], check=True)
    tree = temporary / 'base'
    tree.mkdir()
    for relative in paths:
        path = Path(relative)
        if path.is_absolute() or '..' in path.parts:
            raise RuntimeError('Unsafe patch path')
        entry = git(repo, 'ls-tree', 'HEAD', '--', relative).decode().strip()
        if not entry:
            continue
        mode = entry.split()[0]
        if mode not in ('100644', '100755'):
            raise RuntimeError('Unexpected source mode: ' + mode)
        target = tree / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(git(repo, 'show', 'HEAD:' + relative))
        target.chmod(0o755 if mode == '100755' else 0o644)
    subprocess.run(['git', 'apply', '--check', str(patch_file)], cwd=tree, check=True)
    subprocess.run(['git', 'apply', str(patch_file)], cwd=tree, check=True)
    for relative in paths:
        current, rebuilt = repo / relative, tree / relative
        if current.exists() != rebuilt.exists():
            raise RuntimeError('Reconstructed existence mismatch: ' + relative)
        if current.exists() and current.read_bytes() != rebuilt.read_bytes():
            raise RuntimeError('Reconstructed source mismatch: ' + relative)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--write', action='store_true', help='Replace patches after all validation passes')
    args = parser.parse_args()
    baselines = json.loads((ROOT / 'sources.json').read_text())
    records, outputs = [], []
    with tempfile.TemporaryDirectory(prefix='clbb-patch-export-') as temporary:
        temporary = Path(temporary)
        for index, (name, repository, filename, approved) in enumerate(SPECS):
            repo = ROOT / repository
            commit = git(repo, 'rev-parse', 'HEAD').decode().strip()
            if commit != baselines[name]['commit']:
                raise RuntimeError('Pinned source revision changed: ' + name)
            patch, paths, included = make_patch(repo, approved)
            check_dir = temporary / str(index)
            check_dir.mkdir()
            verify(repo, patch, paths, check_dir)
            output = ROOT / 'patches' / filename
            records.append({'patch': filename, 'source_commit': commit,
                            'files': len(paths), 'approved_untracked': included,
                            'bytes': len(patch), 'sha256': hashlib.sha256(patch).hexdigest(),
                            'reverse_check': True, 'clean_forward_check': True,
                            'reconstructed_sources_equal': True,
                            'changed': not output.exists() or output.read_bytes() != patch})
            outputs.append((output, patch))
        if args.write:
            for output, patch in outputs:
                fd, name = tempfile.mkstemp(prefix=output.name + '.', dir=output.parent)
                with os.fdopen(fd, 'wb') as file:
                    file.write(patch)
                os.chmod(name, 0o644)
                os.replace(name, output)
    report = {'written': args.write, 'patches': records}
    if args.write:
        (ROOT / 'logs').mkdir(exist_ok=True)
        (ROOT / 'logs/runtime-patch-export.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
