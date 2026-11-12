#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Diagnostic fallback that still runs if the Rust launcher cannot load."""
import json,os,platform,re,subprocess,sys,time
from pathlib import Path
import urllib.request
ENDPOINT='https://clbb-downloads.velikiievgenius.com/reports'
def clean(text):
    home=str(Path.home());out=[]
    for line in text.splitlines():
        if any(word in line.lower()for word in ['password','passwd','cookie','token','authorization','session','email','moonton'])or '@'in line or 'http://'in line or 'https://'in line:continue
        out.append(line.replace('\0','').replace(home,'~')[:1000])
    return '\n'.join(out)[:24000]
def tail(path):
    try:
        with path.open('rb')as f:f.seek(0,2);size=f.tell();f.seek(max(0,size-65536));return f.read().decode(errors='replace')
    except OSError:return ''
def run(args,env=None):
    try:
        value=subprocess.run(args,env=env,capture_output=True,text=True,timeout=20);return clean(value.stdout+'\n'+value.stderr)
    except (OSError,subprocess.TimeoutExpired)as error:return str(error)
def positive_int(value):
    return type(value) is int and 0 < value <= 2**64-1

def invocation_id(value):
    return value if isinstance(value,str) and re.fullmatch(r'[a-zA-Z0-9-]{1,100}',value) else None

def birth(pid):
    try:return int(Path(f'/proc/{pid}/stat').read_text().rsplit(')',1)[1].split()[19])
    except (OSError,ValueError,IndexError):return None

def boot_id():
    try:return Path('/proc/sys/kernel/random/boot_id').read_text().strip()
    except OSError:return ''

def public_identity(record):
    keys=['schema','backend','pid','launcher_pid','pid_start_ticks','launcher_start_ticks','start_monotonic_ns','start_unix_ns','boot_id','runtime_root','runtime_version','runtime_revision','launch_directory','invocation_id']
    return {key:(str(value).replace(str(Path.home()),'~') if key in ['runtime_root','launch_directory'] else value) for key,value in record.items() if key in keys}

def resolve_launch(state,expected_invocation=None):
    """Only the exact canonical pointer may nominate a run; never scan logs."""
    record={}
    def unknown(reason):
        return None,{'status':'unknown','source':'current_pointer','reason':reason,'expected':public_identity(record),'runtime_log_attached':False}
    try:record=json.loads((state/'logs/current-atl-pid.json').read_text())
    except (OSError,ValueError):return unknown('Canonical current launch pointer is missing or invalid')
    if not isinstance(record,dict):record={};return unknown('Invalid canonical launch record')
    if record.get('schema')!='clbb-launch-scope-v2':return unknown('Legacy launch record lacks verified process birth and runtime manifest')
    if expected_invocation is not None and record.get('invocation_id')!=expected_invocation:return unknown('Canonical pointer belongs to a different wrapper invocation')
    for key in ['pid','launcher_pid','pid_start_ticks','launcher_start_ticks','start_monotonic_ns','start_unix_ns']:
        if not positive_int(record.get(key)):return unknown('Launch identity is incomplete: '+key)
    if record['pid']>2**32-1 or record['launcher_pid']>2**32-1 or not boot_id() or record.get('boot_id')!=boot_id():return unknown('Launch boot or process identity does not match this host boot')
    for key in ['runtime_root','runtime_version','runtime_revision']:
        if not isinstance(record.get(key),str) or not record[key]:return unknown('Missing launch manifest field '+key)
    if not Path(record['runtime_root']).is_absolute():return unknown('Runtime root is not absolute')
    run=(state/'logs').resolve()/f"qemu-launch-{record['start_monotonic_ns']}-{record['launcher_pid']}"
    try:
        if Path(record.get('launch_directory','')).resolve()!=run:return unknown('Launch directory does not match its monotonic start and launcher PID')
        actual=json.loads((run/'launch.json').read_text())
    except (OSError,ValueError,TypeError):return unknown('Exact launch.json is missing or invalid')
    if not isinstance(actual,dict):return unknown('Exact launch.json is not an object')
    for key in ['schema','backend','pid','launcher_pid','start_monotonic_ns','start_unix_ns','pid_start_ticks','launcher_start_ticks','boot_id','runtime_root','runtime_version','runtime_revision','launch_directory','invocation_id']:
        if actual.get(key)!=record.get(key):return unknown('Exact launch.json identity mismatch: '+key)
    for key,start_key in [('pid','pid_start_ticks'),('launcher_pid','launcher_start_ticks')]:
        current=birth(record[key])
        if current is not None and current!=record[start_key]:return unknown(key+' has been reused by a different process birth')
    return (run,record),{'status':'verified_launch','source':'current_pointer','identity':public_identity(actual),'runtime_log_attached':True,'version_source':'launch_manifest'}

def launch_exit(context):
    if context is None:return {}
    directory,record=context
    try:
        value=json.loads((directory/'exit.json').read_text())
        if isinstance(value,dict) and all(value.get(k)==record.get(k) for k in ['pid','launcher_pid','start_monotonic_ns']):return value
    except (OSError,ValueError):pass
    return {}

def crash_stack(context,exit_record):
    if context is None:return ''
    directory,record=context
    # PID alone can select a prior coredump. Limit journal lookup to the exact
    # recorded boot and wall-clock launch/exit interval, or omit it entirely.
    start,end=record.get('start_unix_ns'),exit_record.get('finished_unix_ns')
    if not positive_int(start) or not positive_int(end) or end<start:return ''
    result=run(['coredumpctl','--no-pager',f'--since=@{start//10**9}',f'--until=@{(end+10**9-1)//10**9}','info',f"COREDUMP_PID={record['pid']}",f"_BOOT_ID={record['boot_id'].replace('-','')}"])
    return '\n'.join(line for line in result.splitlines()if re.search(r'Stack trace of thread|^\s*#\d+\s',line))[:16000]

def summarize(reason,errors,checks):
    text=errors+'\n'+str(checks);lower=(reason+'\n'+text).lower();exit=reason.lower()
    if 'sigkill'in exit or 'signal: 9 ('in exit:return 'Процесс CLBB принудительно завершён (SIGKILL). Сам по себе этот сигнал не указывает на ошибку OpenGL.'
    if 'sigill'in exit or 'signal: 4 ('in exit:return 'Нативное падение процесса по сигналу SIGILL. Причину нужно определить по точному журналу и стеку этого запуска; сам сигнал не доказывает несовместимость CPU.'
    if any(key in text for key in ['couldn\'t find "libmain.so"','Failed to load \'libmain.so\'']):return 'Не подготовлены ARM64-библиотеки APK. Требуется установка CLBB с подготовкой библиотек игры.'
    if 'not found'in text and any(key in text for key in ['GLIBC_','GLIBCXX_']):return 'Несовместимая версия glibc/libstdc++. Обновите дистрибутив; точные версии есть в отчёте.'
    if 'not found'in text or 'error while loading shared libraries'in text:return 'Возможна отсутствующая системная библиотека или файл. Смотрите проверки в отчёте.'
    if 'illegal instruction'in lower or 'sigill'in lower:return 'Нативное падение процесса по сигналу SIGILL. Причину нужно определить по точному журналу и стеку этого запуска; сам сигнал не доказывает несовместимость CPU.'
    if 'sigsegv'in lower or 'segmentation'in lower:return 'Нативное падение процесса (SIGSEGV). Отправьте технический отчёт для разбора.'
    return reason

def collect(root,state,reason,expected_invocation=None):
    logs=state/'logs';logs.mkdir(parents=True,exist_ok=True)
    context,provenance=resolve_launch(state,expected_invocation)
    errors=''
    # The legacy shared stderr file has no reliable invocation identity.
    # A new wrapper provides an id and a derived, non-overwritten stderr path.
    stderr_provenance={'status':'unknown','reason':'No wrapper invocation; shared launcher-stderr.log is not trusted'}
    marker=invocation_id(expected_invocation)
    if marker:
        stderr_path=logs/f'launcher-stderr-{marker}.log'
        errors=tail(stderr_path);stderr_provenance={'status':'explicit_invocation','invocation_id':marker,'path':str(stderr_path).replace(str(Path.home()),'~')}
    if context:
        errors+='\n'+'\n'.join(line for line in tail(context[0]/'runtime.log').splitlines()if 'getSignalHandler'not in line and any(key in line.lower()for key in ['error','fatal','failed','not found','exception','signal','assert','traceback']))
    checked_root=Path(context[1]['runtime_root']) if context else root
    env=os.environ.copy();env.pop('LD_PRELOAD',None);env['LC_ALL']='C';env['LD_LIBRARY_PATH']=':'.join(str(checked_root/p)for p in ['prefix/lib','prefix/lib/art','prefix/lib/java/dex/art/natives'])
    checks=[{'file':p,'result':run(['ldd',str(checked_root/p)],env)}for p in ['prefix/bin/android-translation-layer','prefix/bin/dalvikvm','prefix/lib/libnb-qemu.so','bin/clbb-launcher']]
    summary=summarize(reason,errors,checks)
    cpu=Path('/proc/cpuinfo').read_text(errors='replace')if Path('/proc/cpuinfo').exists()else ''
    exit_record=launch_exit(context)
    return {'crash_stack':crash_stack(context,exit_record),'private_state':{'identity_present':(state/'device/android-id').is_file(),'guest_fonts_visible':(state/'guest-root/system/etc/fonts.xml').is_file()},
        'last_exit':exit_record,'last_launch':str(context[0]).replace(str(Path.home()),'~') if context else None,'launch_provenance':provenance,'launcher_stderr_provenance':stderr_provenance,
        'diagnostic_runtime':{'root':str(root).replace(str(Path.home()),'~'),'version':(root/'VERSION').read_text().strip()if(root/'VERSION').is_file()else 'unknown','revision':(root/'REVISION').read_text().strip()if(root/'REVISION').is_file()else ''},
        'game_native':{name:(state/'data/base.apk_/lib'/name).is_file()for name in ['libmain.so','libEncryptorP.so','libunity.so','libil2cpp.so']},'schema':'clbb-diagnostic-v1',
        'app_revision':context[1]['runtime_revision'] if context else '', 'app_version':context[1]['runtime_version'] if context else 'unknown','kind':'launcher_fallback','reason':clean(reason),'summary':summary,'diagnostic_invocation_id':marker,
        'host':{'kernel':platform.platform(),'glibc':platform.libc_ver(),'display_available':bool(os.environ.get('DISPLAY')or os.environ.get('WAYLAND_DISPLAY')),'cpu':[line for line in cpu.splitlines()if line.startswith(('model name','flags'))][:2]},
        'checks':checks,'checks_scope':'Current files of the verified launch runtime, or diagnostic runtime when launch identity is unknown; not crash-time binary contents.','startup_errors':clean(errors)}

def main():
    root=Path(sys.argv[1]);state=Path(sys.argv[2]);reason=sys.argv[3]if len(sys.argv)>3 else 'Проверка запуска CLBB по запросу'
    value=collect(root,state,reason,invocation_id(os.environ.get('CLBB_DIAGNOSTIC_INVOCATION')));directory=state/'logs/reports';directory.mkdir(parents=True,exist_ok=True);directory.chmod(0o700)
    path=directory/f'report-{time.time_ns()}.json';path.write_text(json.dumps(value,ensure_ascii=False,indent=2));path.chmod(0o600)
    (state/'logs/last-diagnostic.json').write_text(path.read_text());(state/'logs/last-diagnostic.json').chmod(0o600)
    print(value['summary']);print('Отчёт / Report:',path)
    if not(os.environ.get('DISPLAY')or os.environ.get('WAYLAND_DISPLAY')):return
    try:
        choice=subprocess.run(['zenity','--question','--no-markup','--width=620','--title=CLBB — Диагностика','--ok-label=Отправить отчёт на сервер','--cancel-label=Закрыть','--text',value['summary']+'\n\nОтчёт: '+str(path)+'\n\nНа StarFive будет отправлен технический отчёт без паролей, cookies и файлов аккаунта.'])
        if choice.returncode:return
        request=urllib.request.Request(ENDPOINT,data=path.read_bytes(),headers={'Content-Type':'application/json'},method='POST')
        with urllib.request.urlopen(request,timeout=60)as response:result=json.load(response)
        identifier=result.get('id','')
        if not re.fullmatch('[a-zA-Z0-9-]{1,100}',identifier):raise ValueError('Invalid report id')
        print('Report sent:',identifier)
        subprocess.run(['zenity','--info','--no-markup','--title=CLBB — Отчёт отправлен','--text','Отчёт отправлен на StarFive.\nНомер: '+identifier])
    except Exception as error:
        print('Report retained locally:',error,file=sys.stderr)
        try:subprocess.run(['zenity','--error','--no-markup','--title=CLBB — Диагностика','--text','Не удалось отправить отчёт. Локальная копия сохранена:\n'+str(path)])
        except OSError:pass
if __name__=='__main__':main()
