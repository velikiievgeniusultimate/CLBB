// SPDX-License-Identifier: GPL-3.0-or-later
//! Private, bounded report storage behind nginx. No public report-reading API.
use std::{io::{Read,Write},net::{TcpListener,TcpStream},path::PathBuf,sync::{Arc,atomic::{AtomicUsize,Ordering}},time::{Duration,SystemTime,UNIX_EPOCH}};
use serde_json::{json,Value};
const LIMIT:usize=512*1024;
fn response(stream:&mut TcpStream,status:&str,value:Value){let bytes=value.to_string();let _=write!(stream,"HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{bytes}",bytes.len());}
fn validate(value:&Value)->bool{value.is_object()&&value["schema"]=="clbb-diagnostic-v1"&&value["host"].is_object()&&value["kind"].is_string()}
fn serve(mut stream:TcpStream,dir:&std::path::Path)->std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;stream.set_write_timeout(Some(Duration::from_secs(10)))?;
    let mut bytes=Vec::new();let header_end=loop{let mut b=[0u8;1];if stream.read(&mut b)?==0{return Ok(());}bytes.push(b[0]);if bytes.ends_with(b"\r\n\r\n"){break bytes.len();}if bytes.len()>16384{response(&mut stream,"431 Request Header Fields Too Large",json!({"error":"headers too large"}));return Ok(());}};
    let headers=String::from_utf8_lossy(&bytes[..header_end]);let mut lines=headers.lines();
    if lines.next()!=Some("POST /reports HTTP/1.0")&& !headers.starts_with("POST /reports HTTP/1.1\r\n"){response(&mut stream,"405 Method Not Allowed",json!({"error":"POST only"}));return Ok(());}
    let mut length=None;let mut valid_type=false;
    for line in lines {if let Some((name,value))=line.split_once(':'){let name=name.trim().to_ascii_lowercase();let value=value.trim();if name=="transfer-encoding"{response(&mut stream,"400 Bad Request",json!({"error":"transfer encoding unsupported"}));return Ok(());}if name=="content-length"{if length.is_some(){response(&mut stream,"400 Bad Request",json!({"error":"duplicate length"}));return Ok(());}length=value.parse::<usize>().ok();}if name=="content-type"{valid_type=value.split(';').next()==Some("application/json");}}}
    let Some(length)=length.filter(|&n|n>0&&n<=LIMIT)else{response(&mut stream,"413 Content Too Large",json!({"error":"report limit 512KiB"}));return Ok(());};
    if !valid_type{response(&mut stream,"415 Unsupported Media Type",json!({"error":"JSON required"}));return Ok(());}
    let mut body=vec![0u8;length];stream.read_exact(&mut body)?;
    let Ok(mut value)=serde_json::from_slice::<Value>(&body)else{response(&mut stream,"400 Bad Request",json!({"error":"invalid JSON"}));return Ok(());};
    if !validate(&value){response(&mut stream,"400 Bad Request",json!({"error":"invalid CLBB report schema"}));return Ok(());}
    let used:u64=std::fs::read_dir(dir)?.filter_map(Result::ok).filter_map(|v|v.metadata().ok()).map(|v|v.len()).sum();
    if used>256*1024*1024 {response(&mut stream,"503 Service Unavailable",json!({"error":"report storage full"}));return Ok(());}
    let now=SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();let id=format!("{}-{}",now.as_secs(),now.subsec_nanos());value["received_at_unix"]=json!(now.as_secs());
    use std::os::unix::fs::OpenOptionsExt;
    let mut file=std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(dir.join(format!("{id}.json")))?;serde_json::to_writer_pretty(&mut file,&value)?;file.sync_all()?;
    eprintln!("Stored CLBB report {id}");response(&mut stream,"201 Created",json!({"id":id,"status":"stored"}));Ok(())
}
fn main()->std::io::Result<()>{
    let dir=PathBuf::from(std::env::var_os("CLBB_REPORT_DIR").unwrap_or_else(||"/var/lib/clbb-reports".into()));std::fs::create_dir_all(&dir)?;
    let active=Arc::new(AtomicUsize::new(0));for connection in TcpListener::bind("127.0.0.1:9631")?.incoming(){let stream=connection?;if active.fetch_add(1,Ordering::AcqRel)>=8{active.fetch_sub(1,Ordering::AcqRel);drop(stream);continue;}let active=active.clone();let dir=dir.clone();std::thread::spawn(move||{if let Err(e)=serve(stream,&dir){eprintln!("Report request rejected: {e}");}active.fetch_sub(1,Ordering::AcqRel);});}Ok(())
}
#[cfg(test)]mod tests{use super::*;#[test]fn requires_technical_report_schema(){assert!(!validate(&json!({"hello":"world"})));assert!(validate(&json!({"schema":"clbb-diagnostic-v1","kind":"selftest","host":{}})));}}

#[cfg(test)]mod transport_tests {
 use super::*;
 fn request(body:&[u8],length:usize)->(String,PathBuf){
  let dir=std::env::temp_dir().join(format!("clbb-receiver-test-{}-{}",std::process::id(),SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));std::fs::create_dir(&dir).unwrap();
  let listener=TcpListener::bind("127.0.0.1:0").unwrap();let address=listener.local_addr().unwrap();let destination=dir.clone();let worker=std::thread::spawn(move||serve(listener.accept().unwrap().0,&destination).unwrap());
  let mut client=TcpStream::connect(address).unwrap();write!(client,"POST /reports HTTP/1.0\r\nContent-Type: application/json\r\nContent-Length: {length}\r\n\r\n").unwrap();client.write_all(body).unwrap();let mut response=String::new();client.read_to_string(&mut response).unwrap();worker.join().unwrap();(response,dir)
 }
 #[test]fn valid_report_is_saved_and_receives_id(){let body=br#"{"schema":"clbb-diagnostic-v1","kind":"selftest","host":{}}"#;let(response,dir)=request(body,body.len());assert!(response.starts_with("HTTP/1.1 201"));let files:Vec<_>=std::fs::read_dir(&dir).unwrap().collect();assert_eq!(files.len(),1);std::fs::remove_dir_all(dir).unwrap();}
 #[test]fn invalid_report_is_not_persisted(){let body=b"{}";let(response,dir)=request(body,body.len());assert!(response.starts_with("HTTP/1.1 400"));assert_eq!(std::fs::read_dir(&dir).unwrap().count(),0);std::fs::remove_dir_all(dir).unwrap();}
 #[test]fn oversized_body_is_refused_before_read(){let(response,dir)=request(b"",LIMIT+1);assert!(response.starts_with("HTTP/1.1 413"));std::fs::remove_dir_all(dir).unwrap();}
}
