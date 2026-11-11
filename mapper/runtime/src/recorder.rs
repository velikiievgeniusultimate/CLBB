//! Opt-in gameplay flight recorder. Disk I/O and JSON serialization stay off the input thread.
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    fs::{self, OpenOptions},
    io::{BufWriter, Write},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
    },
    time::{SystemTime, UNIX_EPOCH},
};
const SECOND: i64 = 1_000_000_000;
const RING_LIMIT: usize = 32768;
const CLIP_LIMIT: usize = 131072;
const DISK_LIMIT: u64 = 256 * 1024 * 1024;
#[derive(Clone, Serialize)]
struct Event {
    ns: i64,
    kind: &'static str,
    data: Value,
}
#[derive(Serialize)]
struct Clip {
    start_ns: i64,
    end_ns: i64,
    first_mark_ns: i64,
    marks: Vec<(i64, String)>,
    rows: Vec<Arc<Event>>,
    truncated: bool,
    context: Value,
}
#[derive(Default)]
struct Buffer {
    ring: VecDeque<Arc<Event>>,
    clips: Vec<Clip>,
    last_auto: i64,
    latest_ns: i64,
    context: Value,
    last_prune_ns:i64,
}
impl Buffer {
    fn push(&mut self, row: Event) -> Vec<Clip> {
        // Worker and GTK observations can reach this mutex out of timestamp order.
        self.latest_ns = self.latest_ns.max(row.ns);
        let row = Arc::new(row);
        self.ring.push_back(row.clone());
        let cutoff=self.latest_ns-5*SECOND;
        while self.ring.front().is_some_and(|r|r.ns<cutoff){self.ring.pop_front();}
        // Timestamp disorder is bounded by producers; remove interior late rows
        // occasionally, not by scanning thousands of entries per input packet.
        if self.latest_ns-self.last_prune_ns>=100_000_000 {
            self.ring.retain(|r|r.ns>=cutoff);self.last_prune_ns=self.latest_ns;
        }
        while self.ring.len() > RING_LIMIT {
            self.ring.pop_front();
        }
        for clip in &mut self.clips {
            if row.ns >= clip.start_ns && row.ns <= clip.end_ns {
                if clip.rows.len() < CLIP_LIMIT {
                    clip.rows.push(row.clone());
                } else {
                    clip.truncated = true;
                }
            }
        }
        let mut ready = Vec::new();
        let mut i = 0;
        while i < self.clips.len() {
            if self.clips[i].end_ns <= self.latest_ns {
                ready.push(self.clips.remove(i));
            } else {
                i += 1;
            }
        }
        ready
    }
    fn mark(&mut self, now: i64, reason: &str, automatic: bool) -> bool {
        if automatic && self.last_auto != 0 && now - self.last_auto < 2 * SECOND {
            return false;
        }
        if automatic {
            self.last_auto = now;
        }
        // Merge nearby marks, but start an overlapping clip before a long burst
        // could grow one allocation without bound. Each mark retains its +5s.
        if let Some(c) = self
            .clips
            .last_mut()
            .filter(|c| now - c.first_mark_ns <= 10 * SECOND)
        {
            if c.marks.len() >= 128 {
                return false;
            }
            c.end_ns = c.end_ns.max(now + 5 * SECOND);
            c.marks.push((now, reason.chars().take(256).collect()));
        } else {
            if self.clips.len() >= 2 {
                return false;
            }
            self.clips.push(Clip {
                context:self.context.clone(),
                start_ns: now - 5 * SECOND,
                end_ns: now + 5 * SECOND,
                first_mark_ns: now,
                marks: vec![(now, reason.chars().take(256).collect())],
                rows: self
                    .ring
                    .iter()
                    .filter(|r| r.ns >= now - 5 * SECOND && r.ns <= now)
                    .cloned()
                    .collect(),
                truncated: self
                    .ring
                    .front()
                    .is_none_or(|r| r.ns > now - 5 * SECOND + 20_000_000),
            });
        }
        true
    }
}
pub struct Recorder {
    pub enabled: AtomicBool,
    buffer: Mutex<Buffer>,
    sender: SyncSender<Clip>,
    status: Arc<Mutex<String>>,
    last_error: Arc<Mutex<Option<String>>>,
    directory: PathBuf,
}
impl Recorder {
    pub fn new() -> Arc<Self> {
        let directory = std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/state")
            })
            .join("clbb/aim-debug");
        Self::at(directory)
    }
    pub(crate) fn at(directory: PathBuf) -> Arc<Self> {
        let (sender, receiver) = mpsc::sync_channel::<Clip>(2);
        let status = Arc::new(Mutex::new("Ожидаю метку".into()));
        let last_error = Arc::new(Mutex::new(None));
        let worker_status = status.clone();
        let worker_error = last_error.clone();
        let path = directory.clone();
        std::thread::Builder::new().name("clbb-debug-writer".into()).spawn(move || {
            let session = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
            for (sequence, mut clip) in receiver.into_iter().enumerate() {
                clip.rows.sort_by_key(|r| r.ns);
                match write_clip(&path, &format!("clip-{session}-{sequence}"), &clip, DISK_LIMIT) {
                    Ok(file) => {
                        *worker_status.lock().unwrap() = format!("Сохранено: {}",file.display());
                        *worker_error.lock().unwrap() = None;
                    }
                    Err(e) => *worker_error.lock().unwrap() = Some(format!("Ошибка записи: {e}")),
                }
            }
        }).expect("debug writer");
        Arc::new(Self {
            enabled: AtomicBool::new(false),
            buffer: Mutex::new(Buffer::default()),
            sender,
            status,
            last_error,
            directory,
        })
    }
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }
    pub fn set_enabled(&self, enabled: bool) {
        let mut b = self.buffer.lock().unwrap();
        if self.enabled.swap(enabled, Ordering::Relaxed) == enabled {
            return;
        }
        if !enabled {
            for mut clip in b.clips.drain(..) {
                clip.truncated = true;
                self.send(clip);
            }
        }
        *b = Buffer::default();
        if enabled {
            *self.status.lock().unwrap() = format!("Скользящее окно 5 с · {}", self.directory.display());
        }
    }

    fn send(&self, clip: Clip) {
        if self.sender.try_send(clip).is_err() {
            *self.last_error.lock().unwrap() = Some("Запись пропущена: очередь диска занята".into());
        }
    }
    pub fn set_context(&self,context:Value){self.buffer.lock().unwrap().context=context;}
    pub fn push(&self, ns: i64, kind: &'static str, data: Value) {
        let mut buffer = self.buffer.lock().unwrap();
        if !self.is_enabled() { return; }
        let ready = buffer.push(Event { ns, kind, data });
        drop(buffer);
        for clip in ready {
            self.send(clip);
        }
    }
    pub fn mark(&self, ns: i64, reason: &str, automatic: bool) -> bool {
        let mut buffer = self.buffer.lock().unwrap();
        if !self.is_enabled() { return false; }
        let marked = buffer.mark(ns, reason, automatic);
        if marked {
            *self.status.lock().unwrap() = format!("Метка: {reason} · собираю следующие 5 с");
        }
        marked
    }
    pub fn status(&self) -> String {
        let state = if self.is_enabled() { "Запись включена" } else { "Запись выключена" };
        let detail = self.last_error.lock().unwrap().clone()
            .unwrap_or_else(||self.status.lock().unwrap().clone());
        format!("{state} · {detail}")
    }
}
// Only completed clips are rotated. Archives and a failed partial remain recoverable.
fn write_clip(path: &std::path::Path, name: &str, clip: &Clip, limit: u64) -> Result<PathBuf,String> {
    fs::create_dir_all(path).map_err(|e|e.to_string())?;
    let mut old = fs::read_dir(path).map_err(|e|e.to_string())?
        .collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?
        .into_iter().filter(|e| {
            let n=e.file_name(); let n=n.to_string_lossy();
            n.starts_with("clip-") && n.ends_with(".jsonl")
        }).map(|e|e.metadata().map(|m|(e.path(),m.len(),m.modified().ok())))
        .collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
    old.sort_by_key(|(_,_,t)|*t);
    let mut used: u64 = old.iter().map(|(_,n,_)|n).sum();
    let mut old = old.into_iter();
    let file=path.join(format!("{name}.jsonl"));
    let pending=path.join(format!("{name}.partial"));
    let result=(|| -> Result<(),String> {
        let mut writer=BufWriter::new(OpenOptions::new().write(true).create_new(true).open(&pending).map_err(|e|e.to_string())?);
        let mut channels=std::collections::BTreeMap::<&str,usize>::new();
        for row in &clip.rows{*channels.entry(row.kind).or_default()+=1;}
        let header=json!({"context":clip.context,"event_count":clip.rows.len(),"channels":channels,"coverage_start_ns":clip.rows.iter().map(|r|r.ns).min(),"coverage_end_ns":clip.rows.iter().map(|r|r.ns).max(),"native_chain_observed":channels.get("android_motion").is_some_and(|n|*n>0),"format":"clbb-aim-debug","version":2,"pre_seconds":5,"post_seconds":5,"start_ns":clip.start_ns,"end_ns":clip.end_ns,"marks":clip.marks,"truncated":clip.truncated,"clock":"CLOCK_MONOTONIC nanoseconds","scope":"local gameplay telemetry; no full process-memory dump"});
        let mut written=0u64;
        for row in std::iter::once(header).chain(clip.rows.iter().map(|r|serde_json::to_value(r.as_ref()).unwrap())) {
            let bytes=serde_json::to_vec(&row).map_err(|e|e.to_string())?;
            written+=bytes.len() as u64+1;
            while used+written>limit {
                let Some((old_file,size,_))=old.next() else {return Err("Один фрагмент превышает лимит записи".into());};
                let archive=path.join("archive");
                fs::create_dir_all(&archive).map_err(|e|e.to_string())?;
                let target=archive.join(old_file.file_name().unwrap());
                if target.exists() {return Err("Архив уже содержит одноимённый файл".into());}
                fs::rename(&old_file,target).map_err(|e|e.to_string())?;
                used=used.saturating_sub(size);
            }
            writer.write_all(&bytes).and_then(|_|writer.write_all(b"\n")).map_err(|e|e.to_string())?;
        }
        writer.flush().map_err(|e|e.to_string())?;
        writer.get_ref().sync_all().map_err(|e|e.to_string())?;
        drop(writer);
        fs::rename(&pending,&file).map_err(|e|e.to_string())
    })();
    result.map_err(|e|format!("{e}; незавершённый файл: {}",pending.display()))?;
    Ok(file)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn row(ns: i64) -> Event {
        Event {
            ns,
            kind: "test",
            data: Value::Null,
        }
    }
    fn temp_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("clbb-recorder-{label}-{}-{}",std::process::id(),SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()))
    }
    fn wait_for(recorder: &Recorder, text: &str) {
        let deadline=std::time::Instant::now()+std::time::Duration::from_secs(3);
        while !recorder.status().contains(text) {
            assert!(std::time::Instant::now()<deadline,"{}",recorder.status());
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    #[test]
    fn repeated_enable_mark_disable_preserves_every_manual_clip() {
        let dir=temp_dir("cycles"); let r=Recorder::at(dir.clone());
        for cycle in 0..12 {
            assert!(!r.mark(SECOND,"disabled",false));
            r.set_enabled(true); r.set_enabled(true);
            r.push(10*SECOND,"input_event",json!({"code":0x04000053}));
            assert!(r.mark(10*SECOND,"manual",false));
            // Exercise both immediate OFF and natural +5s completion.
            if cycle%2==0 {r.push(15*SECOND,"source",json!({"ok":true}));}
            r.set_enabled(false); r.set_enabled(false);
            let deadline=std::time::Instant::now()+std::time::Duration::from_secs(3);
            loop {
                let files=fs::read_dir(&dir).ok().into_iter().flatten().filter_map(Result::ok)
                    .filter(|e|e.path().extension().is_some_and(|v|v=="jsonl")).count();
                if files==cycle+1 {break;}
                assert!(std::time::Instant::now()<deadline,"{}",r.status());
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
        for entry in fs::read_dir(&dir).unwrap() {
            let text=fs::read_to_string(entry.unwrap().path()).unwrap();
            let rows:Vec<Value>=text.lines().map(|l|serde_json::from_str(l).unwrap()).collect();
            assert_eq!(rows[0]["marks"][0][1],"manual");
            assert!(rows.iter().any(|r|r["kind"]=="input_event"));
        }
        assert!(r.status().starts_with("Запись выключена"));
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn disk_failure_survives_off_and_retry_then_success_recovers() {
        let dir=temp_dir("error"); fs::write(&dir,b"blocked").unwrap();
        let r=Recorder::at(dir.clone()); r.set_enabled(true);
        assert!(r.mark(SECOND,"manual",false)); r.set_enabled(false);
        wait_for(&r,"Ошибка записи"); r.set_enabled(true); r.set_enabled(false);
        assert!(r.status().contains("Ошибка записи"));
        fs::remove_file(&dir).unwrap(); r.set_enabled(true);
        assert!(r.mark(SECOND,"retry",false)); r.set_enabled(false);
        wait_for(&r,"Сохранено:"); assert!(!r.status().contains("Ошибка записи"));
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn oversized_fragment_keeps_a_recoverable_partial_with_an_error_path() {
        let dir=temp_dir("partial"); let mut b=Buffer::default();
        b.push(Event{ns:SECOND,kind:"large",data:json!({"data":"x".repeat(2000)})});
        b.mark(SECOND,"manual",false); let clip=b.clips.pop().unwrap();
        let error=write_clip(&dir,"clip-large",&clip,1000).unwrap_err();
        assert!(error.contains("clip-large.partial"));
        assert!(!dir.join("clip-large.jsonl").exists());
        let partial=fs::read_to_string(dir.join("clip-large.partial")).unwrap();
        assert_eq!(serde_json::from_str::<Value>(partial.lines().next().unwrap()).unwrap()["marks"][0][1],"manual");
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn quota_rotates_old_completed_clip_without_losing_new_or_old_data() {
        let dir=temp_dir("quota"); fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("clip-old.jsonl"),vec![b'x';1000]).unwrap();
        let mut b=Buffer::default(); b.push(row(SECOND)); b.mark(SECOND,"manual",false);
        let clip=b.clips.pop().unwrap();
        let new=write_clip(&dir,"clip-new",&clip,1000).unwrap();
        let saved=fs::read_to_string(new).unwrap();
        assert_eq!(serde_json::from_str::<Value>(saved.lines().next().unwrap()).unwrap()["marks"][0][1],"manual");
        assert_eq!(fs::read(dir.join("archive/clip-old.jsonl")).unwrap(),vec![b'x';1000]);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn windows_keep_five_seconds_before_and_after_and_merge_marks() {
        let mut b = Buffer::default();
        for i in 0..=10 {
            b.push(row(i * SECOND));
        }
        assert!(b.mark(10 * SECOND, "manual", false));
        b.push(row(12 * SECOND));
        assert!(b.mark(12 * SECOND, "auto", true));
        assert!(b.push(row(16 * SECOND)).is_empty());
        let ready = b.push(row(17 * SECOND));
        assert_eq!(ready.len(), 1);
        let c = &ready[0];
        assert_eq!(c.start_ns, 5 * SECOND);
        assert_eq!(c.end_ns, 17 * SECOND);
        assert_eq!(c.rows.first().unwrap().ns, 5 * SECOND);
        assert_eq!(c.rows.last().unwrap().ns, 17 * SECOND);
        assert_eq!(c.marks.len(), 2);
    }
    #[test]
    fn repeated_marks_do_not_grow_an_unbounded_clip_or_ring() {
        let mut b = Buffer::default();
        for i in 0..60 {
            b.push(row(i * SECOND));
            b.mark(i * SECOND, "auto", true);
            assert!(b.clips.len() <= 2);
            for c in &b.clips {
                assert!(c.end_ns - c.start_ns <= 20 * SECOND);
            }
        }
        let mut b = Buffer::default();
        for _ in 0..RING_LIMIT + 5 {
            b.push(row(0));
        }
        assert_eq!(b.ring.len(), RING_LIMIT);
    }
    #[test]
    fn schema_two_keeps_context_and_complete_native_packet_across_ring_rotation() {
        let dir=temp_dir("schema-two");let mut b=Buffer::default();
        b.context=json!({"revision":"tested-build","settings":{"debug.recording":true}});
        for i in 0..=1000 {b.push(row(i*10_000_000));}
        for stage in ["before_dispatch","after_dispatch","motion_event_reader"] {
            b.push(Event{ns:10*SECOND,kind:"android_motion",data:json!({"packet":42,"stage":stage,"action":261,"pointers":[{"android_id":0},{"android_id":1}]})});
        }
        b.mark(10*SECOND,"manual",false);
        b.context=json!({"revision":"later"});
        let ready=b.push(row(15*SECOND));
        assert!(!ready[0].truncated);
        let file=write_clip(&dir,"clip-schema-two",&ready[0],10_000_000).unwrap();
        let rows:Vec<Value>=fs::read_to_string(file).unwrap().lines().map(|line|serde_json::from_str(line).unwrap()).collect();
        assert_eq!(rows[0]["version"],2);
        assert_eq!(rows[0]["context"]["revision"],"tested-build");
        assert_eq!(rows[0]["channels"]["android_motion"],3);
        assert_eq!(rows[0]["native_chain_observed"],true);
        assert_eq!(rows.iter().filter(|row|row["kind"]=="android_motion"&&row["data"]["packet"]==42).count(),3);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn automatic_marks_are_debounced_but_manual_marks_are_kept() {
        let mut b = Buffer::default();
        assert!(b.mark(SECOND, "auto", true));
        assert!(!b.mark(SECOND + 1, "auto", true));
        assert!(b.mark(SECOND + 1, "manual", false));
    }
}

pub fn skill_result(
    source: &mut crate::source::Source<crate::platform::linux::ProcessMemory>,
    show: u64,
    actor: u64,
) -> Result<serde_json::Value, String> {
    let r = &mut source.reflection;
    let start = crate::platform::linux::now_ns();
    let mut checks = Vec::new();
    let mut values = serde_json::Map::new();
    let mut objects = vec![("actor", actor)];
    let link = r.field(show, "m_SkillCompCurrentSkillData")?;
    let raw = r.read(link, 8)?;
    let data = crate::reflection::u64_at(&raw, 0);
    checks.push((link, raw));
    if data != 0 {
        values.insert("result.owner_is_local_skill_comp".into(),json!(crate::source::skill_result_is_local(r,show,data,&mut checks)?));
        objects.push(("result", data));
    }
    for (role, object) in objects {
        let owner = r.pointer(object)?;
        checks.push((object, owner.to_le_bytes().to_vec()));
        let mut class = owner;
        for _ in 0..16 {
            if class == 0 {
                break;
            }
            let c = r.class(class)?;
            if role == "result" && class == owner && c.name != "ShowSkillData" {
                return Err("Unexpected local skill-result class".into());
            }
            for name in [
                "m_iFirstSkillUseCount",
                "m_iSecondSkillUseCount",
                "m_iThirdSkillUseCount",
                "m_iSkillUseCount",
                "m_glLastPushedSkillID",
                "m_InitSkillID",
                "m_SkillGuid",
                "m_TranID",
                "m_uSrcOwnerGuid",
                "m_uParentSkillGuid",
                "m_uSrcSkillGuid",
                "m_fSrcDirX",
                "m_fSrcDirY",
                "m_fSrcPosX",
                "m_fSrcPosY",
            ] {
                if let Some(field) = c.fields.get(name) {
                    if field.is_static || field.offset > 1048576 {
                        continue;
                    }
                    let length = match field.kind {
                        8 | 9 | 12 => 4,
                        13 => 8,
                        _ => continue,
                    };
                    let address = object + field.offset;
                    let raw = r.read(address, length)?;
                    let value = match field.kind {
                        8 => json!(i32::from_le_bytes(raw[..4].try_into().unwrap())),
                        9 => json!(u32::from_le_bytes(raw[..4].try_into().unwrap())),
                        12 => json!(f32::from_le_bytes(raw[..4].try_into().unwrap())),
                        13 => json!(f64::from_le_bytes(raw[..8].try_into().unwrap())),
                        _ => unreachable!(),
                    };
                    values.insert(format!("{role}.{name}"), value);
                    checks.push((address, raw));
                }
            }
            class = c.parent;
        }
    }
    for (address, bytes) in checks {
        if r.read(address, bytes.len())? != bytes {
            return Err("Local result changed during receipt".into());
        }
    }
    Ok(json!({"start_ns":start,"end_ns":crate::platform::linux::now_ns(),"values":values}))
}
