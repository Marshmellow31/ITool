//! Export options shared by the guided desktop workflow and CLI.
use crate::{CopiedFile, ExportEntry};
use anyhow::{Context, Result, ensure};
use chrono::{Local, NaiveDateTime, TimeZone};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::io::AsyncWriteExt;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExportOptions {
    pub chunk_kib: u32,
    pub new_only: bool,
    pub capture_timestamps: bool,
    pub jpeg_copies: bool,
    pub mp4_copies: bool,
    /// A hash of the scanned device identifier, never the identifier itself.
    pub expected_device: Option<String>,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            chunk_kib: crate::DEFAULT_TRANSFER_CHUNK_KIB,
            new_only: false,
            capture_timestamps: false,
            jpeg_copies: false,
            mp4_copies: false,
            expected_device: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExportOutcome {
    pub folder: PathBuf,
    pub copied: usize,
    pub skipped: usize,
    pub converted: usize,
    pub copied_bytes: u64,
    pub elapsed_seconds: f64,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct SourceVersion {
    path: String,
    bytes: u64,
    created: String,
    modified: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SavedFile {
    path: PathBuf,
    bytes: u64,
    sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct History {
    source: SourceVersion,
    original: SavedFile,
    converted: Option<SavedFile>,
    capture_timestamp_applied: bool,
    capture_time: Option<SystemTime>,
}

#[derive(Serialize)]
struct Report<'a> {
    transport: &'static str,
    chunk_kib: u32,
    options: &'a ExportOptions,
    files: &'a [CopiedFile],
    skipped_files: Vec<PathBuf>,
    converted_files: Vec<SavedFile>,
    outcome: &'a ExportOutcome,
    transfer_seconds: f64,
    verification: &'static str,
}

pub fn named_destination(parent: &str, name: &str) -> Result<PathBuf> {
    let parent = PathBuf::from(parent.trim());
    ensure!(parent.is_dir(), "Choose an existing save location");
    let name = name.trim();
    ensure!(!name.contains('/'), "Folder name cannot contain a slash");
    crate::local_name(&format!("/DCIM/{name}"))?;
    let dest = parent.join(name);
    ensure!(
        !dest.exists(),
        "That folder already exists. Choose a different folder name."
    );
    Ok(dest)
}

pub fn tool_path(name: &str) -> Option<PathBuf> {
    let file = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.into()
    };
    let next_to_app = std::env::current_exe()
        .ok()?
        .parent()?
        .join("tools")
        .join(&file);
    if next_to_app.is_file() {
        return Some(next_to_app);
    }
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|p| p.join(&file))
        .find(|p| p.is_file())
}

fn command(path: &Path) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(path);
    cmd.kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW for optional converter processes.
    cmd
}

async fn execute(
    mut cmd: tokio::process::Command,
    cancel: &AtomicBool,
) -> Result<std::process::Output> {
    let output = cmd.output();
    tokio::pin!(output);
    loop {
        tokio::select! {
            result = &mut output => return Ok(result?),
            _ = tokio::time::sleep(Duration::from_millis(150)) => {
                ensure!(!cancel.load(Ordering::Relaxed), "Cancelled; copied originals are preserved");
            }
        }
    }
}

fn ascii(exif: &exif::Exif, tag: exif::Tag) -> Option<String> {
    match &exif.get_field(tag, exif::In::PRIMARY)?.value {
        exif::Value::Ascii(values) => Some(
            String::from_utf8_lossy(values.first()?)
                .trim_end_matches('\0')
                .to_owned(),
        ),
        _ => None,
    }
}

fn system_time(seconds: i64) -> Result<SystemTime> {
    if seconds >= 0 {
        Ok(UNIX_EPOCH
            .checked_add(Duration::from_secs(seconds as u64))
            .context("Capture date out of range")?)
    } else {
        Ok(UNIX_EPOCH
            .checked_sub(Duration::from_secs(seconds.unsigned_abs()))
            .context("Capture date out of range")?)
    }
}

async fn capture_time(path: &Path, cancel: &AtomicBool) -> Result<(SystemTime, bool)> {
    let path_owned = path.to_owned();
    let exif_date = tokio::task::spawn_blocking(move || -> Option<(String, Option<String>)> {
        let mut reader = std::io::BufReader::new(std::fs::File::open(path_owned).ok()?);
        let data = exif::Reader::new().read_from_container(&mut reader).ok()?;
        Some((
            ascii(&data, exif::Tag::DateTimeOriginal)?,
            ascii(&data, exif::Tag::OffsetTimeOriginal),
        ))
    })
    .await?;
    if let Some((date, offset)) = exif_date {
        let naive = NaiveDateTime::parse_from_str(&date, "%Y:%m:%d %H:%M:%S")?;
        if let Some(offset) = offset {
            let date = chrono::DateTime::parse_from_str(
                &format!("{date} {offset}"),
                "%Y:%m:%d %H:%M:%S %:z",
            )?;
            return Ok((system_time(date.timestamp())?, false));
        }
        let date = Local
            .from_local_datetime(&naive)
            .single()
            .context("Capture time is ambiguous in the PC timezone")?;
        return Ok((system_time(date.timestamp())?, true));
    }
    let probe = tool_path("ffprobe")
        .context("No embedded EXIF date; install FFmpeg/ffprobe to read video capture dates")?;
    let mut cmd = command(&probe);
    cmd.args([
        "-v",
        "error",
        "-protocol_whitelist",
        "file,pipe",
        "-show_entries",
        "format_tags=creation_time,com.apple.quicktime.creationdate:stream_tags=creation_time",
        "-of",
        "json",
    ])
    .arg(path);
    let output = tokio::time::timeout(Duration::from_secs(30), execute(cmd, cancel))
        .await
        .context("Capture date lookup timed out")??;
    ensure!(
        output.status.success(),
        "Capture metadata could not be read"
    );
    let data: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let text = data["format"]["tags"]["com.apple.quicktime.creationdate"]
        .as_str()
        .or_else(|| data["format"]["tags"]["creation_time"].as_str())
        .or_else(|| {
            data["streams"]
                .as_array()?
                .iter()
                .find_map(|s| s["tags"]["creation_time"].as_str())
        })
        .context("No embedded capture date; existing destination timestamps were kept")?;
    let date = chrono::DateTime::parse_from_rfc3339(text)?;
    Ok((system_time(date.timestamp())?, false))
}

fn stamp(path: &Path, time: SystemTime) -> Result<()> {
    let file = std::fs::OpenOptions::new().write(true).open(path)?;
    let times = std::fs::FileTimes::new().set_modified(time);
    #[cfg(windows)]
    let times = {
        use std::os::windows::fs::FileTimesExt;
        times.set_created(time)
    };
    file.set_times(times)?;
    Ok(())
}

fn conversion_extension(source: &str, options: &ExportOptions) -> Option<&'static str> {
    let ext = source.rsplit('.').next()?.to_ascii_lowercase();
    if options.jpeg_copies && matches!(ext.as_str(), "heic" | "heif") {
        Some("jpg")
    } else if options.mp4_copies && matches!(ext.as_str(), "mov" | "m4v") {
        Some("mp4")
    } else {
        None
    }
}

async fn convert(path: &Path, extension: &str, cancel: &AtomicBool) -> Result<PathBuf> {
    let ffmpeg = tool_path("ffmpeg").context("FFmpeg is required for compatible copies")?;
    if extension == "mp4" {
        let probe = tool_path("ffprobe")
            .context("ffprobe is required to check video color before conversion")?;
        let mut cmd = command(&probe);
        cmd.args([
            "-v",
            "error",
            "-protocol_whitelist",
            "file,pipe",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=color_transfer",
            "-of",
            "json",
        ])
        .arg(path);
        let output = tokio::time::timeout(Duration::from_secs(30), execute(cmd, cancel))
            .await
            .context("Video color lookup timed out")??;
        ensure!(
            output.status.success(),
            "Video color information unavailable"
        );
        let data: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        let transfer = data["streams"][0]["color_transfer"].as_str().unwrap_or("");
        ensure!(
            !matches!(transfer, "smpte2084" | "arib-std-b67"),
            "HDR video kept as original; SDR conversion is skipped to avoid incorrect colors"
        );
        ensure!(
            !transfer.is_empty() && transfer != "unknown",
            "Video color metadata is missing; kept the original instead of guessing a conversion"
        );
    }
    // Include the original extension: IMG_1.HEIC.jpg cannot collide with an original IMG_1.JPG.
    let final_path = path.with_file_name(format!(
        "{}.{}",
        path.file_name()
            .context("Missing filename")?
            .to_string_lossy(),
        extension
    ));
    ensure!(
        !final_path.exists(),
        "Compatible-copy destination already exists"
    );
    let scratch = tempfile::Builder::new()
        .prefix(".itool-convert-")
        .tempdir_in(path.parent().context("Missing parent")?)?;
    let pending = scratch.path().join(format!("copy.{extension}"));
    let mut cmd = command(&ffmpeg);
    cmd.args([
        "-nostdin",
        "-v",
        "error",
        "-n",
        "-protocol_whitelist",
        "file,pipe",
        "-i",
    ])
    .arg(path);
    if extension == "jpg" {
        cmd.args(["-map", "0:v:0", "-frames:v", "1", "-q:v", "2"]);
    } else {
        cmd.args([
            "-map",
            "0:v:0",
            "-map",
            "0:a?",
            "-c:v",
            "libx264",
            "-crf",
            "18",
            "-pix_fmt",
            "yuv420p",
            "-vf",
            "scale=trunc(iw/2)*2:trunc(ih/2)*2",
            "-c:a",
            "aac",
            "-movflags",
            "+faststart",
        ]);
    }
    cmd.args(["-map_metadata", "0"]).arg(&pending);
    let output = execute(cmd, cancel).await?;
    ensure!(
        output.status.success(),
        "The converter could not decode or encode this media; the original is preserved"
    );
    ensure!(
        std::fs::metadata(&pending)?.len() > 0,
        "Converter produced an empty file"
    );
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "Cancelled; original preserved"
    );
    std::fs::hard_link(&pending, &final_path)
        .context("Could not publish compatible copy without overwriting")?;
    Ok(final_path)
}

async fn saved(path: &Path) -> Result<SavedFile> {
    let (bytes, sha256) = crate::hash_file(path).await?;
    Ok(SavedFile {
        path: path.to_owned(),
        bytes,
        sha256,
    })
}

async fn intact(file: &SavedFile) -> bool {
    if !file.path.is_file() {
        return false;
    }
    matches!(crate::hash_file(&file.path).await, Ok((n, hash)) if n == file.bytes && hash == file.sha256)
}

fn history_path(root: &Path, device: &str, version: &SourceVersion) -> PathBuf {
    let key = format!("{:x}", Sha256::digest(version.path.as_bytes()));
    root.join(".itool-history")
        .join(device)
        .join(format!("{key}.json"))
}

async fn previous(
    root: &Path,
    device: &str,
    version: &SourceVersion,
    options: &ExportOptions,
) -> Option<History> {
    let data = tokio::fs::read(history_path(root, device, version))
        .await
        .ok()?;
    let history: History = serde_json::from_slice(&data).ok()?;
    if history.source != *version
        || (options.capture_timestamps && !history.capture_timestamp_applied)
        || !intact(&history.original).await
    {
        return None;
    }
    if options.capture_timestamps {
        let expected = history.capture_time?;
        let metadata = std::fs::metadata(&history.original.path).ok()?;
        if metadata.modified().ok()? != expected {
            return None;
        }
        #[cfg(windows)]
        if metadata.created().ok()? != expected {
            return None;
        }
    }
    if conversion_extension(&version.path, options).is_some() {
        let converted = history.converted.as_ref()?;
        if !intact(converted).await {
            return None;
        }
        if options.capture_timestamps {
            let expected = history.capture_time?;
            let metadata = std::fs::metadata(&converted.path).ok()?;
            if metadata.modified().ok()? != expected {
                return None;
            }
            #[cfg(windows)]
            if metadata.created().ok()? != expected {
                return None;
            }
        }
    }
    Some(history)
}

async fn store_history(root: &Path, device: &str, history: &History) -> Result<()> {
    let path = history_path(root, device, &history.source);
    let parent = path.parent().context("History path has no parent")?;
    tokio::fs::create_dir_all(parent).await?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    std::io::Write::write_all(&mut temp, &serde_json::to_vec_pretty(history)?)?;
    temp.as_file().sync_all()?;
    temp.persist(&path).map_err(|e| e.error)?;
    Ok(())
}

pub async fn export(
    dest: &Path,
    entries: Vec<ExportEntry>,
    options: ExportOptions,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, u64, String),
) -> Result<ExportOutcome> {
    ensure!(!cancel.load(Ordering::Relaxed), "Cancelled before export");
    crate::validate_export_plan(&entries)?;
    ensure!(
        (64..=8192).contains(&options.chunk_kib),
        "Transfer chunk must be 64–8192 KiB"
    );
    ensure!(
        !dest.exists(),
        "Choose a new export folder; existing folders are never overwritten"
    );
    let parent = dest.parent().context("Choose a save location")?;
    ensure!(parent.is_dir(), "Choose an existing save location");
    if options.jpeg_copies || options.mp4_copies {
        ensure!(
            tool_path("ffmpeg").is_some(),
            "Install FFmpeg before requesting compatible copies"
        );
    }
    if options.mp4_copies {
        ensure!(
            tool_path("ffprobe").is_some(),
            "Install ffprobe before requesting MP4 copies"
        );
    }
    let (device_key, mut client) = crate::library::session().await?;
    if let Some(expected) = &options.expected_device {
        ensure!(
            *expected == device_key,
            "A different iPhone is connected. Rescan before exporting."
        );
    }
    let mut versions = Vec::new();
    let mut total = 0u64;
    for entry in &entries {
        ensure!(
            !cancel.load(Ordering::Relaxed),
            "Cancelled during preparation"
        );
        let info =
            tokio::time::timeout(Duration::from_secs(30), client.get_file_info(&entry.source))
                .await
                .context("Source metadata timed out")??;
        ensure!(
            info.st_ifmt == "S_IFREG",
            "Original is unavailable: {}",
            entry.source
        );
        total += info.size as u64;
        versions.push(SourceVersion {
            path: entry.source.clone(),
            bytes: info.size as u64,
            created: info.creation.to_string(),
            modified: info.modified.to_string(),
        });
    }
    // A Live Photo's still and clip are one skip unit. Never split the pair across batches.
    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, entry) in entries.iter().enumerate() {
        groups
            .entry(
                entry
                    .source
                    .rsplit_once('.')
                    .map(|v| v.0)
                    .unwrap_or(&entry.source)
                    .to_lowercase(),
            )
            .or_default()
            .push(i);
    }
    let mut skip = HashMap::new();
    if options.new_only {
        for group in groups.values() {
            let mut found = Vec::new();
            for i in group {
                progress(0, total, "Checking saved originals…".into());
                ensure!(
                    !cancel.load(Ordering::Relaxed),
                    "Cancelled while checking history"
                );
                if let Some(record) = previous(parent, &device_key, &versions[*i], &options).await {
                    found.push((*i, record));
                }
            }
            if found.len() == group.len() {
                skip.extend(found);
            }
        }
    }
    tokio::fs::create_dir(dest)
        .await
        .context("Could not create the new export folder")?;
    let root = std::fs::canonicalize(dest)?;
    let started = std::time::Instant::now();
    let mut outcome = ExportOutcome {
        folder: dest.to_owned(),
        copied: 0,
        skipped: 0,
        converted: 0,
        copied_bytes: 0,
        elapsed_seconds: 0.0,
        warnings: vec![],
    };
    let mut files = Vec::new();
    let mut skipped_files = Vec::new();
    let mut converted_files = Vec::new();
    let mut done = 0;
    for (i, entry) in entries.into_iter().enumerate() {
        ensure!(
            !cancel.load(Ordering::Relaxed),
            "Cancelled; completed files are preserved"
        );
        if let Some(record) = skip.remove(&i) {
            // Recheck the source after potentially lengthy destination hash checks.
            let info =
                tokio::time::timeout(Duration::from_secs(30), client.get_file_info(&entry.source))
                    .await
                    .context("Source metadata timed out")??;
            ensure!(
                info.size as u64 == versions[i].bytes
                    && info.modified.to_string() == versions[i].modified
                    && info.creation.to_string() == versions[i].created,
                "Source changed while checking export history; rescan"
            );
            outcome.skipped += 1;
            done += versions[i].bytes;
            skipped_files.push(record.original.path);
            progress(
                done,
                total,
                format!("Already saved: {}", crate::local_name(&entry.source)?),
            );
            continue;
        }
        let output = root.join(&entry.folder);
        tokio::fs::create_dir_all(&output).await?;
        let mut copied = crate::copy_one_observed(
            &mut client,
            &entry.source,
            &output,
            options.chunk_kib as usize * 1024,
            cancel,
            &mut |n| {
                progress(
                    done + n,
                    total,
                    format!(
                        "Copying {} ({}/{})",
                        entry.source.rsplit('/').next().unwrap_or("original"),
                        i + 1,
                        versions.len()
                    ),
                )
            },
        )
        .await?;
        let path = output.join(&copied.filename);
        // Do not record an old preflight signature against different bytes.
        let after =
            tokio::time::timeout(Duration::from_secs(30), client.get_file_info(&entry.source))
                .await
                .context("Source metadata recheck timed out")??;
        ensure!(
            after.size as u64 == versions[i].bytes
                && after.modified.to_string() == versions[i].modified
                && after.creation.to_string() == versions[i].created,
            "Source changed after export preparation; completed file preserved but not added to history"
        );
        if !entry.folder.is_empty() {
            copied.filename = format!("{}/{}", entry.folder, copied.filename);
        }
        done += copied.bytes;
        let original = SavedFile {
            path: path.clone(),
            bytes: copied.bytes,
            sha256: copied.sha256.clone(),
        };
        files.push(copied);
        outcome.copied += 1;
        outcome.copied_bytes += versions[i].bytes;
        crate::checkpoint(dest, &files).await?;
        let mut capture = None;
        if options.capture_timestamps {
            progress(done, total, "Applying capture date…".into());
            match capture_time(&path, cancel)
                .await
                .and_then(|(time, assumed)| {
                    stamp(&path, time)?;
                    Ok((time, assumed))
                }) {
                Ok((time, assumed)) => {
                    capture = Some(time);
                    if assumed {
                        outcome.warnings.push(format!(
                            "{}: embedded date has no timezone; used the PC timezone.",
                            path.file_name().unwrap_or_default().to_string_lossy()
                        ));
                    }
                }
                Err(e) => outcome.warnings.push(format!(
                    "{}: {e}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                )),
            }
        }
        let mut converted = None;
        if let Some(extension) = conversion_extension(&entry.source, &options) {
            progress(
                done,
                total,
                "Making a compatible copy; original is already saved…".into(),
            );
            match convert(&path, extension, cancel).await {
                Ok(converted_path) => {
                    if let Some(time) = capture {
                        if let Err(e) = stamp(&converted_path, time) {
                            outcome
                                .warnings
                                .push(format!("Compatible-copy date could not be set: {e}"));
                        }
                    }
                    let file = saved(&converted_path).await?;
                    converted_files.push(file.clone());
                    converted = Some(file);
                    outcome.converted += 1;
                }
                Err(e) => outcome.warnings.push(format!(
                    "{}: {e}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                )),
            }
        }
        let history = History {
            source: versions[i].clone(),
            original,
            converted,
            capture_timestamp_applied: capture.is_some(),
            capture_time: capture,
        };
        if let Err(e) = store_history(parent, &device_key, &history).await {
            outcome.warnings.push(format!(
                "Original saved, but history could not be updated: {e}"
            ));
        }
    }
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "Cancelled; completed originals and history are preserved"
    );
    outcome.elapsed_seconds = started.elapsed().as_secs_f64();
    let report = Report {
        transport: "Apple USB service + pure Rust AFC",
        chunk_kib: options.chunk_kib,
        options: &options,
        files: &files,
        skipped_files,
        converted_files,
        outcome: &outcome,
        transfer_seconds: started.elapsed().as_secs_f64(),
        verification: "Copied original stream SHA-256 equals destination reread; source size and mtime stable. Skips use device/path/size/creation/mtime plus a rehashed saved copy, not a new source digest. Compatible copies can lose metadata or color information.",
    };
    let mut file = tokio::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(dest.join("transfer.json"))
        .await?;
    file.write_all(&serde_json::to_vec_pretty(&report)?).await?;
    file.sync_all().await?;
    Ok(outcome)
}
