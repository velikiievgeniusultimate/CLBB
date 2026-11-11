#!/usr/bin/env python3
"""Exercise the exact production checkPermission body without ART/game input."""
from pathlib import Path
import os
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
PRODUCTION = ROOT / 'src/atl/src/api-impl/android/content/pm/PackageManager.java'


def method_body():
    text = PRODUCTION.read_text()
    start = text.index('public int checkPermission(String permName, String pkgName) {')
    opening = text.index('{', start)
    depth, state, i = 1, 'normal', opening + 1
    while depth:
        c, following = text[i], text[i:i + 2]
        if state in ('string', 'char'):
            if c == '\\':
                i += 2
                continue
            if c == ('"' if state == 'string' else "'"):
                state = 'normal'
        elif state == 'line':
            if c == '\n':
                state = 'normal'
        elif state == 'block':
            if following == '*/':
                state = 'normal'
                i += 2
                continue
        elif following in ('//', '/*'):
            state = 'line' if following == '//' else 'block'
            i += 2
            continue
        elif c in ('"', "'"):
            state = 'string' if c == '"' else 'char'
        elif c == '{':
            depth += 1
        elif c == '}':
            depth -= 1
        i += 1
    return text[start:i]


FIXTURE = r'''
import java.io.File;
import java.util.*;
class PackageParser {
 static class Package {
  final String packageName;
  final List<String> requestedPermissions = new ArrayList<>();
  Package(String name) { packageName = name; }
 }
}
class ATLLoadedApp {
 final PackageParser.Package pkg = new PackageParser.Package("com.mobile.legends");
 static final ATLLoadedApp primary = new ATLLoadedApp();
 static ATLLoadedApp getPrimaryApplication() { return primary; }
}
class ATLLoadedAppManager {
 static int calls;
 static final Map<String,PackageParser.Package> registry = new HashMap<>();
 static PackageParser.Package getPackageFromPackageName(String name) {
  calls++;
  // Mirrors the manager's compatibility stub: non-null without an installed APK.
  if (name.equals("com.google.android.gms")) return new PackageParser.Package(name);
  return registry.get(name);
 }
}
public class PermissionContract {
 static final int PERMISSION_GRANTED = 0, PERMISSION_DENIED = -1;
 static int checks;
 __PRODUCTION_METHOD__
 static void expect(PermissionContract pm, String permission, String pkg, int expected) {
  checks++;
  int got = pm.checkPermission(permission, pkg);
  if (got != expected) throw new AssertionError(pkg + " / " + permission + ": " + got + " != " + expected);
 }
 public static void main(String[] args) throws Exception {
  PermissionContract pm = new PermissionContract();
  String primary = ATLLoadedApp.primary.pkg.packageName;
  expect(pm,"android.permission.RECORD_AUDIO",primary,System.getenv("ATL_UGLY_ENABLE_MICROPHONE")!=null?PERMISSION_GRANTED:PERMISSION_DENIED);
  String send = "com.google.android.c2dm.permission.SEND";
  String internet = "android.permission.INTERNET";
  String storage = "android.permission.READ_EXTERNAL_STORAGE";
  String network = "android.permission.ACCESS_NETWORK_STATE";
  String dynamic = "com.example.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION";
  for (String missing : new String[]{"com.google.android.gms", "com.absent.service", "android.absent", ""}) {
   for (String permission : new String[]{send, internet, storage, network, dynamic})
    expect(pm, permission, missing, PERMISSION_DENIED);
  }
  if (ATLLoadedAppManager.calls != 0) throw new AssertionError("Absent permission check resolved/created a stub");
  expect(pm, null, primary, PERMISSION_DENIED);
  expect(pm, send, null, PERMISSION_DENIED);
  ATLLoadedApp.primary.pkg.requestedPermissions.add(internet);
  expect(pm, internet, primary, PERMISSION_GRANTED);
  ATLLoadedApp.primary.pkg.requestedPermissions.remove(internet);
  expect(pm, internet, primary, PERMISSION_DENIED);
  for (String permission : new String[]{send, storage, network, dynamic})
   expect(pm, permission, primary, PERMISSION_GRANTED);
  expect(pm, "android.permission.NOT_IMPLEMENTED", primary, PERMISSION_DENIED);
  for (String runtime : new String[]{"android", "atl"}) {
   expect(pm, send, runtime, PERMISSION_GRANTED);
   expect(pm, network, runtime, PERMISSION_GRANTED);
  }
  String installed = "com.installed.secondary";
  File apk = new File(android.atl.ATLPaths.installed_apks_dir, installed + ".apk");
  if (!apk.createNewFile()) throw new AssertionError("APK fixture already existed");
  // A file alone does not prove the package manager recognizes an installed app.
  expect(pm, send, installed, PERMISSION_DENIED);
  ATLLoadedAppManager.registry.put(installed, new PackageParser.Package(installed));
  expect(pm, send, installed, PERMISSION_GRANTED);
  expect(pm, storage, installed, PERMISSION_GRANTED);
  expect(pm, network, installed, PERMISSION_GRANTED);
  expect(pm, dynamic, installed, PERMISSION_GRANTED);
  // Existing primary-only INTERNET semantics remain unchanged in this patch.
  expect(pm, internet, installed, PERMISSION_DENIED);
  if (!apk.delete()) throw new AssertionError("Unable to remove APK fixture");
  int queries = ATLLoadedAppManager.calls;
  expect(pm, send, installed, PERMISSION_DENIED);
  if (ATLLoadedAppManager.calls != queries) throw new AssertionError("Cached app without APK was treated as installed");
  System.out.println("PACKAGE_PERMISSIONS_OK cases=" + checks + " absent_gms_denied=1 primary_grants_preserved=1 installed_resolution=1 no_stub_creation=1");
 }
}
'''

with tempfile.TemporaryDirectory(prefix='clbb-package-permissions-') as temporary:
    directory = Path(temporary)
    installed = directory / 'installed'
    installed.mkdir()
    source = directory / 'PermissionContract.java'
    source.write_text(FIXTURE.replace('__PRODUCTION_METHOD__', method_body()))
    paths = directory / 'android/atl/ATLPaths.java'
    paths.parent.mkdir(parents=True)
    paths.write_text('package android.atl; import java.io.File; public class ATLPaths { '
                     'public static final File installed_apks_dir = new File(System.getProperty("test.installed")); }')
    subprocess.run(['javac', '-d', str(directory), str(source), str(paths)], check=True)
    environment = {key: value for key, value in os.environ.items()
                   if key not in ('ATL_UGLY_ENABLE_LOCATION', 'ATL_UGLY_ENABLE_MICROPHONE', 'ATL_ENABLE_GMS_STUB')}
    for mic in (False, True):
        invocation=dict(environment)
        if mic: invocation['ATL_UGLY_ENABLE_MICROPHONE']='1'
        subprocess.run(['java', '-ea', '-Dtest.installed=' + str(installed), '-cp', str(directory),
                        'PermissionContract'], env=invocation, check=True, timeout=10)
