use anyhow::{Context, Result, bail, ensure};
use clap::{Parser, Subcommand};
use idevice::{
    IdeviceService,
    afc::{AfcClient, opcode::AfcFopenMode},
    provider::UsbmuxdProvider,
    usbmuxd::{Connection, UsbmuxdAddr},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    time::Instant,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Parser)]
#[command(
    version,
    about = "Copy accessible original media from a trusted iPhone. Never writes to the phone."
)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Show connected USB devices without revealing device identifiers.
    Devices,
    /// List a directory in the phone's exposed media filesystem.
    List {
        #[arg(default_value = "/DCIM")]
        path: String,
    },
    /// Locate exact filenames in DCIM (does not copy files).
    Find {
        #[arg(required = true)]
        names: Vec<String>,
    },
    /// Export exact device paths into a NEW destination directory.
    Export {
        #[arg(long)]
        dest: PathBuf,
        #[arg(long, default_value_t = 1024, value_parser = clap::value_parser!(u32).range(64..=8192))]
        chunk_kib: u32,
        #[arg(required = true)]
        paths: Vec<String>,
    },
    /// Check an exported directory against a 3uTools baseline manifest.
    Verify {
        #[arg(long)]
        baseline: PathBuf,
        #[arg(long)]
        dir: PathBuf,
    },
}

#[derive(Serialize, Deserialize)]
pub struct CopiedFile {
    source_path: String,
    filename: String,
    bytes: u64,
    sha256: String,
    seconds: f64,
}

#[derive(Serialize)]
pub struct TransferReport {
    transport: &'static str,
    chunk_kib: u32,
    files: Vec<CopiedFile>,
    total_bytes: u64,
    transfer_seconds: f64,
    mib_per_second: f64,
    verification: &'static str,
}

async fn provider() -> Result<UsbmuxdProvider> {
    // Explicit local socket: do not follow a remote USBMUXD_SOCKET_ADDRESS.
    let addr = UsbmuxdAddr::default();
    let mut mux = addr
        .connect(1)
        .await
        .context("Apple USB service unavailable on localhost:27015")?;
    let devices = mux
        .get_devices()
        .await
        .context("Cannot enumerate Apple USB devices")?;
    let usb: Vec<_> = devices
        .into_iter()
        .filter(|d| d.connection_type == Connection::Usb)
        .collect();
    ensure!(
        usb.len() == 1,
        "Expected exactly one USB iPhone; found {}. Connect and unlock one phone.",
        usb.len()
    );
    Ok(usb[0].to_provider(addr, "ITool"))
}

pub async fn afc() -> Result<AfcClient> {
    let provider = tokio::time::timeout(std::time::Duration::from_secs(15), provider())
        .await
        .context("Device discovery timed out")??;
    tokio::time::timeout(
        std::time::Duration::from_secs(30),
        AfcClient::connect(&provider),
    )
    .await
    .context("Device connection timed out")?
    .context("Cannot open read-only media session. Unlock the phone and confirm Trust if prompted.")
}

fn local_name(remote: &str) -> Result<String> {
    ensure!(
        remote.starts_with("/DCIM/"),
        "Only /DCIM/ media paths can be exported"
    );
    ensure!(
        !remote.split('/').any(|s| s == ".." || s == "."),
        "Relative path components are not allowed"
    );
    let name = remote.rsplit('/').next().unwrap_or_default();
    ensure!(
        !["completed.json", "completed.json.tmp", "transfer.json"]
            .contains(&name.to_ascii_lowercase().as_str()),
        "Filename reserved for transfer reports"
    );
    ensure!(
        !name.is_empty()
            && !name.chars().any(char::is_control)
            && !name.contains(['\\', ':', '<', '>', '"', '|', '?', '*'])
            && !name.ends_with(['.', ' ']),
        "Unsafe destination filename"
    );
    ensure!(
        !matches!(
            name.split('.')
                .next()
                .unwrap_or("")
                .to_ascii_uppercase()
                .as_str(),
            "CON"
                | "PRN"
                | "AUX"
                | "NUL"
                | "COM1"
                | "COM2"
                | "COM3"
                | "COM4"
                | "COM5"
                | "COM6"
                | "COM7"
                | "COM8"
                | "COM9"
                | "LPT1"
                | "LPT2"
                | "LPT3"
                | "LPT4"
                | "LPT5"
                | "LPT6"
                | "LPT7"
                | "LPT8"
                | "LPT9"
        ),
        "Reserved Windows filename"
    );
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    ensure!(
        !["COM¹", "COM²", "COM³", "LPT¹", "LPT²", "LPT³"].contains(&stem.as_str()),
        "Reserved Windows filename"
    );
    Ok(name.to_owned())
}

fn baseline_rows(data: &serde_json::Value) -> Result<Vec<(String, u64, String)>> {
    let rows = data["files"]
        .as_array()
        .context("Baseline has no files array")?;
    ensure!(!rows.is_empty(), "Baseline files array is empty");
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for row in rows {
        let name = row["filename"]
            .as_str()
            .context("Missing baseline filename")?;
        let name = name.find("IMG_").map(|i| &name[i..]).unwrap_or(name);
        let name = local_name(&format!("/DCIM/manifest/{name}"))?;
        ensure!(
            seen.insert(name.to_ascii_lowercase()),
            "Duplicate normalized baseline filename"
        );
        let bytes = row["bytes"].as_u64().context("Invalid baseline size")?;
        let hash = row["sha256"].as_str().context("Invalid baseline digest")?;
        ensure!(
            hash.len() == 64 && hash.bytes().all(|c| c.is_ascii_hexdigit()),
            "Invalid SHA-256"
        );
        result.push((name, bytes, hash.to_ascii_lowercase()));
    }
    Ok(result)
}

pub async fn checkpoint(dest: &Path, files: &[CopiedFile]) -> Result<()> {
    let pending = dest.join("completed.json.tmp");
    let mut out = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&pending)
        .await?;
    out.write_all(&serde_json::to_vec_pretty(files)?).await?;
    out.sync_all().await?;
    drop(out);
    tokio::fs::rename(pending, dest.join("completed.json")).await?;
    Ok(())
}

async fn hash_file(path: &Path) -> Result<(u64, String)> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hash = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    let mut bytes = 0;
    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
        bytes += n as u64;
    }
    Ok((bytes, format!("{:x}", hash.finalize())))
}

pub async fn copy_one(
    client: &mut AfcClient,
    remote: &str,
    dest: &Path,
    chunk: usize,
) -> Result<CopiedFile> {
    copy_one_observed(
        client,
        remote,
        dest,
        chunk,
        &std::sync::atomic::AtomicBool::new(false),
        &mut |_| {},
    )
    .await
}

async fn copy_one_observed(
    client: &mut AfcClient,
    remote: &str,
    dest: &Path,
    chunk: usize,
    cancel: &std::sync::atomic::AtomicBool,
    progress: &mut impl FnMut(u64),
) -> Result<CopiedFile> {
    let name = local_name(remote)?;
    let info = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        client.get_file_info(remote),
    )
    .await
    .context("Source metadata timed out")??;
    ensure!(
        info.st_ifmt == "S_IFREG",
        "Source is not a regular file: {remote}"
    );
    let final_path = dest.join(&name);
    let partial = dest.join(format!("{name}.partial"));
    ensure!(
        !final_path.exists(),
        "Refusing to replace {}",
        final_path.display()
    );
    let mut out = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&partial)
        .await?;
    let mut source = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        client.open(remote, AfcFopenMode::RdOnly),
    )
    .await
    .context("Source open timed out")??;
    let start = Instant::now();
    let transfer = async {
        let mut hash = Sha256::new();
        let mut bytes = 0u64;
        while bytes < info.size as u64 {
            ensure!(
                !cancel.load(std::sync::atomic::Ordering::Relaxed),
                "Cancelled; completed files and incomplete .partial retained"
            );
            let wanted = chunk.min(info.size - bytes as usize);
            let data =
                tokio::time::timeout(std::time::Duration::from_secs(60), source.read_n(wanted))
                    .await
                    .context("Device read stalled for 60 seconds; partial retained")??;
            ensure!(
                !data.is_empty() && data.len() <= wanted,
                "Unexpected source length for {remote}"
            );
            out.write_all(&data).await?;
            hash.update(&data);
            bytes += data.len() as u64;
            progress(bytes);
        }
        out.sync_all().await?;
        Ok::<_, anyhow::Error>((bytes, format!("{:x}", hash.finalize())))
    }
    .await;
    let closed = tokio::time::timeout(std::time::Duration::from_secs(15), source.close())
        .await
        .context("Closing source timed out")?;
    let (bytes, hash) = transfer?;
    closed?;
    drop(out);
    let after = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        client.get_file_info(remote),
    )
    .await
    .context("Source metadata recheck timed out")??;
    ensure!(
        after.size == info.size && after.modified == info.modified,
        "Source changed while copying; partial retained"
    );
    let (disk_bytes, disk_hash) = hash_file(&partial).await?;
    ensure!(
        !cancel.load(std::sync::atomic::Ordering::Relaxed),
        "Cancelled; incomplete .partial retained"
    );
    ensure!(
        disk_bytes == bytes && disk_hash == hash,
        "Destination reread failed; partial retained"
    );
    // A hard link publishes the completed file atomically and cannot overwrite an existing path.
    tokio::fs::hard_link(&partial, &final_path)
        .await
        .context("Finalization requires a filesystem supporting hard links (use NTFS)")?;
    tokio::fs::remove_file(&partial).await?;
    let seconds = start.elapsed().as_secs_f64();
    eprintln!("Copied {name}: {bytes} bytes, {seconds:.3}s (includes disk verification)");
    Ok(CopiedFile {
        source_path: remote.into(),
        filename: name,
        bytes,
        sha256: hash,
        seconds,
    })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MediaItem {
    pub path: String,
    pub name: String,
    pub resources: Vec<String>,
    pub kind: &'static str,
    pub size: u64,
    pub timestamp: i64,
    pub date_str: String,
    pub month_key: String,
    pub month_label: String,
    pub week_label: String,
    pub date_label: String,
}

fn is_image(name: &str) -> bool {
    [
        ".heic", ".heif", ".jpg", ".jpeg", ".png", ".dng", ".gif", ".webp",
    ]
    .iter()
    .any(|ext| name.to_ascii_lowercase().ends_with(ext))
}

pub async fn scan_media(mut status: impl FnMut(String)) -> Result<Vec<MediaItem>> {
    let mut client = afc().await?;
    let mut folders =
        tokio::time::timeout(std::time::Duration::from_secs(30), client.list_dir("/DCIM"))
            .await
            .context("DCIM listing timed out")??;
    folders.retain(|f| !f.starts_with('.'));
    folders.sort();
    folders.reverse();
    let mut items = Vec::new();
    let mut total_count = 0usize;
    for folder in &folders {
        status(format!("Cataloging {folder}… ({total_count} media items found)"));
        let names = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            client.list_dir(format!("/DCIM/{folder}")),
        )
        .await
        .context("Media folder listing timed out")??;
        let lower: std::collections::HashSet<_> =
            names.iter().map(|n| n.to_ascii_lowercase()).collect();
        for name in &names {
            let image = is_image(name);
            let video = [".mov", ".mp4", ".m4v"]
                .iter()
                .any(|ext| name.to_ascii_lowercase().ends_with(ext));
            if !image && !video {
                continue;
            }
            let stem = name.rsplit_once('.').map(|p| p.0).unwrap_or(name);
            if name.to_ascii_lowercase().ends_with(".mov")
                && ["heic", "heif", "jpg", "jpeg", "png", "dng"]
                    .iter()
                    .any(|ext| lower.contains(&format!("{stem}.{ext}").to_ascii_lowercase()))
            {
                continue;
            }
            let path = format!("/DCIM/{folder}/{name}");
            let mut resources = vec![path.clone()];
            let live = image && lower.contains(&format!("{stem}.mov").to_ascii_lowercase());
            if live
                && let Some(mov) = names
                    .iter()
                    .find(|n| n.eq_ignore_ascii_case(&format!("{stem}.mov")))
            {
                resources.push(format!("/DCIM/{folder}/{mov}"));
            }

            let (mut size, timestamp, date_str, month_key, month_label, week_label, date_label) =
                match client.get_file_info(&path).await {
                    Ok(info) => {
                        let dt = info.creation;
                        let ts = dt.and_utc().timestamp();
                        let date_str = dt.format("%Y-%m-%d").to_string();
                        let month_key = dt.format("%Y-%m").to_string();
                        let month_label = dt.format("%B %Y").to_string();
                        let week_label = format!("Week {}, {}", dt.format("%U"), dt.format("%b %Y"));
                        let date_label = dt.format("%a, %b %e, %Y").to_string();
                        (info.size as u64, ts, date_str, month_key, month_label, week_label, date_label)
                    }
                    Err(_) => {
                        (0, 0, "Unknown Date".into(), "Unknown".into(), "Unknown Month".into(), "Unknown Week".into(), "Unknown Date".into())
                    }
                };

            if live && resources.len() > 1 {
                if let Ok(mov_info) = client.get_file_info(&resources[1]).await {
                    size += mov_info.size as u64;
                }
            }

            total_count += 1;
            if total_count % 150 == 0 {
                status(format!("Cataloging {folder}… ({total_count} media items found)"));
            }

            items.push(MediaItem {
                path,
                name: name.clone(),
                resources,
                kind: if live {
                    "Photo + MOV"
                } else if video {
                    "Video"
                } else {
                    "Photo"
                },
                size,
                timestamp,
                date_str,
                month_key,
                month_label,
                week_label,
                date_label,
            });
        }
    }
    items.sort_by(|a, b| b.timestamp.cmp(&a.timestamp).then_with(|| b.path.cmp(&a.path)));
    Ok(items)
}

pub async fn read_thumbnail(client: &mut AfcClient, path: &str) -> Result<Vec<u8>> {
    local_name(path)?;
    let thumb = format!("/PhotoData/Thumbnails/V2{path}/5005.JPG");
    let info = tokio::time::timeout(
        std::time::Duration::from_secs(8),
        client.get_file_info(&thumb),
    )
    .await
    .context("Preview metadata timed out")??;
    ensure!(
        info.size > 0 && info.size <= 2 * 1024 * 1024,
        "Preview unavailable or too large"
    );
    let mut source = tokio::time::timeout(
        std::time::Duration::from_secs(8),
        client.open(&thumb, AfcFopenMode::RdOnly),
    )
    .await
    .context("Preview open timed out")??;
    let data = tokio::time::timeout(std::time::Duration::from_secs(8), source.read_n(info.size))
        .await
        .context("Preview read timed out")?;
    let closed = tokio::time::timeout(std::time::Duration::from_secs(5), source.close())
        .await
        .context("Preview close timed out")?;
    let data = data?;
    closed?;
    ensure!(data.len() == info.size, "Incomplete preview");
    Ok(data)
}

pub async fn export_media(
    dest: &Path,
    paths: Vec<String>,
    cancel: &std::sync::atomic::AtomicBool,
    mut progress: impl FnMut(u64, u64, String),
) -> Result<PathBuf> {
    ensure!(
        !cancel.load(std::sync::atomic::Ordering::Relaxed),
        "Cancelled before export"
    );
    ensure!(!paths.is_empty(), "Select at least one photo or video");
    let mut names = std::collections::HashSet::new();
    for path in &paths {
        ensure!(
            !cancel.load(std::sync::atomic::Ordering::Relaxed),
            "Cancelled during preparation"
        );
        ensure!(
            names.insert(local_name(path)?.to_ascii_lowercase()),
            "Selection contains duplicate filenames from different folders; export those in separate batches"
        );
    }
    let mut client = afc().await?;
    let mut total = 0u64;
    for path in &paths {
        ensure!(
            !cancel.load(std::sync::atomic::Ordering::Relaxed),
            "Cancelled during preparation"
        );
        total += tokio::time::timeout(
            std::time::Duration::from_secs(30),
            client.get_file_info(path),
        )
        .await
        .context("Media size lookup timed out")??
        .size as u64;
    }
    tokio::fs::create_dir(dest)
        .await
        .context("Export needs a new folder on an NTFS drive; choose an existing parent")?;
    let mut files = Vec::new();
    let mut completed = 0u64;
    let start = Instant::now();
    for path in paths {
        ensure!(
            !cancel.load(std::sync::atomic::Ordering::Relaxed),
            "Cancelled; completed files retained"
        );
        let name = local_name(&path)?;
        let copied = copy_one_observed(&mut client, &path, dest, 1024 * 1024, cancel, &mut |n| {
            progress(completed + n, total, name.clone())
        })
        .await?;
        completed += copied.bytes;
        files.push(copied);
    }
    let elapsed = start.elapsed().as_secs_f64();
    eprintln!(
        "Export complete: {} files, {:.1} MiB in {:.1}s ({:.2} MiB/s)",
        files.len(),
        completed as f64 / 1_048_576.0,
        elapsed,
        completed as f64 / 1_048_576.0 / elapsed,
    );
    // Return a synthetic path pointing at the export directory itself
    Ok(dest.to_path_buf())
}

async fn run(args: Args) -> Result<()> {
    match args.command {
        Command::Devices => {
            let addr = UsbmuxdAddr::default();
            let mut mux = addr.connect(1).await?;
            let devices = mux.get_devices().await?;
            for (i, d) in devices.iter().enumerate() {
                println!("Device {}: {:?}", i + 1, d.connection_type);
            }
            if devices.is_empty() {
                bail!("No iPhone detected");
            }
        }
        Command::List { path } => {
            let mut client = afc().await?;
            let mut names = client.list_dir(path).await?;
            names.sort();
            for name in names {
                println!("{name}");
            }
        }
        Command::Find { names } => {
            let mut client = afc().await?;
            let wanted: Vec<_> = names.iter().map(|n| n.to_ascii_lowercase()).collect();
            let mut queue = VecDeque::from(["/DCIM".to_owned()]);
            let mut found = Vec::new();
            while let Some(dir) = queue.pop_front() {
                for name in client.list_dir(&dir).await? {
                    if name == "." || name == ".." || name.is_empty() {
                        continue;
                    }
                    let path = format!("{dir}/{name}");
                    if wanted.contains(&name.to_ascii_lowercase()) {
                        println!("{path}");
                        found.push(name.to_ascii_lowercase());
                    } else if dir == "/DCIM" {
                        let info = client.get_file_info(&path).await?;
                        if info.st_ifmt == "S_IFDIR" {
                            queue.push_back(path);
                        }
                    }
                }
            }
            for name in wanted {
                ensure!(found.contains(&name), "Not found in exposed DCIM: {name}");
            }
        }
        Command::Export {
            dest,
            chunk_kib,
            paths,
        } => {
            let mut seen = std::collections::HashSet::new();
            for path in &paths {
                let name = local_name(path)?.to_ascii_lowercase();
                ensure!(
                    seen.insert(name.clone()) && seen.insert(format!("{name}.partial")),
                    "Duplicate output filename in selection"
                );
                ensure!(name != "completed.json.tmp", "Reserved checkpoint filename");
            }
            // A new directory per run prevents accidental overwrites and mixed benchmark results.
            tokio::fs::create_dir(&dest)
                .await
                .context("Destination must be a NEW directory inside an existing parent")?;
            let mut client = afc().await?;
            let start = Instant::now();
            let mut files = Vec::new();
            for remote in paths {
                files.push(copy_one(&mut client, &remote, &dest, chunk_kib as usize * 1024).await?);
                checkpoint(&dest, &files).await?;
            }
            let elapsed = start.elapsed().as_secs_f64();
            let bytes = files.iter().map(|f| f.bytes).sum();
            let report = TransferReport {
                transport: "Apple USB service + pure Rust AFC",
                chunk_kib,
                files,
                total_bytes: bytes,
                transfer_seconds: elapsed,
                mib_per_second: bytes as f64 / 1048576.0 / elapsed,
                verification: "stream SHA-256 matches destination reread; source size and mtime stable; no independent source digest",
            };
            let json = serde_json::to_string_pretty(&report)?;
            tokio::fs::write(dest.join("transfer.json"), &json).await?;
            println!("{json}");
        }
        Command::Verify { baseline, dir } => {
            let data: serde_json::Value =
                serde_json::from_slice(&tokio::fs::read(baseline).await?)?;
            let rows = baseline_rows(&data)?;
            let mut unmatched = 0;
            for (device_name, expected_bytes, expected_hash) in &rows {
                let (bytes, hash) = hash_file(&dir.join(device_name)).await?;
                let matches = *expected_bytes == bytes && expected_hash == &hash;
                println!(
                    "{} {device_name}",
                    if matches { "MATCH" } else { "MISMATCH" }
                );
                if !matches {
                    unmatched += 1;
                }
            }
            ensure!(unmatched == 0, "{unmatched} files differ from baseline");
            println!(
                "All {} files match the 3uTools baseline byte-for-byte.",
                rows.len()
            );
        }
    }
    Ok(())
}

#[tokio::main]
pub async fn cli_main() -> Result<()> {
    let args = Args::parse();
    // Active bulk transfers have no total duration limit; each source read has an idle deadline.
    if matches!(args.command, Command::Export { .. }) {
        run(args).await
    } else {
        tokio::time::timeout(std::time::Duration::from_secs(120), run(args))
            .await
            .context("Device discovery/listing timed out")?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn destination_paths_stay_inside_export_directory() {
        assert_eq!(
            local_name("/DCIM/100APPLE/IMG_1.HEIC").unwrap(),
            "IMG_1.HEIC"
        );
        for path in [
            "/DCIM/../secret",
            "/PhotoData/a",
            "/DCIM/a/b:c",
            "/DCIM/a/CON.mov",
            "/DCIM/a/x\\y",
            "/DCIM/a/a.",
            "/DCIM/a/TRANSFER.JSON",
        ] {
            assert!(local_name(path).is_err(), "{path}");
        }
    }
    #[test]
    fn manifest_rejects_empty_duplicate_and_invalid_hash() {
        assert!(baseline_rows(&serde_json::json!({"files":[]})).is_err());
        let row = serde_json::json!({"filename":"IMG_1.MOV", "bytes":1, "sha256":"a".repeat(64)});
        assert!(baseline_rows(&serde_json::json!({"files":[row.clone(),row.clone()]})).is_err());
        assert!(
            baseline_rows(
                &serde_json::json!({"files":[{"filename":"IMG_1.MOV","bytes":1,"sha256":"bad"}]})
            )
            .is_err()
        );
        assert_eq!(
            baseline_rows(&serde_json::json!({"files":[row]}))
                .unwrap()
                .len(),
            1
        );
    }
}