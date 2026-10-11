//! Descriptor-anchored Linux observation. No document locks or write permissions.
use super::*;
use rustix::fs::{Mode, OFlags};
use std::{
    fs::{File, Metadata},
    io::Read,
    os::unix::fs::MetadataExt,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
static INSPECTING: AtomicBool = AtomicBool::new(false);
struct Permit;
impl Permit {
    fn acquire() -> io::Result<Self> {
        INSPECTING
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| Self)
            .map_err(|_| invalid("saved release inspection busy; retry after current reader finishes"))
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        INSPECTING.store(false, Ordering::Release);
    }
}
struct Opened {
    file: File,
    metadata: Metadata,
}
fn same(a: &Metadata, b: &Metadata) -> bool {
    a.dev() == b.dev()
        && a.ino() == b.ino()
        && a.mode() == b.mode()
        && a.len() == b.len()
        && a.mtime() == b.mtime()
        && a.mtime_nsec() == b.mtime_nsec()
        && a.ctime() == b.ctime()
        && a.ctime_nsec() == b.ctime_nsec()
}
fn directory(path: &Path) -> io::Result<Opened> {
    let file = File::from(rustix::fs::open(path, OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC, Mode::empty())?);
    let metadata = file.metadata()?;
    if !metadata.is_dir() {
        return Err(invalid("release root must be a non-symlink directory"));
    }
    Ok(Opened { file, metadata })
}
fn child(parent: &File, name: &str, is_dir: bool) -> io::Result<Opened> {
    let flags = OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC | if is_dir { OFlags::DIRECTORY } else { OFlags::empty() };
    let file = File::from(rustix::fs::openat(parent, name, flags, Mode::empty())?);
    let metadata = file.metadata()?;
    if if is_dir { !metadata.is_dir() } else { !metadata.is_file() } {
        return Err(invalid("release entry must have its expected regular-file/directory type"));
    }
    Ok(Opened { file, metadata })
}
fn read(opened: &mut Opened, cap: usize, retain: bool) -> io::Result<Vec<u8>> {
    if opened.metadata.len() > cap as u64 {
        return Err(invalid("release file exceeds inspector per-file cap"));
    }
    let mut data = Vec::new();
    let mut buffer = [0u8; 32 * 1024];
    let mut count = 0usize;
    loop {
        let remaining = cap.saturating_sub(count).saturating_add(1).min(buffer.len());
        let n = opened.file.read(&mut buffer[..remaining])?;
        if n == 0 {
            break;
        }
        count = count.checked_add(n).ok_or_else(|| invalid("release byte count overflow"))?;
        if count > cap || count as u64 > opened.metadata.len() {
            return Err(invalid("release file changed or exceeded read bounds"));
        }
        if retain {
            data.extend_from_slice(&buffer[..n]);
        }
    }
    if count as u64 != opened.metadata.len() || !same(&opened.metadata, &opened.file.metadata()?) {
        return Err(invalid("release file changed while reading"));
    }
    Ok(data)
}
fn cap(name: &str) -> usize {
    if name.ends_with(".json") {
        if name == VALIDATION { 16 * 1024 * 1024 } else { JSON_LIMIT }
    } else if name.ends_with(".csv") {
        CSV_LIMIT
    } else {
        FAB_LIMIT
    }
}
fn components(name: &str) -> (&str, &str) {
    name.split_once('/').unwrap_or(("", name))
}
fn revalidate(root_path: &Path, root: &Opened, dirs: &BTreeMap<&str, Opened>, files: &BTreeMap<String, Opened>) -> io::Result<()> {
    if !same(&root.metadata, &root.file.metadata()?) || !same(&root.metadata, &directory(root_path)?.metadata) {
        return Err(invalid("release root path/identity changed during inspection"));
    }
    for (name, dir) in dirs {
        if !same(&dir.metadata, &dir.file.metadata()?) || !same(&dir.metadata, &child(&root.file, name, true)?.metadata) {
            return Err(invalid("release subdirectory path/identity changed during inspection"));
        }
    }
    for (name, file) in files {
        let (dir, base) = components(name);
        let parent = if dir.is_empty() { &root.file } else { &dirs[dir].file };
        if !same(&file.metadata, &file.file.metadata()?) || !same(&file.metadata, &child(parent, base, false)?.metadata) {
            return Err(invalid("release file path/identity changed during inspection"));
        }
    }
    Ok(())
}
pub(super) fn inspect(input: &InspectOutputRelease) -> io::Result<ReleaseInspection> {
    inspect_with(input, || {})
}
fn inspect_with(input: &InspectOutputRelease, before_final: impl FnOnce()) -> io::Result<ReleaseInspection> {
    if input.path.as_os_str().is_empty() || input.path.as_os_str().as_encoded_bytes().len() > 4096 {
        return Err(invalid("release path needs1..4096 encoded bytes"));
    }
    let path = std::path::absolute(&input.path)?;
    if path.as_os_str().as_encoded_bytes().len() > 4096 {
        return Err(invalid("absolute release path exceeds4096 bytes"));
    }
    let _permit = Permit::acquire()?;
    let root = directory(&path)?;
    let mut manifest_file = child(&root.file, "release-manifest.json", false)?;
    let manifest_bytes = read(&mut manifest_file, JSON_LIMIT, true)?;
    let manifest: Manifest = json(&manifest_bytes)?;
    let mut job_file = child(&root.file, "job.json", false)?;
    let job_bytes = read(&mut job_file, JSON_LIMIT, true)?;
    let job = crate::OutputJob::parse(&job_bytes)?;
    manifest.validate(&job)?;
    let mut dirs = BTreeMap::new();
    for name in ["assembly", "fabrication"].into_iter().chain((job.profile == crate::CHECKED_PROFILE).then_some("validation")) {
        dirs.insert(name, child(&root.file, name, true)?);
    }
    let mut files = BTreeMap::new();
    files.insert("release-manifest.json".to_owned(), manifest_file);
    files.insert("job.json".to_owned(), job_file);
    let mut total = manifest_bytes.len();
    let mut fabrication_total = 0usize;
    let mut facts = Vec::with_capacity(manifest.files.len());
    for (name, declared) in &manifest.files {
        if !files.contains_key(name) {
            let (dir, base) = components(name);
            let parent = if dir.is_empty() { &root.file } else { &dirs[dir].file };
            files.insert(name.clone(), child(parent, base, false)?);
        }
        let observed = usize::try_from(files[name].metadata.len()).map_err(|_| invalid("release entry size cannot fit platform"))?;
        if observed != *declared || observed > cap(name) {
            return Err(invalid(format!("release file {name} length differs or exceeds inspection cap")));
        }
        total = total.checked_add(observed).filter(|n| *n <= RELEASE_LIMIT).ok_or_else(|| invalid("observed release exceeds192MiB"))?;
        if name.starts_with("fabrication/") {
            fabrication_total = fabrication_total
                .checked_add(observed)
                .filter(|n| *n <= FAB_LIMIT)
                .ok_or_else(|| invalid("fabrication files exceed128MiB inspector cap"))?;
        }
        facts.push(ReleaseFileFact { path: name.clone(), declared_bytes: *declared, observed_bytes: observed });
    }
    let mut placement = None;
    let mut bom = None;
    for name in manifest.files.keys().filter(|name| name.as_str() != "job.json") {
        let parsed = matches!(name.as_str(), CALIBRATED | POPULATED | "fabrication/manifest.json");
        let bytes = read(files.get_mut(name).expect("opened fixed path"), cap(name), parsed)?;
        match name.as_str() {
            CALIBRATED => placement = Some(bytes),
            POPULATED => bom = Some(bytes),
            "fabrication/manifest.json" => validate_fabrication(&json::<FabManifest>(&bytes)?, &manifest, &job)?,
            _ => {}
        }
    }
    let matched = if let Some(cal) = &manifest.calibrated_placement {
        csv::inspect(
            placement.as_deref().expect("required calibrated layout"),
            bom.as_deref(),
            cal.component_count,
            manifest.populated_assembly.as_ref().map(|p| p.component_ids_sha256.as_str()),
        )?
    } else {
        None
    };
    before_final();
    revalidate(&path, &root, &dirs, &files)?;
    let mut final_manifest = child(&root.file, "release-manifest.json", false)?;
    if read(&mut final_manifest, JSON_LIMIT, true)? != manifest_bytes {
        return Err(invalid("release manifest bytes changed during inspection"));
    }
    revalidate(&path, &root, &dirs, &files)?;
    Ok(ReleaseInspection{
        path,format:manifest.format,version:manifest.version,profile:job.profile,project_id:manifest.project_id,exported_cursor:manifest.exported_cursor,
        file_count:facts.len()+1,observed_bytes:total,files:facts,
        calibrated_component_count:manifest.calibrated_placement.map(|c|c.component_count),matched_assembly:matched,release_exclusions:manifest.exclusions,
        checks_performed:vec!["Strict supported release/job layout and version/profile/summary matrix".into(),"Declared/observed regular file byte lengths and observed path/descriptor identity stability".into(),"Fabrication manifest project/files/mask/coordinate metadata consistency".into(),"When present: calibrated row/count validity and populated shared fields/component membership/identity digest".into()],
        checks_not_performed:vec!["No authenticated file-content digest, original-export authenticity or current-board equality".into(),"No geometry/Gerber/XNC/legacy assembly/validation report semantic or DRC recomputation".into(),"No vendor/manufacturing/stock/polarity acceptance".into(),"Unlisted filesystem entries not inspected; per-file inspector caps may refuse otherwise valid larger exports".into(),"Independent file reads are not an atomic cross-file snapshot; same-length unchecked-content edits can pass and files may change after return".into()],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        os::unix::fs::symlink,
        sync::{Mutex, mpsc},
    };
    static SERIAL: Mutex<()> = Mutex::new(());
    fn release(parent: &Path, name: &str, version: u32, checked: bool) -> PathBuf {
        let project = viamind_doc::file::from_str(include_str!("../../../../fixtures/assembly/output-job-population.viamind.json")).unwrap();
        let mut job = crate::OutputJob::new("Original inspection fixture".into(), 0.);
        job.version = version;
        job.include_calibrated_placement = (version >= 2).then_some(true);
        job.include_populated_bom = (version == 3).then_some(true);
        if checked {
            job.profile = crate::CHECKED_PROFILE.into();
            job.checks = Some(crate::OutputJobChecks {
                copper_clearance_mm: 0.,
                edge_clearance_mm: 0.,
                minimum_via_drill_mm: 0.,
                minimum_annular_width_mm: 0.,
                hole_clearance_mm: 0.,
                hole_to_copper_clearance_mm: 0.,
                max_violations: 100,
            });
        }
        let path = parent.join(name);
        let snapshot = crate::OutputJobSnapshot::capture(&project, job).unwrap();
        let cursor = ReleaseCursor { stream_id: "00000000-0000-0000-0000-000000000000".parse().unwrap(), sequence: 42 };
        // Deliberately opaque validation bytes: reader must not claim DRC validation.
        snapshot.write_with_validation(cursor, &path, None, |_, _, _| Ok(b"opaque fixture, not DRC authorization".to_vec())).unwrap();
        path
    }
    fn inspect_path(path: &Path) -> io::Result<ReleaseInspection> {
        inspect(&InspectOutputRelease { path: path.into() })
    }
    fn patch_manifest(path: &Path, patch: impl FnOnce(&mut serde_json::Value)) {
        let p = path.join("release-manifest.json");
        let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
        patch(&mut value);
        fs::write(p, serde_json::to_vec(&value).unwrap()).unwrap();
    }
    fn change_csv(path: &Path, name: &str, change: impl FnOnce(String) -> String) {
        let file = path.join(name);
        let value = change(fs::read_to_string(&file).unwrap());
        fs::write(file, value.as_bytes()).unwrap();
        patch_manifest(path, |m| m["files"][name] = serde_json::json!(value.len()));
    }
    #[test]
    fn supported_layouts_roundtrip_and_unchecked_content_is_honestly_excluded() {
        let _serial = SERIAL.lock().unwrap();
        let temp = tempfile::tempdir().unwrap();
        for version in 1..=3 {
            for checked in [false, true] {
                let path = release(temp.path(), &format!("v{version}-{checked}"), version, checked);
                let before = fs::read(path.join("release-manifest.json")).unwrap();
                let result = inspect_path(&path).unwrap();
                assert_eq!(result.version, version);
                assert_eq!(result.file_count, 10 + version as usize + usize::from(checked));
                assert_eq!(result.exported_cursor.sequence, 42);
                assert_eq!(result.calibrated_component_count, (version >= 2).then_some(2));
                assert_eq!(result.matched_assembly.as_ref().map(|m| m.component_count), (version == 3).then_some(2));
                assert_eq!(fs::read(path.join("release-manifest.json")).unwrap(), before);
                let copper = path.join("fabrication/top-copper.gbr");
                let mut bytes = fs::read(&copper).unwrap();
                bytes[0] = b'!';
                fs::write(&copper, &bytes).unwrap();
                assert!(inspect_path(&path).is_ok(), "same-length unchecked Gerber changes are not authenticated");
                fs::write(path.join("unlisted.txt"), b"explicitly not inspected").unwrap();
                assert!(inspect_path(&path).is_ok());
            }
        }
    }
    #[test]
    fn inspector_caps_refuse_large_exportable_tables_and_empty_population_is_valid() {
        use sha2::{Digest, Sha256};
        let _serial = SERIAL.lock().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let path = release(temp.path(), "empty", 3, false);
        for name in [CALIBRATED, POPULATED] {
            change_csv(&path, name, |s| format!("{}\r\n", s.split_once("\r\n").unwrap().0));
        }
        let digest = Sha256::digest(b"viamind-populated-components-v1\0").iter().map(|b| format!("{b:02x}")).collect::<String>();
        patch_manifest(&path, |m| {
            m["calibrated_placement"]["component_count"] = serde_json::json!(0);
            m["populated_assembly"]["component_count"] = serde_json::json!(0);
            m["populated_assembly"]["component_ids_sha256"] = serde_json::json!(digest);
        });
        assert_eq!(inspect_path(&path).unwrap().matched_assembly.unwrap().component_count, 0);
        let file = path.join("assembly/bom.csv");
        fs::OpenOptions::new().write(true).open(&file).unwrap().set_len((CSV_LIMIT + 1) as u64).unwrap();
        patch_manifest(&path, |m| m["files"]["assembly/bom.csv"] = serde_json::json!(CSV_LIMIT + 1));
        assert!(inspect_path(&path).unwrap_err().to_string().contains("cap"));
        fs::OpenOptions::new().write(true).open(path.join("release-manifest.json")).unwrap().set_len((JSON_LIMIT + 1) as u64).unwrap();
        assert!(inspect_path(&path).is_err());
    }
    #[test]
    fn strict_layout_lengths_and_matched_csv_errors_reject_without_partial_result() {
        let _serial = SERIAL.lock().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let path = release(temp.path(), "base", 3, false);
        let manifest = fs::read(path.join("release-manifest.json")).unwrap();
        for bad in ["../escape", "/absolute", "assembly/other.csv", "release-manifest.json"] {
            patch_manifest(&path, |m| {
                let files = m["files"].as_object_mut().unwrap();
                let old = files.remove("job.json").unwrap();
                files.insert(bad.into(), old);
            });
            assert!(inspect_path(&path).is_err());
            fs::write(path.join("release-manifest.json"), &manifest).unwrap();
        }
        for change in 0..4 {
            patch_manifest(&path, |m| match change {
                0 => m["version"] = serde_json::json!(2),
                1 => m["calibrated_placement"] = serde_json::Value::Null,
                2 => m["populated_assembly"]["component_ids_sha256"] = serde_json::json!("0".repeat(64)),
                _ => m["files"]["assembly/bom.csv"] = serde_json::json!(1),
            });
            assert!(inspect_path(&path).is_err());
            fs::write(path.join("release-manifest.json"), &manifest).unwrap();
        }
        let bom = fs::read(path.join(POPULATED)).unwrap();
        for bad in ["µ,", "WRONG", "garbage"] {
            change_csv(&path, POPULATED, |s| if bad == "garbage" { format!("{s}garbage") } else { s.replacen("A_TOP", bad, 1) });
            assert!(inspect_path(&path).is_err());
            fs::write(path.join(POPULATED), &bom).unwrap();
            fs::write(path.join("release-manifest.json"), &manifest).unwrap();
        }
        change_csv(&path, CALIBRATED, |s| s.replacen("13.963514", "1e99", 1));
        assert!(inspect_path(&path).is_err());
    }
    #[test]
    fn symlink_fifo_and_observed_directory_or_file_replacement_refuse() {
        let _serial = SERIAL.lock().unwrap();
        let temp = tempfile::tempdir().unwrap();
        for (index, kind) in ["root", "subdir", "file", "same_length"].iter().enumerate() {
            let path = release(temp.path(), &format!("race{index}"), 3, false);
            let result = inspect_with(&InspectOutputRelease { path: path.clone() }, || match *kind {
                "root" => {
                    fs::rename(&path, path.with_extension("old")).unwrap();
                    fs::create_dir(&path).unwrap();
                }
                "subdir" => {
                    fs::rename(path.join("assembly"), path.join("old-assembly")).unwrap();
                    fs::create_dir(path.join("assembly")).unwrap();
                }
                "file" => {
                    let p = path.join("job.json");
                    let bytes = fs::read(&p).unwrap();
                    fs::rename(&p, path.join("old-job")).unwrap();
                    fs::write(&p, bytes).unwrap();
                }
                _ => {
                    let p = path.join("fabrication/top-copper.gbr");
                    let mut bytes = fs::read(&p).unwrap();
                    bytes[0] = b'!';
                    fs::write(&p, bytes).unwrap();
                }
            });
            assert!(result.is_err(), "{kind}");
        }
        let path = release(temp.path(), "unsafe", 1, false);
        let file = path.join("assembly/bom.csv");
        let old = path.join("original.csv");
        fs::rename(&file, &old).unwrap();
        symlink(&old, &file).unwrap();
        assert!(inspect_path(&path).is_err());
        fs::remove_file(&file).unwrap();
        rustix::fs::mkfifoat(rustix::fs::CWD, &file, Mode::RUSR | Mode::WUSR).unwrap();
        assert!(inspect_path(&path).is_err());
        let link = temp.path().join("root-link");
        symlink(&path, &link).unwrap();
        assert!(inspect_path(&link).is_err());
    }
    #[test]
    fn canceled_receiver_does_not_release_real_owned_reader_permit() {
        let _serial = SERIAL.lock().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let path = release(temp.path(), "cancel", 3, false);
        let (started, waiting) = mpsc::channel();
        let (release, resume) = mpsc::channel();
        let (result, receiver) = mpsc::channel();
        let request = InspectOutputRelease { path: path.clone() };
        let worker = std::thread::spawn(move || {
            let read = inspect_with(&request, || {
                started.send(()).unwrap();
                resume.recv().unwrap();
            });
            let _ = result.send(read);
        });
        waiting.recv().unwrap();
        drop(receiver);
        assert!(inspect_path(&path).unwrap_err().to_string().contains("busy"));
        release.send(()).unwrap();
        worker.join().unwrap();
        assert!(inspect_path(&path).is_ok());
    }
}
