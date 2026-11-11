// SPDX-License-Identifier: GPL-3.0-or-later
//! User-triggered updates run outside the GTK/input thread. Activation affects
//! the next launch; running releases and persistent game data are untouched.
use std::{path::PathBuf, process::{Command,Stdio},io::Read, sync::{Arc,Mutex}};
#[derive(Default)]
struct State {busy:bool,available:bool,status:String,progress:Option<f64>,outcome:String,installed_version:String}
pub struct Updates {state:Arc<Mutex<State>>,manager:PathBuf,managed:bool,version:String}
impl Updates {
    pub fn new()->Self {
        let root=std::env::var_os("CLBB_ATL_ROOT").map(PathBuf::from);
        let mut manager=std::env::var_os("CLBB_UPDATE_MANAGER").map(PathBuf::from).unwrap_or_default();
        if !manager.is_file(){if let Some(root)=root{manager=root.join("distribution/manager/target/release/clbb-manager");}}
        let managed=std::env::var("CLBB_MANAGED_INSTALL").as_deref()==Ok("1");
        let mut version=std::env::var("CLBB_RELEASE_VERSION").unwrap_or_else(|_|"dev".into());
        if let Ok(revision)=std::env::var("CLBB_RELEASE_REVISION") {
            if revision.len()==40 && revision.bytes().all(|c|c.is_ascii_hexdigit()) { version.push_str(&format!(" · {}",&revision[..12])); }
        }
        Self {state:Arc::new(Mutex::new(State{status:if managed{"Нажмите «Проверить обновления»."}else{"Версия из исходников. Автообновление доступно после установки CLBB установщиком."}.into(),..Default::default()})),manager,managed,version}
    }
    pub fn read(&self,key:&str)->String {
        let state=self.state.lock().unwrap_or_else(|e|e.into_inner());
        match key {"updates.installed_version"=>state.installed_version.clone(),"updates.outcome"=>state.outcome.clone(),"updates.progress"=>state.progress.unwrap_or(-1.0).to_string(),"updates.version"=>self.version.clone(),"updates.status"=>state.status.clone(),"updates.busy"=>state.busy.to_string(),"updates.available"=>(self.managed&&state.available&&!state.busy).to_string(),_=>String::new()}
    }
    pub fn request(&self,key:&str)->bool {
        if !matches!(key,"updates.check"|"updates.install"){return false;}
        let install=key=="updates.install";let mut state=self.state.lock().unwrap_or_else(|e|e.into_inner());
        if state.busy||install&&!self.managed{return false;}
        if !self.manager.is_file(){state.status="Менеджер обновлений не найден. Запустите установщик CLBB.".into();return false;}
        state.busy=true;state.progress=None;state.outcome.clear();state.status=if install{"Загружается обновление…"}else{"Проверяем новые версии…"}.into();drop(state);
        let shared=self.state.clone();let manager=self.manager.clone();
        let spawn=std::thread::Builder::new().name("clbb-updater".into()).spawn(move||{
            let result=(||->std::io::Result<std::process::Output>{
                let mut child=Command::new(manager).arg(if install{"--update"}else{"--check"}).env_remove("LD_PRELOAD").env_remove("LD_LIBRARY_PATH").stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
                let mut stream=child.stderr.take().unwrap();let progress=shared.clone();
                let reader=std::thread::spawn(move||{
                    let mut buffer=[0u8;1024];let mut line=Vec::new();
                    while let Ok(count)=stream.read(&mut buffer){if count==0{break;}for byte in &buffer[..count]{
                        if *byte==b'\r'||*byte==b'\n'{
                            let text=String::from_utf8_lossy(&line);let text=text.trim();
                            if text.starts_with("[CLBB") {let mut state=progress.lock().unwrap_or_else(|e|e.into_inner());state.status=text.chars().take(400).collect();state.progress=None;}
                            else if let Some(number)=text.split_whitespace().last().and_then(|s|s.strip_suffix('%')).and_then(|s|s.parse::<f64>().ok()){
                                let mut state=progress.lock().unwrap_or_else(|e|e.into_inner());state.progress=Some(number.clamp(0.0,100.0));
                            }
                            line.clear();
                        }else if line.len()<2048{line.push(*byte);}
                    }}
                });
                let result=child.wait_with_output();let _=reader.join();result
            })();
            let mut state=shared.lock().unwrap_or_else(|e|e.into_inner());state.busy=false;state.progress=None;
            match result {
                Ok(output)=>{
                    let value=serde_json::from_slice::<serde_json::Value>(&output.stdout);
                    match value {
                        Ok(v) if output.status.success()=>{
                            if install {state.installed_version=v["version"].as_str().unwrap_or("").into();}
                            state.outcome=if install {if v["state"]=="installed"{"installed"}else{"current"}} else if v["available"].as_bool().unwrap_or(false){"available"}else{"current"}.into();
                            if install {state.available=false;state.status=if v["state"]=="installed"{format!("Версия {} установлена. Она включится при следующем запуске CLBB.",v["version"].as_str().unwrap_or(""))}else{"Установлена актуальная версия.".into()};}
                            else{state.available=v["available"].as_bool().unwrap_or(false);state.status=if state.available {format!("Доступна версия {}.",v["latest"].as_str().unwrap_or(""))}else{"Установлена актуальная версия.".into()};}
                        }
                        Ok(v)=>{state.outcome="error".into();state.status=format!("Не удалось обновить: {}",v["error"].as_str().unwrap_or("ошибка менеджера").chars().take(700).collect::<String>());},
                        Err(_)=>{state.outcome="error".into();state.status="Менеджер обновлений вернул ошибку. Текущая версия сохранена.".into();},
                    }
                }
                Err(e)=>{state.outcome="error".into();state.status=format!("Не удалось запустить менеджер: {e}");},
            }
        });
        if spawn.is_err(){let mut state=self.state.lock().unwrap_or_else(|e|e.into_inner());state.busy=false;state.status="Не удалось начать обновление.".into();return false;}
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]fn development_copy_cannot_install_over_itself(){let updates=Updates{state:Default::default(),manager:PathBuf::from("/bin/true"),managed:false,version:"dev".into()};assert!(!updates.request("updates.install"));assert_eq!(updates.read("updates.available"),"false");}
    #[cfg(target_os="linux")]
    #[test]fn check_runs_outside_input_thread_and_coalesces_clicks(){
        use std::os::unix::fs::PermissionsExt;
        let dir=std::env::temp_dir().join(format!("clbb-updater-test-{}",std::process::id()));std::fs::create_dir_all(&dir).unwrap();
        let helper=dir.join("helper");let gate=dir.join("gate");let _=std::fs::remove_file(&gate);
        let script=format!("#!/bin/sh\nprintf '%s\\r' '######### 42.0%' >&2\ni=0\nwhile [ ! -f '{}' ] && [ \"$i\" -lt 100 ]; do i=$((i+1)); sleep .01; done\nprintf '%s' '{{\"available\":true,\"latest\":\"v0.1.0\"}}'\n",gate.display());
        std::fs::write(&helper,script).unwrap();std::fs::set_permissions(&helper,std::fs::Permissions::from_mode(0o700)).unwrap();
        let updates=Updates{state:Default::default(),manager:helper,managed:true,version:"v0.0.1".into()};
        assert!(updates.request("updates.check"));assert_eq!(updates.read("updates.busy"),"true");assert!(!updates.request("updates.check"));
        for _ in 0..200 {if updates.read("updates.progress")=="42"{break;}std::thread::sleep(std::time::Duration::from_millis(5));}
        assert_eq!(updates.read("updates.progress"),"42");assert_eq!(updates.read("updates.busy"),"true");
        std::fs::write(gate,b"go").unwrap();
        for _ in 0..200 {if updates.read("updates.busy")=="false"{break;}std::thread::sleep(std::time::Duration::from_millis(5));}
        assert_eq!(updates.read("updates.available"),"true");assert!(updates.read("updates.status").contains("v0.1.0"));let _=std::fs::remove_dir_all(dir);
    }
    #[test]fn refuses_unknown_commands_and_reports_missing_helper(){let updates=Updates{state:Default::default(),manager:PathBuf::from("/definitely/missing/clbb-manager"),managed:true,version:"v0.1.0".into()};assert!(!updates.request("updates.unknown"));assert!(!updates.request("updates.check"));assert_eq!(updates.read("updates.busy"),"false");assert!(!updates.read("updates.status").is_empty());}
}
