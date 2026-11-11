// SPDX-License-Identifier: GPL-3.0-or-later
//! Versioned user installation. Platform-specific activation is kept separate
//! from the GitHub release protocol so future Windows support can replace it.
mod progress;
use serde_json::{json, Value};
use std::{env, fs, io, path::{Path, PathBuf}, process::{Command, Stdio}};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const REPO: &str = "velikiievgeniusultimate/CLBB";
const API: &str = "https://api.github.com/repos/velikiievgeniusultimate/CLBB/releases?per_page=20";
const ASSET: &str = "clbb-manager-linux-x86_64";
fn fail(message: &str) -> Box<dyn std::error::Error> { io::Error::other(message).into() }
fn base() -> PathBuf {
    env::var_os("CLBB_INSTALL_ROOT").map(PathBuf::from).unwrap_or_else(||
        env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".local/share")).join("clbb"))
}
fn fetch(url: &str, output: Option<&Path>) -> Result<Vec<u8>> {
    let mut command=Command::new("curl");
    command.args(["--fail", "--location", "--show-error", "--proto", "=https", "--proto-redir", "=https", "--connect-timeout", "15", "--max-time", if output.is_some(){"1800"}else{"60"}, "--retry", "2", "--user-agent", "CLBB-Manager/0.1"]);
    if let Some(path)=output {
        eprintln!("[CLBB ↓] Download / Загрузка: {}",path.file_name().unwrap_or_default().to_string_lossy());
        command.args(["--progress-bar","--write-out","\n[CLBB download] %{size_download} bytes · %{speed_download} B/s · %{time_total}s\n"]).stderr(Stdio::inherit()).arg("--output").arg(path);
    } else {command.arg("--silent");}
    command.arg(url);
    let result=if output.is_some(){command.output()?}else{progress::run("Получение метаданных / Fetching metadata",||command.output())?};
    if !result.status.success(){return Err(fail(&format!("Download failed ({}): {}",result.status,String::from_utf8_lossy(&result.stderr))));}
    if output.is_some(){eprint!("{}",String::from_utf8_lossy(&result.stdout));return Ok(Vec::new());}
    Ok(result.stdout)
}
fn version(tag:&str)->Option<(u32,u32,u32,bool,u32)> {
    let tag=tag.strip_prefix('v')?;let (number,pre)=tag.split_once('-').unwrap_or((tag,""));
    let n:Vec<_>=number.split('.').collect();if n.len()!=3{return None;}
    let seq=if pre.is_empty(){0}else{pre.strip_prefix("alpha.")?.parse().ok()?};
    Some((n[0].parse().ok()?,n[1].parse().ok()?,n[2].parse().ok()?,pre.is_empty(),seq))
}
fn release(raw:&[u8])->Result<Value> {
    let rows:Vec<Value>=serde_json::from_slice(raw)?;
    rows.into_iter().filter(|v| !v["draft"].as_bool().unwrap_or(true)&&v["tag_name"].as_str().and_then(version).is_some())
        .max_by_key(|v| version(v["tag_name"].as_str().unwrap())).ok_or_else(||fail("No published CLBB release / Нет опубликованной версии CLBB"))
}
fn asset(release:&Value,name:&str)->Result<String> {
    let tag=release["tag_name"].as_str().ok_or("Missing release tag")?;
    let prefix=format!("https://github.com/{REPO}/releases/download/{tag}/");
    let url=release["assets"].as_array().ok_or("Missing release assets")?.iter().find(|v|v["name"]==name)
        .and_then(|v|v["browser_download_url"].as_str()).ok_or_else(||fail(&format!("Missing release asset: {name}")))?;
    if url!=format!("{prefix}{name}"){return Err(fail("Unexpected release asset origin"));}Ok(url.into())
}
fn checksum(sums:&str,name:&str)->Result<String> {
    let mut found=Vec::new();
    for line in sums.lines(){let fields:Vec<_>=line.split_whitespace().collect();if fields.len()==2&&fields[1].trim_start_matches('*')==name{found.push(fields[0]);}}
    if found.len()!=1||found[0].len()!=64||!found[0].bytes().all(|c|c.is_ascii_hexdigit()){return Err(fail("Missing or ambiguous SHA256"));}
    Ok(found[0].to_lowercase())
}
fn verify_inner(path:&Path,expected:&str)->Result<()> {
    let output=Command::new("sha256sum").arg("--").arg(path).output()?;
    let actual=String::from_utf8(output.stdout)?.split_whitespace().next().unwrap_or("").to_owned();
    if !output.status.success()||actual!=expected{return Err(fail("SHA256 mismatch / Контрольная сумма не совпадает"));}Ok(())
}
fn verify(path:&Path,expected:&str)->Result<()> {
    progress::run("Проверка SHA256 / Verifying SHA256",||verify_inner(path,expected))
}
fn installed(root:&Path)->String {fs::read_to_string(root.join("current/VERSION")).unwrap_or_default().trim().to_owned()}
fn supported()->Result<()> {
    if !cfg!(all(target_os="linux",target_arch="x86_64")){return Err(fail("This release supports Linux x86_64 only"));}
    let status=Command::new("pacman").arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status()?;
    if !status.success(){return Err(fail("Arch/Manjaro/EndeavourOS required for this release"));}Ok(())
}
fn dependencies(info:&Value,install:bool)->Result<()> {
    let list=info["arch_dependencies"].as_array().ok_or("Missing runtime dependency manifest")?;
    let mut deps=Vec::new();
    for value in list {let s=value.as_str().ok_or("Invalid dependency")?;
        if s.starts_with('-')||s.is_empty()||!s.bytes().all(|c|c.is_ascii_alphanumeric()||b"@._+:-<>=~".contains(&c)){return Err(fail("Invalid dependency name"));}deps.push(s);
    }
    let missing=Command::new("pacman").arg("-T").args(&deps).output()?;
    if missing.status.success(){return Ok(());}
    let names=String::from_utf8(missing.stdout)?;
    if !install{return Err(fail(&format!("Runtime dependencies required. Run the installer in a terminal / Запустите установщик в терминале: {}",names.trim())));}
    let mut command=Command::new("sudo");command.args(["pacman","-S","--needed","--"]);command.args(&deps).stdout(Stdio::inherit()).stderr(Stdio::inherit()).stdin(Stdio::from(fs::File::open("/dev/tty")?));
    if !command.status()?.success(){return Err(fail("Dependency installation failed"));}
    if !Command::new("pacman").arg("-T").args(&deps).output()?.status.success(){return Err(fail("Required package versions are unavailable; update the distribution first"));}Ok(())
}
fn extract(archive:&Path,directory:&Path)->Result<()> {
    let result=Command::new("python3").arg("-c").arg(include_str!("../../extract.py")).arg(archive).arg(directory).output()?;
    if !result.status.success(){return Err(fail(&String::from_utf8_lossy(&result.stderr)));}Ok(())
}
struct Guard(PathBuf);
impl Drop for Guard {fn drop(&mut self){let _=fs::remove_dir_all(&self.0);}}
fn write_atomic(path:&Path,bytes:&[u8])->Result<()> {
    fs::create_dir_all(path.parent().ok_or("Invalid destination")?)?;
    let temp=path.with_extension(format!("{}.tmp",std::process::id()));fs::write(&temp,bytes)?;fs::rename(temp,path)?;Ok(())
}
#[cfg(target_os="linux")]
fn activate(root:&Path,tag:&str)->Result<()> {
    let link=root.join(format!(".current-{}",std::process::id()));
    std::os::unix::fs::symlink(Path::new("releases").join(tag),&link)?;
    if let Err(e)=fs::rename(&link,root.join("current")){let _=fs::remove_file(link);return Err(e.into());}Ok(())
}
#[cfg(not(target_os="linux"))]
fn activate(_root:&Path,_tag:&str)->Result<()> {Err(fail("Installation backend not implemented on this OS"))}
#[cfg(target_os="linux")]
fn entrypoints(root:&Path)->Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let home=PathBuf::from(env::var_os("HOME").ok_or("HOME is not set")?);
    let bin=home.join(".local/bin");fs::create_dir_all(&bin)?;
    // Embed the selected installation root as a shell-quoted literal, never eval.
    let quoted=format!("'{}'",root.to_string_lossy().replace('\'',"'\\''"));
    let script=format!(r#"#!/bin/sh
set -eu
umask 077
CLBB_BASE={quoted}
export CLBB_STATE_ROOT="$CLBB_BASE/state"
export CLBB_INSTALL_ROOT="$CLBB_BASE"
export CLBB_MANAGED_INSTALL=1
CLBB_ROOT=$(readlink -f -- "$CLBB_BASE/current")
export CLBB_ROOT
if [ "$#" -gt 0 ]; then exec "$CLBB_ROOT/bin/clbb-launcher" "$@"; fi
mkdir -p "$CLBB_STATE_ROOT/logs"
rm -f -- "$CLBB_STATE_ROOT/logs/last-diagnostic.json"
export CLBB_WRAPPED=1
set +e
"$CLBB_ROOT/bin/clbb-launcher" 2>"$CLBB_STATE_ROOT/logs/launcher-stderr.log"
CLBB_EXIT=$?
set -e
if [ "$CLBB_EXIT" -ne 0 ]; then
    "$CLBB_ROOT/bin/clbb-launcher" --show-last-error 2>>"$CLBB_STATE_ROOT/logs/launcher-stderr.log" || \
        python3 "$CLBB_ROOT/distribution/launch-fallback.py" "$CLBB_ROOT" "$CLBB_STATE_ROOT" "Ошибка запуска CLBB (код $CLBB_EXIT)"
fi
exit "$CLBB_EXIT"
"#);

    write_atomic(&bin.join("clbb"),script.as_bytes())?;fs::set_permissions(bin.join("clbb"),fs::Permissions::from_mode(0o755))?;
    let data=env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(||home.join(".local/share"));
    let exe=bin.join("clbb").to_string_lossy().replace('\\',"\\\\").replace('"',"\\\"").replace('`',"\\`").replace('$',"\\$").replace('%',"%%");
    let desktop=format!("[Desktop Entry]\nType=Application\nName=CLBB\nComment=Computer Legends Bang Bang\nComment[ru]=Computer Legends Bang Bang — Linux-клиент\nExec=\"{exe}\"\nTerminal=false\nCategories=Game;\nIcon=applications-games\nStartupNotify=true\n");
    write_atomic(&data.join("applications/org.clbb.CLBB.desktop"),desktop.as_bytes())?;Ok(())
}
#[cfg(not(target_os="linux"))]
fn entrypoints(_root:&Path)->Result<()>{Err(fail("Entrypoints not implemented on this OS"))}
#[cfg(target_os="linux")]
fn update_lock(root:&Path)->Result<fs::File> {
    use std::os::fd::AsRawFd;
    unsafe extern "C" {fn flock(fd:i32,operation:i32)->i32;}
    let file=fs::OpenOptions::new().create(true).truncate(false).read(true).write(true).open(root.join(".update.lock"))?;
    // Kernel-owned lock is released even on hard process termination.
    if unsafe {flock(file.as_raw_fd(),2|4)}!=0{return Err(fail("Another CLBB update is active"));}Ok(file)
}
#[cfg(not(target_os="linux"))]
fn update_lock(_root:&Path)->Result<fs::File>{Err(fail("Update locking backend unavailable"))}
fn verify_payload(root:&Path,manifest:&Path)->Result<()> {
    let output=Command::new("python3").arg("-c").arg(include_str!("../../verify.py")).arg(root).arg(manifest).output()?;
    if !output.status.success(){return Err(fail(&String::from_utf8_lossy(&output.stderr)));}Ok(())
}
fn install(rel:&Value,root:&Path,allow_deps:bool)->Result<Value> {
    supported()?;let tag=rel["tag_name"].as_str().ok_or("Missing release version")?;
    let current=installed(root);
    if version(&current).is_some_and(|v|v>=version(tag).unwrap()){return Ok(json!({"state":"current","version":current}));}
    fs::create_dir_all(root)?;
    let _lock=update_lock(root)?;
    let work=root.join(format!(".download-{}",std::process::id()));fs::create_dir(&work)?;let _work=Guard(work.clone());
    let sums=String::from_utf8(fetch(&asset(rel,"SHA256SUMS")?,None)?)?;
    let name=format!("clbb-{tag}-linux-x86_64.tar.gz");let archive=work.join(&name);
    fetch(&asset(rel,&name)?,Some(&archive))?;verify(&archive,&checksum(&sums,&name)?)?;
    let staged=work.join("runtime");fs::create_dir(&staged)?;progress::run("Распаковка CLBB / Extracting runtime",||extract(&archive,&staged))?;
    let info:Value=serde_json::from_slice(&fs::read(staged.join("release.json"))?)?;
    if info["schema"]!=1||info["version"]!=tag||info["platform"]!="linux-x86_64"||fs::read_to_string(staged.join("VERSION"))?.trim()!=tag{return Err(fail("Release payload metadata does not match"));}
    // The source companion is mandatory; a code-only release is not a runtime installer.
    asset(rel,&format!("clbb-{tag}-sources.tar.gz"))?;
    progress::run("Проверка файлов CLBB / Verifying runtime files",||verify_payload(&staged,&staged.join("release.json")))?;
    eprintln!("[CLBB …] Проверка и установка системных пакетов / System dependencies");
    dependencies(&info,allow_deps)?;
    eprintln!("[CLBB OK] Системные пакеты / System dependencies");
    for required in ["bin/clbb-launcher","bin/clbb-manager","run-mlbb.sh","prefix/bin/android-translation-layer"]{if !staged.join(required).is_file(){return Err(fail("Incomplete runtime payload"));}}
    // Check that the declared minimum libc is satisfied before activation.
    let result=Command::new("getconf").arg("GNU_LIBC_VERSION").output()?;
    let text=String::from_utf8(result.stdout)?;
    let libc=|s:&str|->Option<(u32,u32)>{let mut v=s.split('.');Some((v.next()?.parse().ok()?,v.next()?.parse().ok()?))};
    let minimum=info["min_glibc"].as_str().and_then(libc).ok_or("Missing minimum libc")?;
    if text.split_whitespace().nth(1).and_then(libc).is_none_or(|v|v<minimum){return Err(fail("System glibc is too old for this release"));}
    let libraries=progress::run("Проверка ABI и библиотек / Checking runtime libraries",||Command::new("python3").arg("-c").arg(include_str!("../../check_libraries.py")).arg(&staged).output())?;
    if !libraries.status.success(){return Err(fail(&String::from_utf8_lossy(&libraries.stderr)));}
    fs::create_dir_all(root.join("releases"))?;let destination=root.join("releases").join(tag);
    // Never overwrite an existing release directory, especially one in use.
    if destination.exists(){verify_payload(&destination,&staged.join("release.json"))?;}
    else{fs::rename(staged,&destination)?;}
    eprintln!("[CLBB …] Создание ярлыка и активация версии / Activating installation");
    entrypoints(root)?;activate(root,tag)?;
    eprintln!("[CLBB OK] Версия активирована / Installation activated");
    Ok(json!({"state":"installed","version":tag,"restart_required":true,"installation":root}))
}
fn import_game(root:&Path,paths:&[String],dialog:bool)->Result<Value> {
    let release=root.join("current");let script=release.join("distribution/import_game.py");
    let state=env::var_os("CLBB_STATE_ROOT").map(PathBuf::from).unwrap_or_else(||root.join("state"));
    let mut files=paths.to_vec();
    if dialog {
        let output=Command::new("zenity").args(["--file-selection","--multiple","--separator=\n","--title=CLBB — Выберите APK Mobile Legends / Select game APK files","--file-filter=APK | *.apk"]).output()?;
        if !output.status.success(){return Err(fail("Game import cancelled / Выбор игры отменён"));}
        files=String::from_utf8(output.stdout)?.lines().map(str::to_owned).collect();
    }
    if files.is_empty(){return Err(fail("Select base APK and any split APKs"));}
    let result=progress::run("Проверка и импорт APK / Validating and importing APK",||Command::new("python3").arg(script).arg(state).args(files).output())?;
    if !result.status.success(){return Err(fail(&String::from_utf8_lossy(&result.stderr)));}
    Ok(serde_json::from_slice(&result.stdout)?)
}
fn execute()->Result<Value> {
    let args:Vec<_>=env::args().skip(1).collect();let command=args.first().map(String::as_str).unwrap_or("--check");let root=base();
    if command=="--version"{return Ok(json!({"version":env!("CARGO_PKG_VERSION"),"repository":REPO}));}
    if command=="--import-game-url" {
        if args.len()!=3||!args[1].starts_with("https://")||args[2].len()!=64||!args[2].bytes().all(|c|c.is_ascii_hexdigit()){return Err(fail("Usage: --import-game-url HTTPS_URL SHA256"));}
        fs::create_dir_all(&root)?;let work=root.join(format!(".game-download-{}",std::process::id()));fs::create_dir(&work)?;let _guard=Guard(work.clone());
        let apk=work.join("game.apk");fetch(&args[1],Some(&apk))?;verify(&apk,&args[2].to_lowercase())?;
        return import_game(&root,&[apk.to_string_lossy().into_owned()],false);
    }
    if command=="--import-game"||command=="--import-game-dialog"{return import_game(&root,&args[1..],command.ends_with("-dialog"));}
    if !["--check","--install","--update"].contains(&command){return Err(fail("Usage: clbb-manager --check | --install | --update | --import-game APK..."));}
    let rel=release(&fetch(API,None)?)?;let current=installed(&root);let tag=rel["tag_name"].as_str().unwrap();
    if command=="--check"{return Ok(json!({"installed":current,"latest":tag,"available":version(&current).is_none_or(|v|v<version(tag).unwrap()),"release_url":rel["html_url"]}));}
    install(&rel,&root,command=="--install")
}
fn main(){match execute(){Ok(v)=>println!("{v}"),Err(e)=>{println!("{}",json!({"error":e.to_string()}));std::process::exit(1);}}}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]fn version_order_and_tag_path_safety(){assert!(version("v0.1.0-alpha.2")>version("v0.1.0-alpha.1"));assert!(version("v0.1.0")>version("v0.1.0-alpha.99"));for tag in ["../../tmp","v1.2.3/evil","1.2.3","v1.2","v1.2.3-beta.1"]{assert!(version(tag).is_none());}}
    #[test]fn reject_checksum_ambiguity(){let hash="a".repeat(64);assert_eq!(checksum(&format!("{hash}  runtime.tar.gz\n"),"runtime.tar.gz").unwrap(),hash);assert!(checksum(&format!("{hash}  x\n{hash}  x\n"),"x").is_err());assert!(checksum("bad x","x").is_err());}
    #[test]fn selects_highest_release_and_rejects_drafts(){let data=br#"[{"tag_name":"v0.1.0","draft":true},{"tag_name":"v0.1.0-alpha.2","draft":false},{"tag_name":"v0.1.0-alpha.1","draft":false}]"#;assert_eq!(release(data).unwrap()["tag_name"],"v0.1.0-alpha.2");}
    #[test]fn enforces_release_origin(){let rel=json!({"tag_name":"v0.1.0","assets":[{"name":ASSET,"browser_download_url":"https://example.org/evil"}]});assert!(asset(&rel,ASSET).is_err());}
}
