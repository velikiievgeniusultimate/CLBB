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
        out.append(line.replace(home,'~')[:1000])
    return '\n'.join(out)[:24000]
def tail(path):
    try:
        with path.open('rb')as f:f.seek(0,2);size=f.tell();f.seek(max(0,size-65536));return f.read().decode(errors='replace')
    except OSError:return ''
def run(args,env=None):
    try:
        value=subprocess.run(args,env=env,capture_output=True,text=True,timeout=20);return clean(value.stdout+'\n'+value.stderr)
    except (OSError,subprocess.TimeoutExpired)as error:return str(error)
def collect(root,state,reason):
    logs=state/'logs';logs.mkdir(parents=True,exist_ok=True)
    latest=sorted(logs.glob('qemu-launch-*'));errors=tail(logs/'launcher-stderr.log')
    if latest:
        errors+='\n'+'\n'.join(line for line in tail(latest[-1]/'runtime.log').splitlines()if any(key in line.lower()for key in ['error','fatal','failed','not found','exception','signal','assert','traceback']))
    env=os.environ.copy();env.pop('LD_PRELOAD',None);env['LC_ALL']='C';env['LD_LIBRARY_PATH']=':'.join(str(root/p)for p in ['prefix/lib','prefix/lib/art','prefix/lib/java/dex/art/natives'])
    checks=[{'file':p,'result':run(['ldd',str(root/p)],env)}for p in ['prefix/bin/android-translation-layer','prefix/bin/dalvikvm','prefix/lib/libnb-qemu.so','bin/clbb-launcher']]
    text=errors+'\n'+str(checks);summary=reason
    if 'not found'in text and any(key in text for key in ['GLIBC_','GLIBCXX_']):summary='Несовместимая версия glibc/libstdc++. Обновите дистрибутив; точные версии есть в отчёте.'
    elif 'not found'in text:summary='Возможна отсутствующая системная библиотека или файл. Смотрите проверки в отчёте.'
    elif 'Illegal instruction'in text:summary='Неподдерживаемая инструкция CPU (SIGILL). Нужна проверка совместимости сборки.'
    cpu=Path('/proc/cpuinfo').read_text(errors='replace')if Path('/proc/cpuinfo').exists()else ''
    return {'schema':'clbb-diagnostic-v1','app_version':(root/'VERSION').read_text().strip()if(root/'VERSION').is_file()else 'unknown','kind':'launcher_fallback','reason':clean(reason),'summary':summary,'host':{'kernel':platform.platform(),'glibc':platform.libc_ver(),'display_available':bool(os.environ.get('DISPLAY')or os.environ.get('WAYLAND_DISPLAY')),'cpu':[line for line in cpu.splitlines()if line.startswith(('model name','flags'))][:2]},'checks':checks,'startup_errors':clean(errors)}
def main():
    root=Path(sys.argv[1]);state=Path(sys.argv[2]);reason=sys.argv[3]if len(sys.argv)>3 else 'Проверка запуска CLBB по запросу'
    value=collect(root,state,reason);directory=state/'logs/reports';directory.mkdir(parents=True,exist_ok=True);directory.chmod(0o700)
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
