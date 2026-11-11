#!/usr/bin/env python3
"""Contract-test the real ATL manager and three real PackageManager methods.

Only package parsing/class loading are fixtures. The production presence logic
is compiled unchanged; no Android runtime or running game is needed.
"""
from pathlib import Path
import os
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
PM = ROOT / 'src/atl/src/api-impl/android/content/pm/PackageManager.java'
MANAGER = ROOT / 'src/atl/src/api-impl/android/atl/ATLLoadedAppManager.java'


def extract(signature):
    text = PM.read_text()
    start = text.index(signature)
    opening = text.index('{', start)
    depth, state, i = 1, 'normal', opening + 1
    while depth:
        c, pair = text[i], text[i:i + 2]
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
            if pair == '*/':
                state = 'normal'
                i += 2
                continue
        elif pair in ('//', '/*'):
            state = 'line' if pair == '//' else 'block'
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


SOURCES = {
'android/annotation/Nullable.java': 'package android.annotation; public @interface Nullable {}',
'android/util/DisplayMetrics.java': 'package android.util; public class DisplayMetrics {}',
'android/util/Slog.java': '''package android.util; public class Slog {
 public static void i(String tag,String text) {} public static void e(String tag,String text) {}
 public static void e(String tag,String text,Throwable error) {} }''',
'android/content/pm/ApplicationInfo.java': '''package android.content.pm; public class ApplicationInfo {
 public String packageName, nativeLibraryDir, sourceDir; }''',
'android/content/pm/PackageInfo.java': '''package android.content.pm; public class PackageInfo {
 public String packageName; public ApplicationInfo applicationInfo; public Signature[] signatures; }''',
'android/content/pm/Signature.java': 'package android.content.pm; public class Signature { public Signature(byte[] data) {} }',
'android/content/pm/PackageUserState.java': 'package android.content.pm; public class PackageUserState {}',
'android/content/pm/PackageParser.java': '''package android.content.pm;
import java.io.File; import java.util.*; import android.util.DisplayMetrics;
public class PackageParser {
 public static int parsed;
 public static class Package {
  public final String packageName;
  public final ApplicationInfo applicationInfo = new ApplicationInfo();
  public final List<String> requestedPermissions = new ArrayList<>();
  public Package(String name) {packageName=name;applicationInfo.packageName=name;}
 }
 public PackageParser(String name) {}
 public Package parsePackage(File apk,String path,DisplayMetrics metrics,int flags) {
  parsed++;Package pkg=new Package(apk.getName().replaceFirst("\\\\.apk$", ""));
  pkg.applicationInfo.sourceDir=path;return pkg;
 }
 public void collectCertificates(Package pkg,int flags) {}
 public static PackageInfo generatePackageInfo(Package pkg,int[] gids,int flags,long first,long last,
                                               HashSet<String> granted,PackageUserState state) {
  PackageInfo info=new PackageInfo();info.packageName=pkg.packageName;info.applicationInfo=pkg.applicationInfo;return info;
 }
 public static ApplicationInfo generateApplicationInfo(Package pkg,int flags,PackageUserState state) {return pkg.applicationInfo;}
}''',
'android/atl/ATLPaths.java': '''package android.atl; import java.io.File; public class ATLPaths {
 public static final File installed_apks_dir=new File(System.getProperty("test.installed"));
 public static final File app_data_dir_base=installed_apks_dir;
}''',
'android/atl/ATLLoadedApp.java': '''package android.atl;
import android.content.pm.PackageParser; import java.io.*; import java.util.*;
public class ATLLoadedApp {
 static final HashSet<String> play_services = new HashSet<>(Arrays.asList("com.google.android.gms","com.google.android.gsf","com.android.vending"));
 public final PackageParser.Package pkg;
 static final ATLLoadedApp primary=new ATLLoadedApp("com.mobile.legends"), system=new ATLLoadedApp("android");
 public ATLLoadedApp(String name) {pkg=new PackageParser.Package(name);}
 public static ATLLoadedApp getPrimaryApplication(){return primary;}
 public static ATLLoadedApp getSystemApplication(){return system;}
 public static ATLLoadedApp loadFromPath(String apk,String libs,String loader) throws IOException {
  ATLLoadedApp app=new ATLLoadedApp(new File(apk).getName().replaceFirst("\\\\.apk$", ""));
  app.pkg.applicationInfo.sourceDir=apk;return app;
 }
}''',
'android/atl/ATLSigHelper.java': '''package android.atl; import android.content.pm.PackageParser;
public class ATLSigHelper {public static void addGMSSignatures(PackageParser.Package pkg) {}}''',
'android/atl/ATLGStub.java': '''package android.atl; public class ATLGStub {
 public static int calls;
 static ATLLoadedApp loadFakePlayServices(String name) {calls++;return new ATLLoadedApp(name);}
}''',
}
CONTRACT = r'''
import java.io.File;
import java.util.*;
import android.atl.*;
import android.content.pm.*;
public class GooglePresenceContract {
 static final int PERMISSION_GRANTED=0,PERMISSION_DENIED=-1;
 static class NameNotFoundException extends Exception {NameNotFoundException(String name){super(name);}}
 PackageInfo package_info=PackageParser.generatePackageInfo(ATLLoadedApp.getPrimaryApplication().pkg,
                new int[0],0,0,0,new HashSet<>(),new PackageUserState());
 __METHODS__
 static void require(boolean valid,String message){if(!valid)throw new AssertionError(message);}
 public static void main(String[] args)throws Exception {
  GooglePresenceContract pm=new GooglePresenceContract();
  String[] google={"com.google.android.gms","com.google.android.gsf","com.android.vending"};
  String send="com.google.android.c2dm.permission.SEND";
  boolean opt="1".equals(System.getenv("ATL_ENABLE_GMS_STUB"));
  if(args.length>0 && args[0].startsWith("installed")) {
   String name=google[0];File apk=new File(ATLPaths.installed_apks_dir,name+".apk");
   require(apk.createNewFile(),"installed fixture existed");
   if(args[0].equals("installed-application"))
    require(ATLLoadedAppManager.getAppFromPackageName(name)!=null,"Real APK app missing");
   require(pm.getPackageInfo(name,0).packageName.equals(name),"Real APK package missing");
   require(pm.getApplicationInfo(name,0).packageName.equals(name),"Real APK info missing");
   require(pm.checkPermission(send,name)==PERMISSION_GRANTED,"Real APK SEND regression");
   require(ATLGStub.calls==0,"Real APK replaced with stub");
   System.out.println("GOOGLE_PRESENCE_OK mode="+args[0]+" real_apk=1 no_stub=1");return;
  }
  for(int i=0;i<google.length;i++) {
   String name=google[i];
   if(i==0)require((ATLLoadedAppManager.getAppFromPackageName(name)!=null)==opt,"loadApplication presence "+name);
   try {PackageInfo info=pm.getPackageInfo(name,0);require(opt,"Absent package was reported installed "+name);
        require(info.packageName.equals(name),"Wrong opt-in package");}
   catch(NameNotFoundException error){require(!opt,"Opt-in package missing");}
   try {pm.getApplicationInfo(name,0);require(opt,"Absent ApplicationInfo "+name);}
   catch(NameNotFoundException error){require(!opt,"Opt-in ApplicationInfo missing");}
   require(pm.checkPermission(send,name)==(opt?PERMISSION_GRANTED:PERMISSION_DENIED),"C2DM presence mismatch "+name);
   require((ATLLoadedAppManager.getAppFromPackageName(name)!=null)==opt,"App presence mismatch "+name);
  }
  require(ATLGStub.calls==(opt?3:0),"Unexpected stub creation count");
  String primary=ATLLoadedApp.getPrimaryApplication().pkg.packageName;
  ATLLoadedApp.getPrimaryApplication().pkg.requestedPermissions.add("android.permission.INTERNET");
  require(pm.getPackageInfo(primary,0).packageName.equals(primary),"Primary PackageInfo changed");
  require(pm.getApplicationInfo(primary,0).packageName.equals(primary),"Primary ApplicationInfo changed");
  require(pm.checkPermission("android.permission.INTERNET",primary)==PERMISSION_GRANTED,"Primary internet changed");
  require(pm.checkPermission(send,primary)==PERMISSION_GRANTED,"Primary grant changed");
  for(String permission:new String[]{send,"android.permission.ACCESS_NETWORK_STATE","x.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION"})
   require(pm.checkPermission(permission,"com.unknown.absent")==PERMISSION_DENIED,"Unknown package granted");
  System.out.println("GOOGLE_PRESENCE_OK explicit_stub="+opt+" both_load_paths=1 primary_preserved=1 unknown_denied=1");
 }
}
'''
methods = '\n'.join(extract(signature) for signature in (
    'public int checkPermission(String permName, String pkgName)',
    'public PackageInfo getPackageInfo(String packageName, int flags)',
    'public ApplicationInfo getApplicationInfo(String packageName,'))
with tempfile.TemporaryDirectory(prefix='clbb-google-presence-') as temporary:
    directory=Path(temporary)
    SOURCES['android/atl/ATLLoadedAppManager.java']=MANAGER.read_text()
    SOURCES['GooglePresenceContract.java']=CONTRACT.replace('__METHODS__',methods)
    files=[]
    for name,text in SOURCES.items():
        file=directory/name;file.parent.mkdir(parents=True,exist_ok=True);file.write_text(text);files.append(str(file))
    subprocess.run(['javac','-d',str(directory),*files],check=True)
    for index,(value,args) in enumerate([(None,[]),('0',[]),('true',[]),('1',[]),(None,['installed-package']),(None,['installed-application'])]):
        installed=directory/f'installed-{index}';installed.mkdir()
        environment=dict(os.environ);environment.pop('ATL_ENABLE_GMS_STUB',None)
        if value is not None:environment['ATL_ENABLE_GMS_STUB']=value
        subprocess.run(['java','-ea','-Dtest.installed='+str(installed),'-cp',str(directory),
                        'GooglePresenceContract',*args],env=environment,check=True,timeout=10)
