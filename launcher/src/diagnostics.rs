// SPDX-License-Identifier: MIT
//! Bounded, local technical reports. Sending requires an explicit user action.
use serde_json::{json,Value};
use std::{env,fs,io::{self,Read,Seek,SeekFrom},path::{Path,PathBuf},process::Command,time::{SystemTime,UNIX_EPOCH}};
use crate::launch_scope::Evidence;
const ENDPOINT:&str="https://clbb-downloads.velikiievgenius.com/reports";
fn command(program:&str,args:&[&str])->String {
    Command::new(program).args(args).env_remove("LD_PRELOAD").env_remove("LD_LIBRARY_PATH").output().map(|v|String::from_utf8_lossy(&v.stdout).chars().take(12000).collect()).unwrap_or_else(|e|format!("unavailable: {e}"))
}
fn private_path(path:&Path)->String {
    let text=path.to_string_lossy();if let Some(home)=env::var_os("HOME"){text.replace(&home.to_string_lossy().to_string(),"~")}else{text.into_owned()}
}
fn tail(path:&Path)->String {
    let Ok(mut file)=fs::File::open(path)else{return String::new()};let size=file.metadata().map(|v|v.len()).unwrap_or(0);let _=file.seek(SeekFrom::Start(size.saturating_sub(65536)));let mut text=String::new();let _=file.read_to_string(&mut text);text
}
fn sanitize(text:&str)->String {
    let home=env::var("HOME").unwrap_or_default();let mut output=String::new();
    for line in text.lines(){let lower=line.to_lowercase();
        if ["password","passwd","cookie","token","authorization","session","email","moonton","usersession"].iter().any(|v|lower.contains(v))||line.contains('@')||line.contains("http://")||line.contains("https://"){continue;}
        let line=if home.is_empty(){line.to_owned()}else{line.replace(&home,"~")};
        output.extend(line.chars().take(1000));output.push('\n');if output.len()>24000{break;}
    }output
}
pub fn summarize(reason:&str,errors:&str)->String {
    let text=format!("{reason}\n{errors}").to_lowercase();
    let exit=reason.to_lowercase();
    if exit.contains("sigkill")||exit.contains("signal: 9 (") {
        return "Процесс CLBB принудительно завершён (SIGKILL). Сам по себе этот сигнал не указывает на ошибку OpenGL.".into();
    }
    if exit.contains("sigill")||exit.contains("signal: 4 (") {
        return "Нативное падение процесса по сигналу SIGILL. Причину нужно определить по точному журналу и стеку этого запуска; сам сигнал не доказывает несовместимость CPU.".into();
    }
    if text.contains("not found")&&(text.contains("glibc_")||text.contains("glibcxx_")){return "Несовместимая версия системной библиотеки glibc/libstdc++. Обновите дистрибутив; точное имя есть в отчёте.".into();}
    if text.contains("error while loading shared libraries")||text.contains("=> not found"){return "Не найдена необходимая системная библиотека. Отчёт содержит проверку зависимостей.".into();}
    if text.contains("illegal instruction")||text.contains("sigill")||exit.contains("signal: 4 ("){return "Нативное падение процесса по сигналу SIGILL. Причину нужно определить по точному журналу и стеку этого запуска; сам сигнал не доказывает несовместимость CPU.".into();}
    if text.contains("segmentation")||text.contains("sigsegv"){return "Нативное падение процесса (SIGSEGV). Отправьте технический отчёт для разбора.".into();}
    if text.contains("display")&&(text.contains("required")||text.contains("cannot open")){return "Нет доступного X11/XWayland-дисплея. Запускайте CLBB из графической сессии.".into();}
    if text.contains("egl")||text.contains("opengl")||text.contains("libgl error"){return "Ошибка графического контекста или драйвера OpenGL/EGL. Подробности сохранены в отчёте.".into();}
    format!("CLBB завершился с ошибкой: {}",reason.chars().take(300).collect::<String>())
}
pub fn report(root:&Path,state:&Path,reason:&str,kind:&str,evidence:&Evidence)->io::Result<PathBuf>{
    let mut errors=String::new();
    if let Some(context)=&evidence.context{for line in tail(&context.run.join("runtime.log")).lines(){let lower=line.to_lowercase();if ["error","fatal","exception","not found","failed","signal","assert","traceback","segmentation"].iter().any(|key|lower.contains(key)){errors.push_str(line);errors.push('\n');}}}
    let errors=sanitize(&errors);let mut checks=Vec::new();
    let checked_root=evidence.context.as_ref().and_then(|c|c.record["runtime_root"].as_str()).map(Path::new).unwrap_or(root);
    for name in ["prefix/bin/android-translation-layer","prefix/bin/dalvikvm","prefix/lib/libnb-qemu.so","prefix/lib/libclbb_mapper.so"]{
        let mut cmd=Command::new("ldd");cmd.arg(checked_root.join(name)).env_remove("LD_PRELOAD").env("LD_LIBRARY_PATH",format!("{}/prefix/lib:{}/prefix/lib/art:{}/prefix/lib/java/dex/art/natives",checked_root.display(),checked_root.display(),checked_root.display())).env("LC_ALL","C");
        let value=cmd.output().map(|v|format!("{}{}",String::from_utf8_lossy(&v.stdout),String::from_utf8_lossy(&v.stderr))).unwrap_or_else(|e|e.to_string());
        checks.push(json!({"file":name,"result":sanitize(&value)}));
    }
    let cpu=fs::read_to_string("/proc/cpuinfo").unwrap_or_default();let cpu:Vec<_>=cpu.lines().filter(|line|line.starts_with("model name")||line.starts_with("flags")).take(2).collect();
    let manifest=evidence.context.as_ref().map(|c|&c.record);
    let version=manifest.and_then(|m|m["runtime_version"].as_str()).unwrap_or("unknown");
    let revision=manifest.and_then(|m|m["runtime_revision"].as_str()).unwrap_or("");
    let reason=sanitize(reason);let summary=summarize(&reason,&format!("{}\n{}",errors,checks.iter().map(|v|v["result"].as_str().unwrap_or("")).collect::<Vec<_>>().join("\n")));
    let mut provenance=evidence.provenance.clone();
    for key in ["identity","expected"] {for field in ["runtime_root","launch_directory"] {
        if let Some(path)=provenance[key][field].as_str(){let value=private_path(Path::new(path));provenance[key][field]=json!(value);}
    }}
    let value=json!({"schema":"clbb-diagnostic-v1","app_version":version.trim(),"app_revision":revision,"kind":kind,"reason":reason.trim(),"summary":summary,"launch_provenance":provenance,
        "diagnostic_invocation_id":crate::launch_scope::invocation_id(),"last_exit":evidence.context.as_ref().map(crate::launch_scope::exit_record),
        "diagnostic_runtime":{"root":private_path(root),"version":fs::read_to_string(root.join("VERSION")).unwrap_or_default().trim(),"revision":fs::read_to_string(root.join("REVISION")).unwrap_or_default().trim()},
        "host":{"kernel":command("uname",&["-srmo"]),"glibc":command("getconf",&["GNU_LIBC_VERSION"]),"os":fs::read_to_string("/etc/os-release").unwrap_or_default(),"cpu":cpu,"display_available":env::var_os("DISPLAY").is_some(),"session_type":env::var("XDG_SESSION_TYPE").unwrap_or_default()},
        "checks":checks,"checks_scope":"Current files of the verified launch runtime, or diagnostic runtime when launch identity is unknown; not crash-time binary contents.","startup_errors":errors,"last_launch":evidence.context.as_ref().map(|c|private_path(&c.run))});
    let dir=state.join("logs/reports");fs::create_dir_all(&dir)?;
    #[cfg(unix)]{use std::os::unix::fs::PermissionsExt;fs::set_permissions(&dir,fs::Permissions::from_mode(0o700))?;}
    let id=SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();let path=dir.join(format!("report-{id}.json"));let bytes=serde_json::to_vec_pretty(&value)?;
    fs::write(&path,&bytes)?;
    #[cfg(unix)]{use std::os::unix::fs::PermissionsExt;fs::set_permissions(&path,fs::Permissions::from_mode(0o600))?;}
    fs::write(state.join("logs/last-diagnostic.json"),&bytes)?;
    #[cfg(unix)]{use std::os::unix::fs::PermissionsExt;fs::set_permissions(state.join("logs/last-diagnostic.json"),fs::Permissions::from_mode(0o600))?;}
    eprintln!("[CLBB] {}\nОтчёт / Report: {}",value["summary"].as_str().unwrap_or(""),path.display());Ok(path)
}
pub fn send(path:&Path)->io::Result<String>{
    let bytes=fs::read(path)?;if bytes.len()>512*1024{return Err(io::Error::other("Report exceeds 512 KiB"));}
    let value:Value=serde_json::from_slice(&bytes)?;if value["schema"]!="clbb-diagnostic-v1"{return Err(io::Error::other("Not a CLBB technical report"));}
    let result=Command::new("curl").args(["--fail","--silent","--show-error","--proto","=https","--connect-timeout","15","--max-time","60","--request","POST","--header","Content-Type: application/json","--data-binary"]).arg(format!("@{}",path.display())).arg(ENDPOINT).env_remove("LD_PRELOAD").env_remove("LD_LIBRARY_PATH").output()?;
    if !result.status.success(){return Err(io::Error::other(format!("Отправка не удалась ({}). Локальный отчёт сохранён.",result.status)));}
    let response:Value=serde_json::from_slice(&result.stdout)?;let id=response["id"].as_str().ok_or_else(||io::Error::other("Invalid server response"))?;
    if id.len()>100||!id.bytes().all(|c|c.is_ascii_alphanumeric()||c==b'-'){return Err(io::Error::other("Invalid report identifier"));}Ok(id.into())
}
pub fn show(path:&Path)->io::Result<()> {
    let data:Value=serde_json::from_slice(&fs::read(path)?)?;
    if let Some(expected)=crate::launch_scope::invocation_id(){
        if data["diagnostic_invocation_id"].as_str()!=Some(expected.as_str()){
            return Err(io::Error::other("Diagnostic report belongs to a different wrapper invocation"));
        }
    }
    if env::var_os("DISPLAY").is_none()&&env::var_os("WAYLAND_DISPLAY").is_none(){eprintln!("Отправка / Send: clbb --send-report {}",path.display());return Ok(());}
    let text=format!("{}\n\nОтчёт: {}\n\nОтправить технический отчёт на StarFive? Пароли, cookies и файлы аккаунта в него не включаются.",data["summary"].as_str().unwrap_or("Ошибка запуска CLBB"),path.display());
    let choice=Command::new("zenity").args(["--question","--no-markup","--width=620","--title=CLBB — Ошибка запуска","--ok-label=Отправить отчёт на сервер","--cancel-label=Закрыть","--text"]).arg(text).status()?;
    if choice.success(){match send(path){Ok(id)=>{eprintln!("[CLBB] Report sent: {id}");let _=Command::new("zenity").args(["--info","--no-markup","--title=CLBB — Отчёт отправлен","--text"]).arg(format!("Отчёт отправлен на StarFive.\nНомер: {id}")).status();},Err(e)=>{let _=Command::new("zenity").args(["--error","--no-markup","--title=CLBB — Ошибка отправки","--text"]).arg(e.to_string()).status();}}}Ok(())
}
#[cfg(test)]mod tests{use super::*;#[test]fn strips_sensitive_lines(){let text=sanitize("fatal SIGSEGV\nCookie: abc\npassword=secret\nuser@example.com\nhttps://example.com/?key=x\nlibfoo.so => not found");assert!(text.contains("SIGSEGV")&&text.contains("libfoo"));assert!(!text.contains("secret")&&!text.contains("example.com")&&!text.contains("abc"));}#[test]fn classifies_loader_and_signals_without_guessing_cpu_cause(){assert!(summarize("exit127","error while loading shared libraries: libfoo.so").contains("библиотека"));let ill=summarize("signal: 4 (SIGILL)","");assert!(ill.contains("SIGILL")&&ill.contains("не доказывает"));assert!(!ill.contains("неподдерживаемой"));assert!(summarize("signal: 9 (SIGKILL)","EGL error").contains("SIGKILL"));assert!(summarize("SIGSEGV","").contains("SIGSEGV"));}}

#[cfg(test)]
mod termination_tests {
    #[test]
    fn killed_process_is_not_misreported_as_a_graphics_failure() {
        let summary=super::summarize("CLBB runtime exited: signal: 9 (SIGKILL)","EGL failed during cleanup");
        assert!(summary.contains("SIGKILL"));
        assert!(!summary.starts_with("Ошибка графического"));
        assert!(super::summarize("runtime failed","libGL error: cannot create context").starts_with("Ошибка графического"));
    }
}
