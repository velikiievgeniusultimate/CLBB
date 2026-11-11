#!/usr/bin/env python3
"""Exercise actual metadata logger with synthetic sensitive messages; no HTTP."""
import os,pathlib,shlex,subprocess,tempfile
root=pathlib.Path(__file__).resolve().parent.parent;prefix=root/'prefix'
with tempfile.TemporaryDirectory(prefix='clbb-web-diag-') as directory:
    temp=pathlib.Path(directory);binary=temp/'diagnostics'
    flags=shlex.split(subprocess.check_output(['pkg-config','--cflags','--libs','webkitgtk-6.0','json-glib-1.0'],text=True))
    subprocess.run(['cc','-std=c11','-O2','-Wall','-Wextra','-Werror',str(root/'smoke/webkit-diagnostics.c'),
                    str(root/'src/atl/src/api-impl-jni/widgets/webview_diagnostics.c'),
                    '-I'+str(root/'src/atl/src/api-impl-jni/widgets'),*flags,'-o',str(binary)],check=True)
    environment=os.environ.copy()
    for name,key in [('runtime','XDG_RUNTIME_DIR'),('cache','XDG_CACHE_HOME'),('data','XDG_DATA_HOME')]:
        path=temp/name;path.mkdir(mode=0o700);environment[key]=str(path)
    environment.update(GTK_A11Y='none',GDK_BACKEND='x11',GTK_USE_PORTAL='0',GSK_RENDERER='cairo',
        CLBB_WEBKIT_PROCESS_DIR=str(prefix/'lib/webkitgtk-6.0'),
        WEBKIT_INJECTED_BUNDLE_PATH=str(prefix/'lib/webkitgtk-6.0/injected-bundle'),
        LD_PRELOAD=str(prefix/'lib/libclbb_webkit_launch_adapter.so'))
    result=subprocess.run(['timeout','12s','xvfb-run','-a','dbus-run-session','--',str(binary)],
                            env=environment,capture_output=True,text=True)
    output=result.stdout+result.stderr
    checks=['ATL_WEBVIEW_JS kind=error type=ReferenceError undefined_api=1',
            'ATL_WEBVIEW_JS kind=promise type=TypeError undefined_api=1',
            'ATL_WEBVIEW_JS kind=error type=Error undefined_api=0 storage=0 network=0',
            'ATL_WEBVIEW_JS kind=csp directive=other',
            'ATL_WEBVIEW_COOKIE_STATE host=mtacc.mobilelegends.com count=0 error=0']
    if result.returncode or 'LOCAL_PRIVATE_SENTINEL' in output or not all(x in output for x in checks):
        raise SystemExit('Web diagnostics privacy/capture test FAIL; no raw messages displayed')
    print('WEB_DIAGNOSTICS error/promise/CSP classifications and cookie count PASS')
    print('WEB_DIAGNOSTICS sensitive/invalid values omitted PASS')
