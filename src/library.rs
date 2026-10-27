//! Optional Photos metadata. All device access is read-only; SQLite runs on a private local copy.
use crate::MediaItem;
use anyhow::{Context, Result, bail, ensure};
use idevice::{
    IdeviceService,
    afc::{AfcClient, opcode::AfcFopenMode},
};
use rusqlite::{Connection, OpenFlags};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::time::Duration;
use tokio::io::AsyncWriteExt;

#[derive(Clone, Debug, Default)]
pub struct AssetMetadata {
    pub favorite: Option<bool>,
    pub albums: Vec<String>,
    pub uuid: Option<String>,
}

pub struct LibrarySnapshot {
    pub items: Vec<MediaItem>,
    pub device_key: String,
    pub assets: BTreeMap<String, AssetMetadata>,
    pub albums_available: bool,
    pub favorites_available: bool,
    pub metadata_status: String,
}

pub(crate) async fn session() -> Result<(String, AfcClient)> {
    let provider = tokio::time::timeout(Duration::from_secs(15), crate::provider())
        .await
        .context("Device discovery timed out")??;
    let device_key = format!("{:x}", Sha256::digest(provider.udid.as_bytes()));
    let client = tokio::time::timeout(Duration::from_secs(30), AfcClient::connect(&provider))
        .await
        .context("Device connection timed out")??;
    Ok((device_key, client))
}

pub async fn scan(mut status: impl FnMut(String)) -> Result<LibrarySnapshot> {
    let (device_key, mut client) = session().await?;
    let items = crate::scan_with_client(&mut client, &mut status).await?;
    status("Reading albums and Favorites, when available…".into());
    let metadata = tokio::time::timeout(Duration::from_secs(90), read_metadata(&mut client)).await;
    let (assets, albums_available, favorites_available, metadata_status) = match metadata {
        Ok(Ok((assets, albums, favorites))) => (assets, albums, favorites,
            "Album metadata read from a local Photos database snapshot. Only accessible DCIM originals are listed.".into()),
        Ok(Err(_)) => (BTreeMap::new(), false, false,
            "Albums and Favorites unavailable: iOS did not expose a readable, supported Photos database. Your accessible media can still be copied.".into()),
        Err(_) => (BTreeMap::new(), false, false,
            "Album lookup timed out. Rescan to retry; your accessible media can still be copied.".into()),
    };
    Ok(LibrarySnapshot {
        items,
        device_key,
        assets,
        albums_available,
        favorites_available,
        metadata_status,
    })
}

async fn signature(client: &mut AfcClient, path: &str) -> Result<Option<(usize, String)>> {
    let info = tokio::time::timeout(Duration::from_secs(5), client.get_file_info(path))
        .await
        .context("Metadata lookup timed out")?;
    match info {
        Ok(info) => Ok(Some((
            info.size,
            format!("{}|{}", info.modified, info.creation),
        ))),
        Err(error) => {
            // Only an explicit not-found result means an optional WAL is absent.
            let message = error.to_string().to_lowercase();
            if message.contains("not found")
                || message.contains("objectnotfound")
                || message.contains("object not found")
            {
                Ok(None)
            } else {
                Err(error.into())
            }
        }
    }
}

async fn download(
    client: &mut AfcClient,
    remote: &str,
    local: &std::path::Path,
    size: usize,
) -> Result<()> {
    ensure!(
        size <= 512 * 1024 * 1024,
        "Photos metadata exceeds the local snapshot limit"
    );
    let mut input = client.open(remote, AfcFopenMode::RdOnly).await?;
    let result: Result<()> = async {
        let mut output = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(local)
            .await?;
        let mut remaining = size;
        while remaining > 0 {
            let bytes = tokio::time::timeout(
                Duration::from_secs(10),
                input.read_n(remaining.min(4 * 1024 * 1024)),
            )
            .await
            .context("Photos metadata read timed out")??;
            ensure!(
                !bytes.is_empty() && bytes.len() <= remaining,
                "Incomplete Photos snapshot"
            );
            output.write_all(&bytes).await?;
            remaining -= bytes.len();
        }
        output.sync_all().await?;
        Ok(())
    }
    .await;
    let closed = input.close().await;
    result?;
    closed?;
    Ok(())
}

async fn read_metadata(
    client: &mut AfcClient,
) -> Result<(BTreeMap<String, AssetMetadata>, bool, bool)> {
    const DATABASE: &str = "/PhotoData/Photos.sqlite";
    const WAL: &str = "/PhotoData/Photos.sqlite-wal";
    let before = signature(client, DATABASE)
        .await?
        .context("Photos database unavailable")?;
    let wal_before = signature(client, WAL).await?;
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("Photos.sqlite");
    download(client, DATABASE, &path, before.0).await?;
    if let Some((size, _)) = &wal_before {
        download(
            client,
            WAL,
            &scratch.path().join("Photos.sqlite-wal"),
            *size,
        )
        .await?;
    }
    ensure!(
        signature(client, DATABASE).await?.as_ref() == Some(&before)
            && signature(client, WAL).await? == wal_before,
        "Photos changed during snapshot; rescan to retry"
    );
    // SQLite can recover the copied WAL locally. Never open the remote database or its shared memory.
    let db = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    db.pragma_update(None, "trusted_schema", false)?;
    db.pragma_update(None, "query_only", true)?;
    let check: String = db.query_row("PRAGMA quick_check(1)", [], |r| r.get(0))?;
    ensure!(check == "ok", "Photos snapshot is inconsistent");
    parse_database(&db)
}

fn ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}
fn columns(db: &Connection, table: &str) -> Result<Vec<String>> {
    Ok(db
        .prepare(&format!("PRAGMA table_info({})", ident(table)))?
        .query_map([], |r| r.get(1))?
        .collect::<std::result::Result<_, _>>()?)
}

fn parse_database(db: &Connection) -> Result<(BTreeMap<String, AssetMetadata>, bool, bool)> {
    let tables: Vec<String> = db
        .prepare("SELECT name FROM sqlite_master WHERE type='table'")?
        .query_map([], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    let table = ["ZASSET", "ZGENERICASSET"]
        .into_iter()
        .find(|t| tables.iter().any(|v| v == t))
        .context("Unsupported Photos asset schema")?;
    let cols = columns(db, table)?;
    ensure!(
        ["Z_PK", "ZFILENAME", "ZDIRECTORY"]
            .iter()
            .all(|c| cols.iter().any(|v| v == c)),
        "Unsupported Photos paths"
    );
    let favorites = cols.iter().any(|c| c == "ZFAVORITE");
    let uuid = if cols.iter().any(|c| c == "ZUUID") {
        "ZUUID"
    } else {
        "NULL"
    };
    let favorite = if favorites { "ZFAVORITE" } else { "NULL" };
    let sql = format!(
        "SELECT Z_PK, ZDIRECTORY, ZFILENAME, {favorite}, {uuid} FROM {}",
        ident(table)
    );
    let mut paths = HashMap::<i64, String>::new();
    let mut assets = BTreeMap::new();
    let mut statement = db.prepare(&sql)?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let id: i64 = row.get(0)?;
        let directory: Option<String> = row.get(1)?;
        let filename: Option<String> = row.get(2)?;
        let (Some(directory), Some(filename)) = (directory, filename) else {
            continue;
        };
        let directory = directory.trim_start_matches('/');
        let path = format!("/{directory}/{filename}");
        if crate::local_name(&path).is_err() {
            continue;
        }
        paths.insert(id, path.clone());
        assets.insert(
            path,
            AssetMetadata {
                favorite: row.get::<_, Option<i64>>(3)?.map(|v| v != 0),
                uuid: row.get(4)?,
                albums: vec![],
            },
        );
    }
    if !tables.iter().any(|t| t == "ZGENERICALBUM") {
        return Ok((assets, false, favorites));
    }
    let album_cols = columns(db, "ZGENERICALBUM")?;
    if !album_cols.iter().any(|c| c == "ZTITLE") {
        return Ok((assets, false, favorites));
    }
    let mut supported = false;
    // Core Data join-table entity numbers vary with the iOS schema. Discover columns rather than hardcoding numbers.
    for join in tables
        .iter()
        .filter(|t| t.starts_with("Z_") && t.ends_with("ASSETS"))
    {
        let cols = columns(db, join)?;
        let a = cols
            .iter()
            .find(|c| c.starts_with("Z_") && c.ends_with("ALBUMS"));
        let b = cols
            .iter()
            .find(|c| c.starts_with("Z_") && c.ends_with("ASSETS"));
        let (Some(a), Some(b)) = (a, b) else { continue };
        let sql = format!(
            "SELECT j.{}, a.ZTITLE FROM {} j JOIN ZGENERICALBUM a ON j.{}=a.Z_PK WHERE a.ZTITLE IS NOT NULL AND a.ZTITLE<>''",
            ident(b),
            ident(join),
            ident(a)
        );
        let mut statement = db.prepare(&sql)?;
        let rows =
            statement.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
        for row in rows {
            let (id, title) = row?;
            if let Some(path) = paths.get(&id) {
                if let Some(asset) = assets.get_mut(path) {
                    if !asset.albums.contains(&title) {
                        asset.albums.push(title);
                    }
                }
            }
        }
        supported = true;
    }
    if assets.is_empty() {
        bail!("Photos database contains no supported DCIM paths");
    }
    Ok((assets, supported, favorites))
}
