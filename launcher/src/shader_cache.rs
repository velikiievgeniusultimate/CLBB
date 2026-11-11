// SPDX-License-Identifier: MIT
//! One-time disk migration of compiled shaders, never preload game assets.
use std::{fs::{self, File, OpenOptions}, io::{self, Read, Write}, path::Path};
#[derive(Default, Debug)]
pub struct Imported { pub files: u64, pub bytes: u64 }
pub fn import(legacy: &Path, cache_root: &Path) -> io::Result<Imported> {
    let marker = cache_root.join(".clbb-legacy-shader-cache-imported");
    if marker.try_exists()? { return Ok(Imported::default()); }
    match fs::symlink_metadata(legacy) {
        Ok(m) if m.is_dir() && !m.file_type().is_symlink() => (),
        Ok(_) => return Ok(Imported::default()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Imported::default()),
        Err(e) => return Err(e),
    }
    fs::create_dir_all(cache_root)?;
    let destination = cache_root.join("UnityShaderCache");
    match fs::symlink_metadata(&destination) {
        Ok(m) if !m.is_dir() || m.file_type().is_symlink() => return Err(io::Error::other("Cache destination is not an owned directory")),
        Ok(_) => (),
        Err(e) if e.kind() == io::ErrorKind::NotFound => fs::create_dir(&destination)?,
        Err(e) => return Err(e),
    }
    let mut result = Imported::default();
    for (index, entry) in fs::read_dir(legacy)?.enumerate() {
        if index >= 4096 { return Err(io::Error::other("Legacy shader cache entry bound")); }
        let entry = entry?;
        let meta = fs::symlink_metadata(entry.path())?;
        if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 4*1024*1024 { continue; }
        let target = destination.join(entry.file_name());
        match fs::symlink_metadata(&target) {
            Ok(_) => continue, // Existing entries always belong to the current runtime.
            Err(e) if e.kind() == io::ErrorKind::NotFound => (),
            Err(e) => return Err(e),
        }
        if result.bytes + meta.len() > 64*1024*1024 { return Err(io::Error::other("Legacy shader cache byte bound")); }
        let temporary = cache_root.join(format!(".clbb-shader-import-{}-{index}",std::process::id()));
        let copied = (|| -> io::Result<u64> {
            let mut output = OpenOptions::new().write(true).create_new(true).open(&temporary)?;
            let input = File::open(entry.path())?;
            if !input.metadata()?.is_file() { return Err(io::Error::other("Legacy shader entry changed type")); }
            let size = io::copy(&mut input.take(4*1024*1024+1), &mut output)?;
            if size > 4*1024*1024 { return Err(io::Error::other("Legacy shader entry grew beyond bound")); }
            drop(output);
            match fs::hard_link(&temporary, &target) {
                Ok(()) => Ok(size),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(0),
                Err(e) => Err(e),
            }
        })();
        let _ = fs::remove_file(&temporary);
        let bytes = copied?;
        if bytes != 0 { result.files += 1; result.bytes += bytes; }
    }
    // Marker is outside Unity's own binary cache, published only after success.
    match OpenOptions::new().write(true).create_new(true).open(marker) {
        Ok(mut f) => writeln!(f,"files={} bytes={}",result.files,result.bytes)?,
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e),
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Scratch(std::path::PathBuf);
    impl Scratch { fn new(name:&str)->Self { let p=std::env::temp_dir().join(format!("clbb-shader-cache-{name}-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));fs::create_dir(&p).unwrap();Self(p) } }
    impl Drop for Scratch { fn drop(&mut self) { let _=fs::remove_dir_all(&self.0); } }
    #[test] fn copy_preserves_existing_and_source_and_runs_once() {
        let t=Scratch::new("copy");let old=t.0.join("old");let root=t.0.join("new");fs::create_dir(&old).unwrap();fs::create_dir_all(root.join("UnityShaderCache")).unwrap();fs::write(old.join("a"),b"shader-a").unwrap();fs::write(old.join("b"),b"older-b").unwrap();fs::write(root.join("UnityShaderCache/b"),b"current-b").unwrap();
        let r=import(&old,&root).unwrap();assert_eq!(r.files,1);assert_eq!(fs::read(root.join("UnityShaderCache/a")).unwrap(),b"shader-a");assert_eq!(fs::read(root.join("UnityShaderCache/b")).unwrap(),b"current-b");assert_eq!(fs::read(old.join("a")).unwrap(),b"shader-a");fs::write(old.join("c"),b"later").unwrap();assert_eq!(import(&old,&root).unwrap().files,0);assert!(!root.join("UnityShaderCache/c").exists());
    }
    #[test] fn missing_source_does_not_create_cache_or_marker() { let t=Scratch::new("missing");let root=t.0.join("new");assert_eq!(import(&t.0.join("absent"),&root).unwrap().files,0);assert!(!root.exists()); }
    #[cfg(unix)] #[test] fn links_are_not_followed_and_oversized_entries_skipped() {
        use std::os::unix::fs::symlink;let t=Scratch::new("links");let old=t.0.join("old");let root=t.0.join("new");fs::create_dir(&old).unwrap();let secret=t.0.join("outside");fs::write(&secret,b"outside").unwrap();symlink(&secret,old.join("link")).unwrap();File::create(old.join("large")).unwrap().set_len(4*1024*1024+1).unwrap();assert_eq!(import(&old,&root).unwrap().files,0);assert!(!root.join("UnityShaderCache/link").exists());assert!(!root.join("UnityShaderCache/large").exists());assert_eq!(fs::read(secret).unwrap(),b"outside");
    }
}
