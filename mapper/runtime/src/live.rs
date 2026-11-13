// SPDX-License-Identifier: GPL-3.0-or-later
//! Stable C boundary for replacing controller code while the game stays alive.
//! Audio resources remain in the initial, pinned module: guest SDK callbacks
//! retain their original symbols and must never open a second mixer/capture.
use std::{ffi::{CStr,CString,c_char},sync::OnceLock};
pub const ABI:u64=0x434c4242_0001_0040;
#[repr(C)]
#[derive(Default,Clone,Copy,Debug)]
pub struct ReloadState {
    pub abi:u32,pub size:u32,pub idle:u32,pub recording:u32,
    pub hero:i32,pub fresh:u32,pub owners:[u64;4],pub sample_ns:i64,
}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct AudioServices {
    pub abi:u64,
    pub configure:Option<unsafe extern "C" fn(*const c_char,*const c_char)>,
    pub read:Option<unsafe extern "C" fn(*const c_char,*mut u8,usize)->usize>,
    pub refresh:Option<extern "C" fn()>,
    pub stream_gain:Option<extern "C" fn()->f32>,
}
static AUDIO:OnceLock<AudioServices>=OnceLock::new();
#[unsafe(no_mangle)]
pub extern "C" fn clbb_mapper_abi()->u64 {ABI}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clbb_mapper_install_audio_services(value:*const AudioServices)->i32 {
    let Some(value)=(unsafe{value.as_ref()}).copied()else{return 0};
    if value.abi!=ABI || value.configure.is_none() || value.read.is_none() || value.refresh.is_none() || value.stream_gain.is_none(){return 0;}
    if let Some(old)=AUDIO.get() {
        return i32::from(old.configure.map(|f|f as usize)==value.configure.map(|f|f as usize)
            &&old.read.map(|f|f as usize)==value.read.map(|f|f as usize)
            &&old.refresh.map(|f|f as usize)==value.refresh.map(|f|f as usize)
            &&old.stream_gain.map(|f|f as usize)==value.stream_gain.map(|f|f as usize));
    }
    i32::from(AUDIO.set(value).is_ok())
}
pub(crate) fn configure(input:&str,output:&str)->bool {
    let Some(services)=AUDIO.get()else{return false};
    if let(Ok(input),Ok(output))=(CString::new(input),CString::new(output)) {
        unsafe{services.configure.unwrap()(input.as_ptr(),output.as_ptr())};
    }
    true
}
pub(crate) fn read(key:&str)->Option<String> {
    let services=AUDIO.get()?;let key=CString::new(key).ok()?;let read=services.read.unwrap();
    let size=unsafe{read(key.as_ptr(),std::ptr::null_mut(),0)};
    if size>1_048_576{return Some("Audio diagnostic response exceeds live ABI bound".into());}
    let mut bytes=vec![0;size];let length=unsafe{read(key.as_ptr(),bytes.as_mut_ptr(),bytes.len())};
    if length>bytes.len(){return Some("Audio diagnostic changed during live read; refresh".into());}
    bytes.truncate(length);Some(String::from_utf8_lossy(&bytes).into_owned())
}
pub(crate) fn refresh()->bool {
    if let Some(s)=AUDIO.get(){s.refresh.unwrap()();true}else{false}
}
pub(crate) fn stream_gain()->Option<f32>{AUDIO.get().map(|s|s.stream_gain.unwrap()())}
unsafe extern "C" fn service_configure(input:*const c_char,output:*const c_char){
    if input.is_null()||output.is_null(){return;}
    let input=unsafe{CStr::from_ptr(input)}.to_string_lossy().into_owned();
    let output=unsafe{CStr::from_ptr(output)}.to_string_lossy().into_owned();
    crate::audio::configure(input,output);
}
unsafe extern "C" fn service_read(key:*const c_char,out:*mut u8,capacity:usize)->usize {
    if key.is_null(){return 0;}
    let key=unsafe{CStr::from_ptr(key)}.to_str().unwrap_or("");let text=crate::audio::read(key);
    if !out.is_null() && capacity>=text.len(){unsafe{std::ptr::copy_nonoverlapping(text.as_ptr(),out,text.len())};}
    text.len()
}
extern "C" fn service_refresh(){crate::audio::refresh();}
extern "C" fn service_stream_gain()->f32{crate::audio::stream_gain()}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clbb_mapper_audio_services(out:*mut AudioServices)->i32 {
    let Some(out)=(unsafe{out.as_mut()})else{return 0};
    *out=AudioServices{abi:ABI,configure:Some(service_configure),read:Some(service_read),refresh:Some(service_refresh),stream_gain:Some(service_stream_gain)};1
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn live_layout_is_explicit_and_stable(){
        assert_eq!(std::mem::size_of::<ReloadState>(),64);
        assert_eq!(std::mem::offset_of!(ReloadState,owners),24);
        assert_eq!(std::mem::offset_of!(ReloadState,sample_ns),56);
        assert_eq!(std::mem::size_of::<AudioServices>(),40);
        assert_eq!(clbb_mapper_abi(),0x434c4242_0001_0040);
    }
    #[test] fn missing_services_never_install_partial_dispatch(){
        assert_eq!(unsafe{clbb_mapper_install_audio_services(std::ptr::null())},0);
        let invalid=AudioServices{abi:ABI,configure:None,read:None,refresh:None,stream_gain:None};
        assert_eq!(unsafe{clbb_mapper_install_audio_services(&invalid)},0);
    }
}
