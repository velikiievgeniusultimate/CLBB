#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""CLBB release workbench: prepare once, test immutable artifacts, publish unchanged."""
import argparse,fcntl,hashlib,json,os,shlex,shutil,subprocess,sys,tempfile,tarfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
REPO='velikiievgeniusultimate/CLBB'

def run(*args,**kwargs):return subprocess.run(args,cwd=ROOT,check=True,**kwargs)
def git(*args):return subprocess.check_output(['git',*args],cwd=ROOT,text=True).strip()
def sha(path):
    with Path(path).open('rb')as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
def write(path,value):
    path=Path(path);path.parent.mkdir(parents=True,exist_ok=True)
    tmp=path.with_suffix('.tmp');tmp.write_text(json.dumps(value,indent=2,ensure_ascii=False)+'\n');tmp.replace(path)
def clean():
    # Patched external trees are deliberately dirty and separately validated by exporter.
    if git('status','--porcelain','--ignore-submodules=all'):
        raise ValueError('Commit project changes before preparing a release / Сначала зафиксируйте изменения')
def bundle(directory):
    directory=Path(directory).resolve();record=json.loads((directory/'candidate.json').read_text())
    for name,digest in record['assets'].items():
        if Path(name).name!=name or sha(directory/name)!=digest:raise ValueError('Candidate changed: '+name)
    return directory,record

def verify(directory):
    directory,record=bundle(directory)
    target=directory/'runtime'
    if target.is_symlink():raise ValueError('Candidate runtime cannot be a symlink')
    if target.exists():
        run('python3',str(ROOT/'distribution/verify.py'),str(target),str(target/'release.json'))
    else:
        target.mkdir()
        try:run('python3',str(ROOT/'distribution/extract.py'),str(directory/record['runtime']),str(target))
        except Exception:shutil.rmtree(target);raise
        run('python3',str(ROOT/'distribution/verify.py'),str(target),str(target/'release.json'))
    info=json.loads((target/'release.json').read_text())
    allowed=set(info['files'])|{'release.json'}
    extra=[str(path.relative_to(target))for path in target.rglob('*')if (path.is_file()or path.is_symlink())and str(path.relative_to(target))not in allowed]
    if extra:raise ValueError('Unexpected files in tested runtime: '+', '.join(extra[:5]))
    if info['version']!=record['version']:raise ValueError('Candidate version mismatch')
    if record.get('source_commit') and info.get('source_commit')!=record['source_commit']:raise ValueError('Candidate source revision mismatch')
    print('Verified / Проверен:',record['version'],record.get('source_commit')or record.get('published_source_commit','legacy release'))
    return directory,record,target

def prepare(directory):
    clean();revision=git('rev-parse','HEAD');version=(ROOT/'VERSION').read_text().strip()
    existing=subprocess.run(['git','rev-parse','--verify','refs/tags/'+version+'^{commit}'],cwd=ROOT,capture_output=True,text=True)
    if existing.returncode==0 and existing.stdout.strip()!=revision:raise ValueError('VERSION already released for another commit; bump VERSION first')
    destination=Path(directory).resolve()
    if destination.exists():raise ValueError('Choose a new output directory; candidates are immutable')
    check=json.loads(subprocess.check_output(['python3','tools/export-runtime-patches.py'],cwd=ROOT,text=True))
    if any(row['changed']for row in check['patches']):raise ValueError('Export and commit runtime patches first')
    # Existing-prefix build already checks exact running process identities before installs.
    run('bash','build-runtime.sh')
    run('bash','build-arm-zlib.sh')
    for project in ['mapper/core','mapper/runtime','launcher','distribution/manager','distribution/report-server']:
        run('cargo','test','--manifest-path',project+'/Cargo.toml','--locked','--offline')
    run('python3','-m','unittest','discover','-s','distribution/tests','-v')
    run('node','smoke/test-email-login-policy.cjs')
    clean()
    if git('rev-parse','HEAD')!=revision:raise ValueError('Source revision changed during build')
    run('python3','distribution/package-release.py','--output',str(destination),
        '--main-executable','src/atl/build/android-translation-layer',
        '--main-library','src/atl/build/libtranslation_layer_main.so','--api-jar','src/atl/build/api-impl.jar')
    run('python3','distribution/package-release.py','--source-only','--output',str(destination))
    names=['clbb-manager-linux-x86_64',f'clbb-{version}-linux-x86_64.tar.gz',f'clbb-{version}-sources.tar.gz']
    sums={name:sha(destination/name)for name in names}
    (destination/'SHA256SUMS').write_text(''.join(f'{digest}  {name}\n'for name,digest in sums.items()))
    sums['SHA256SUMS']=sha(destination/'SHA256SUMS')
    write(destination/'candidate.json',{'schema':1,'version':version,'source_commit':revision,'runtime':names[1],'assets':sums})
    verify(destination)
    print('Candidate ready. Run use-local, check actual gameplay, then accept with observed results.')

def import_release(directory,tag):
    directory=Path(directory).resolve()
    release=json.loads(subprocess.check_output(['gh','release','view',tag,'--repo',REPO,'--json','assets,isDraft'],cwd=ROOT,text=True))
    if release['isDraft']:raise ValueError('Select a published release')
    names=['clbb-manager-linux-x86_64',f'clbb-{tag}-linux-x86_64.tar.gz',f'clbb-{tag}-sources.tar.gz','SHA256SUMS']
    remote={a['name']:a for a in release['assets']};sums={}
    for name in names:
        digest=sha(directory/name)
        if name not in remote or remote[name].get('digest')!='sha256:'+digest:raise ValueError('Published asset differs: '+name)
        sums[name]=digest
    ref=json.loads(subprocess.check_output(['gh','api',f'repos/{REPO}/git/ref/tags/{tag}'],cwd=ROOT,text=True))['object']
    if ref['type']!='commit':raise ValueError('Annotated legacy tags must be resolved before import')
    with tarfile.open(directory/names[1])as archive:info=json.load(archive.extractfile('release.json'))
    if info['version']!=tag:raise ValueError('Published version mismatch')
    revision=info.get('source_commit')
    if revision and revision!=ref['sha']:raise ValueError('Published commit differs from payload')
    write(directory/'candidate.json',{'schema':1,'version':tag,'source_commit':revision,
        'published_source_commit':ref['sha'],'runtime':names[1],'assets':sums})
    verify(directory)

def use_local(directory):
    lockpath=ROOT/'logs/clbb-launch.lock';lockpath.parent.mkdir(parents=True,exist_ok=True)
    with lockpath.open('a+b')as lock:
        try:fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        except BlockingIOError:raise ValueError('Close CLBB before switching the local runtime')
        _use_local(directory)

def _use_local(directory):
    directory,record,target=verify(directory)
    data=Path(os.environ.get('XDG_DATA_HOME')or Path.home()/'.local/share')
    stable=data/'clbb-workbench'/('candidates/'+record['version']+'-'+record['assets'][record['runtime']][:12])
    if stable!=directory:
        if not stable.exists():
            stable.parent.mkdir(parents=True,exist_ok=True)
            staging=Path(tempfile.mkdtemp(prefix='.candidate-',dir=stable.parent))
            try:
                for name in [*record['assets'],'candidate.json']:shutil.copy2(directory/name,staging/name)
                staging.rename(stable)
            finally:
                if staging.exists():shutil.rmtree(staging)
        _,stored=bundle(stable)
        if stored!=record:raise ValueError('Stored local candidate metadata differs')
        directory,record,target=verify(stable)
    # Persistent Android data stays in the existing development profile; never packaged here.
    wrapper=ROOT/'.clbb-local-launch'
    wrapper.write_text('#!/bin/sh\nset -eu\n'+
        'export CLBB_ROOT='+shlex.quote(str(target))+'\n'+
        'export CLBB_STATE_ROOT='+shlex.quote(str(ROOT))+'\n'+
        'export CLBB_MANAGED_INSTALL=0\nexec "$CLBB_ROOT/bin/clbb-launcher" "$@"\n')
    wrapper.chmod(0o755)
    data=Path(os.environ.get('XDG_DATA_HOME')or Path.home()/'.local/share')
    desktop=data/'applications/org.clbb.CLBB.desktop';desktop.parent.mkdir(parents=True,exist_ok=True)
    backup=ROOT/'logs/desktop-before-workbench.desktop'
    if desktop.exists()and not backup.exists():backup.parent.mkdir(exist_ok=True);shutil.copy2(desktop,backup)
    exe=str(wrapper).replace('\\','\\\\').replace('"','\\"').replace('$','\\$').replace('`','\\`').replace('%','%%')
    desktop.write_text('[Desktop Entry]\nType=Application\nName=CLBB\nComment=Computer Legends Bang Bang\nExec="'+exe+'"\nTerminal=false\nIcon=applications-games\nCategories=Game;\nStartupNotify=false\n')
    desktop.chmod(0o644)
    desktop_icon=Path.home()/'Desktop/CLBB.desktop'
    if desktop_icon.is_file()and str(ROOT)in desktop_icon.read_text():
        saved=ROOT/'logs/desktop-icon-before-workbench.desktop'
        if not saved.exists():shutil.copy2(desktop_icon,saved)
        desktop_icon.write_text(desktop.read_text())
    for tool in ['kbuildsycoca6','kbuildsycoca5']:
        if shutil.which(tool):
            try:subprocess.run([tool,'--noincremental'],capture_output=True,timeout=30)
            except (OSError,subprocess.TimeoutExpired):pass
            break
    write(ROOT/'.clbb-local-runtime.json',{'directory':str(directory),'version':record['version'],
        'source_commit':record.get('source_commit')or record.get('published_source_commit'),'state':str(ROOT),'runtime_sha256':record['assets'][record['runtime']]})
    print('Local shortcut switched; account/game files untouched. Already running games need restart.')

def accept(directory,note):
    directory,record,_=verify(directory)
    if not note.strip():raise ValueError('Describe actual observed game checks')
    write(directory/'tested.json',{'version':record['version'],'source_commit':record.get('source_commit'),'assets':record['assets'],'observed':note})

def publish(directory,notes):
    directory,record,_=verify(directory)
    tested=json.loads((directory/'tested.json').read_text())
    if tested['assets']!=record['assets']or tested['source_commit']!=record.get('source_commit'):raise ValueError('Tested files differ; retest the candidate')
    if not record.get('source_commit'):raise ValueError('Legacy releases cannot be republished through this workflow')
    tag=record['version'];revision=record['source_commit']
    existing=subprocess.run(['git','rev-parse','--verify','refs/tags/'+tag+'^{commit}'],cwd=ROOT,capture_output=True,text=True)
    if existing.returncode==0 and existing.stdout.strip()!=revision:raise ValueError('Version tag already points to another source commit; bump VERSION')
    if existing.returncode:run('git','tag',tag,revision)
    run('git','push','origin',tag)
    # Resume interrupted draft uploads; published releases are never overwritten.
    previous=subprocess.run(['gh','release','view',tag,'--repo',REPO,'--json','assets,isDraft'],cwd=ROOT,capture_output=True,text=True)
    if previous.returncode==0:
        draft=json.loads(previous.stdout)
        if not draft['isDraft']:raise ValueError('Release is already published; choose a new version')
        uploaded={asset['name']:asset for asset in draft['assets']}
        if set(uploaded)-set(record['assets']):raise ValueError('Draft contains unexpected assets')
        pending=[]
        for name,digest in record['assets'].items():
            asset=uploaded.get(name)
            if asset and asset['state']=='uploaded':
                if asset.get('digest')!='sha256:'+digest:raise ValueError('Draft asset belongs to another candidate: '+name)
            else:pending.append(str(directory/name))
        if pending:run('gh','release','upload',tag,*pending,'--repo',REPO,'--clobber')
    else:
        run('gh','release','create',tag,*[str(directory/name)for name in record['assets']],
            '--repo',REPO,'--verify-tag','--draft','--prerelease','--title','CLBB '+tag,'--notes-file',str(Path(notes).resolve()))
    remote=json.loads(subprocess.check_output(['gh','release','view',tag,'--repo',REPO,'--json','assets'],cwd=ROOT,text=True))
    actual={a['name']:a for a in remote['assets']}
    for name,digest in record['assets'].items():
        if name not in actual or actual[name]['state']!='uploaded'or actual[name].get('digest')!='sha256:'+digest:raise ValueError('Remote asset verification failed; release remains draft')
    checks=json.loads(subprocess.check_output(['gh','run','list','--repo',REPO,'--commit',revision,'--workflow','checks.yml','--json','status,conclusion'],cwd=ROOT,text=True))
    if not any(check['status']=='completed'and check['conclusion']=='success'for check in checks):raise ValueError('GitHub source checks not yet green; release remains draft')
    run('gh','release','edit',tag,'--repo',REPO,'--draft=false','--prerelease')

def status():
    local=ROOT/'.clbb-local-runtime.json'
    print(json.dumps({'checkout':git('rev-parse','HEAD'),'version_file':(ROOT/'VERSION').read_text().strip(),
        'local_runtime':json.loads(local.read_text())if local.exists()else None},indent=2,ensure_ascii=False))

if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);sub=p.add_subparsers(dest='action',required=True)
    sub.add_parser('status')
    for name in ['prepare','verify','use-local','accept','publish','import-release']:
        command=sub.add_parser(name);command.add_argument('directory')
        if name=='import-release':command.add_argument('--tag',required=True)
        if name=='accept':command.add_argument('--note',required=True)
        if name=='publish':command.add_argument('--notes',type=Path,required=True)
    a=p.parse_args()
    try:
        with (ROOT/'.release-workflow.lock').open('a+b')as lock:
            fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
            if a.action=='status':status()
            elif a.action=='import-release':import_release(a.directory,a.tag)
            elif a.action=='prepare':prepare(a.directory)
            elif a.action=='verify':verify(a.directory)
            elif a.action=='use-local':use_local(a.directory)
            elif a.action=='accept':accept(a.directory,a.note)
            elif a.action=='publish':publish(a.directory,a.notes)
    except (OSError,ValueError,KeyError,subprocess.CalledProcessError)as error:
        raise SystemExit('CLBB release: '+str(error))
