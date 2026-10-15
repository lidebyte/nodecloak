//! Closed-profile Chromium localStorage, without loading website documents.
use super::browser_session::{origin, SiteKey};
use crate::profiles;
use rusqlite::OptionalExtension;
use rusty_leveldb::{LdbIterator, Options, WriteBatch, DB};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path};

const INVALID: &str = "网站本地存储格式无效，保留本机数据";
const LIMIT: usize = 8 << 20;

// Keep document-based handling for IndexedDB and partitioned namespaces. An
// unknown bucket layout is deliberately conservative, never classified empty.
pub(super) fn eligible(path: &Path, key: &SiteKey) -> Result<bool, String> {
    if key.partition_key.is_some() || origin(&key.origin)? != key.origin {
        return Ok(false);
    }
    profiles::guard_tree(path)?;
    // Chromium's newer SQLite backend is named LocalStorage (without a space).
    // It may coexist with an obsolete LevelDB during migration.
    if path.join("Default/LocalStorage").exists() || path.join("Default/LocalStorage-wal").exists()
    {
        return Ok(false);
    }
    let local = path.join("Default/Local Storage");
    if local.exists() {
        for entry in std::fs::read_dir(local).map_err(|_| INVALID)? {
            let entry = entry.map_err(|_| INVALID)?;
            if entry.file_name().to_string_lossy().contains("sqlite") {
                return Ok(false);
            }
        }
    }
    let legacy = path.join("Default/IndexedDB");
    if legacy.exists() {
        for entry in std::fs::read_dir(legacy).map_err(|_| INVALID)? {
            let entry = entry.map_err(|_| INVALID)?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let raw = name.split(".indexeddb.").next().unwrap_or("");
            let Some((scheme, rest)) = raw.split_once('_') else {
                return Ok(false);
            };
            if !matches!(scheme, "http" | "https") {
                continue;
            }
            let Some((host, port)) = rest.rsplit_once('_') else {
                return Ok(false);
            };
            let value = if port == "0" {
                format!("{scheme}://{host}")
            } else {
                format!("{scheme}://{host}:{port}")
            };
            if origin(&value).map(|o| o == key.origin).unwrap_or(true) {
                return Ok(false);
            }
        }
    }
    let buckets = path.join("Default/WebStorage");
    if buckets.exists() {
        for entry in std::fs::read_dir(buckets).map_err(|_| INVALID)? {
            let entry = entry.map_err(|_| INVALID)?;
            profiles::guard_tree(&entry.path())?;
            if entry.file_type().map_err(|_| INVALID)?.is_dir() {
                for child in std::fs::read_dir(entry.path()).map_err(|_| INVALID)? {
                    let child = child.map_err(|_| INVALID)?;
                    // Future indexed database layouts must not be mistaken for
                    // localStorage-only state, even in a different bucket.
                    if child
                        .file_name()
                        .to_string_lossy()
                        .to_ascii_lowercase()
                        .contains("indexed")
                    {
                        let id = entry
                            .file_name()
                            .to_string_lossy()
                            .parse::<i64>()
                            .ok()
                            .filter(|id| *id > 0);
                        let Some(id) = id else { return Ok(false) };
                        let mut known_other = false;
                        for relative in ["Default/WebStorage/QuotaManager", "Default/QuotaManager"]
                        {
                            let quota = path.join(relative);
                            if !quota.exists() {
                                continue;
                            }
                            let db = super::session_storage::sqlite(&quota)?;
                            let owner: Option<String> = db
                                .query_row(
                                    "SELECT storage_key FROM buckets WHERE id=?1",
                                    [id],
                                    |row| row.get(0),
                                )
                                .optional()
                                .map_err(|_| INVALID)?;
                            if let Some(owner) = owner {
                                let raw = super::session_storage::chromium_web_key(&owner);
                                match raw {
                                    Ok(Some(owner)) if owner == *key => return Ok(false),
                                    Ok(_) => known_other = true,
                                    Err(_)
                                        if !owner.starts_with("http://")
                                            && !owner.starts_with("https://") =>
                                    {
                                        known_other = true
                                    }
                                    Err(error) => return Err(error),
                                }
                            }
                        }
                        if !known_other {
                            return Ok(false);
                        }
                    }
                }
            }
        }
    }
    Ok(true)
}
fn prefix(key: &SiteKey) -> Result<Vec<u8>, String> {
    if key.partition_key.is_some() || origin(&key.origin)? != key.origin {
        return Err(INVALID.into());
    }
    Ok(format!("_{}\0", key.origin).into_bytes())
}
fn open(path: &Path, create: bool) -> Result<DB, String> {
    profiles::guard_tree(path)?;
    let existing = path.join("CURRENT").exists();
    let mut options = Options::default();
    options.create_if_missing = create;
    let mut db = DB::open(path, options).map_err(|_| "无法打开网站本地存储，保留本机数据")?;
    // On a real Chromium SST, point lookup reported absent while iteration
    // returned VERSION=1. Use the same bounded iterator as website records.
    let mut iter = db.new_iter().map_err(|_| INVALID)?;
    let mut version = None;
    let mut count = 0;
    while let Some((key, value)) = iter.next() {
        count += 1;
        if count > 100000 {
            return Err(INVALID.into());
        }
        if key == b"VERSION" {
            version = Some(value);
            break;
        }
    }
    drop(iter);
    if let Some(version) = version {
        if version != b"1" {
            return Err("网站本地存储版本不支持，保留本机数据".into());
        }
    } else if existing || !create {
        return Err("网站本地存储版本标记缺失，保留本机数据".into());
    }
    Ok(db)
}
fn decode(bytes: &[u8]) -> Result<String, String> {
    if bytes.len() > LIMIT {
        return Err(INVALID.into());
    }
    match bytes.split_first() {
        Some((1, value)) => Ok(value.iter().map(|b| char::from(*b)).collect()),
        Some((0, value)) if value.len() % 2 == 0 => {
            let units = value
                .chunks_exact(2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .collect::<Vec<_>>();
            String::from_utf16(&units).map_err(|_| INVALID.into())
        }
        _ => Err(INVALID.into()),
    }
}
fn encode(value: &str) -> Vec<u8> {
    if value.chars().all(|c| u32::from(c) <= 255) {
        std::iter::once(1)
            .chain(value.chars().map(|c| c as u8))
            .collect()
    } else {
        std::iter::once(0)
            .chain(value.encode_utf16().flat_map(u16::to_le_bytes))
            .collect()
    }
}
fn entries(db: &mut DB, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>, String> {
    let mut iter = db.new_iter().map_err(|_| INVALID)?;
    let mut found = Vec::new();
    let mut count = 0;
    let mut bytes = 0;
    while let Some((key, value)) = iter.next() {
        count += 1;
        if count > 100000 {
            return Err(INVALID.into());
        }
        if key.starts_with(prefix) {
            bytes += key.len() + value.len();
            if bytes > LIMIT {
                return Err("网站本地存储超过同步限制".into());
            }
            found.push((key, value));
        }
    }
    Ok(found)
}
pub(super) fn read(path: &Path, key: &SiteKey) -> Result<Value, String> {
    let prefix = prefix(key)?;
    let dir = path.join("Default/Local Storage/leveldb");
    if !dir.join("CURRENT").exists() {
        return Ok(json!({"local":[],"databases":[]}));
    }
    let mut db = open(&dir, false)?;
    let mut local = BTreeMap::new();
    for (key, value) in entries(&mut db, &prefix)? {
        let key =
            decode(&key[prefix.len()..]).map_err(|_| "网站本地存储键编码无效，保留本机数据")?;
        let value = decode(&value).map_err(|_| "网站本地存储值编码无效，保留本机数据")?;
        if local.insert(key, value).is_some() {
            return Err("网站本地存储包含重复键，保留本机数据".into());
        }
    }
    Ok(json!({"local":local.into_iter().map(|(k,v)| [k,v]).collect::<Vec<_>>(),"databases":[]}))
}
fn varint(mut value: u64, output: &mut Vec<u8>) {
    while value >= 128 {
        output.push((value as u8) | 128);
        value >>= 7;
    }
    output.push(value as u8);
}
pub(super) fn write(path: &Path, key: &SiteKey, state: &Value) -> Result<(), String> {
    let prefix = prefix(key)?;
    if !eligible(path, key)? {
        return Err("网站存储布局已改变，取消导入并保留本机数据".into());
    }
    if !state["databases"].as_array().is_some_and(Vec::is_empty) {
        return Err(INVALID.into());
    }
    let rows = state["local"].as_array().ok_or(INVALID)?;
    if rows.len() > 100000 {
        return Err(INVALID.into());
    }
    let mut values = BTreeMap::new();
    let mut size = 0;
    for pair in rows {
        let pair = pair.as_array().filter(|v| v.len() == 2).ok_or(INVALID)?;
        let key = pair[0].as_str().ok_or(INVALID)?;
        let value = pair[1].as_str().ok_or(INVALID)?;
        let encoded_key = encode(key);
        let encoded_value = encode(value);
        size += encoded_key.len() + encoded_value.len();
        if size > LIMIT || values.insert(encoded_key, encoded_value).is_some() {
            return Err(INVALID.into());
        }
    }
    let dir = path.join("Default/Local Storage/leveldb");
    profiles::guard_tree(&dir)?;
    if values.is_empty() && !dir.join("CURRENT").exists() {
        return Ok(());
    }
    std::fs::create_dir_all(&dir).map_err(|_| INVALID)?;
    let mut db = open(&dir, true)?;
    let mut batch = WriteBatch::default();
    for (key, _) in entries(&mut db, &prefix)? {
        batch.delete(&key);
    }
    batch.put(b"VERSION", b"1");
    for (key, value) in values {
        let mut full = prefix.clone();
        full.extend(key);
        batch.put(&full, &value);
    }
    let meta = format!("META:{}", key.origin);
    let access = format!("METAACCESS:{}", key.origin);
    if rows.is_empty() {
        batch.delete(meta.as_bytes());
        batch.delete(access.as_bytes());
    } else {
        // Chromium protobuf: last_modified (Windows-epoch microseconds), size_bytes.
        let now = (chrono::Utc::now().timestamp_micros() + 11644473600000000) as u64;
        let mut modified = vec![8];
        varint(now, &mut modified);
        let accessed = modified.clone();
        modified.push(16);
        varint(size as u64, &mut modified);
        batch.put(meta.as_bytes(), &modified);
        batch.put(access.as_bytes(), &accessed);
    }
    db.write(batch, true).map_err(|_| INVALID)?;
    db.flush().map_err(|_| INVALID.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn key() -> SiteKey {
        SiteKey {
            origin: "https://ntp.msn.com".into(),
            partition_key: None,
        }
    }
    #[test]
    fn native_local_storage_roundtrip_preserves_unicode_and_other_namespaces() {
        let dir = tempfile::tempdir().unwrap();
        let state =
            json!({"local":[["", ""],["latin", "café"],["汉字😀", "a\u{0000}b😀"]],"databases":[]});
        assert!(eligible(dir.path(), &key()).unwrap());
        write(dir.path(), &key(), &state).unwrap();
        assert_eq!(read(dir.path(), &key()).unwrap(), state);
        let other = SiteKey {
            origin: "https://other.example".into(),
            partition_key: None,
        };
        write(
            dir.path(),
            &other,
            &json!({"local":[["keep","existing"]],"databases":[]}),
        )
        .unwrap();
        write(dir.path(), &key(), &json!({"local":[],"databases":[]})).unwrap();
        assert_eq!(
            read(dir.path(), &key()).unwrap(),
            json!({"local":[],"databases":[]})
        );
        assert_eq!(
            read(dir.path(), &other).unwrap()["local"],
            json!([["keep", "existing"]])
        );
    }
    #[test]
    fn native_path_never_handles_partitioned_or_indexeddb_namespaces() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir
            .path()
            .join("Default/IndexedDB/https_ntp.msn.com_0.indexeddb.leveldb");
        std::fs::create_dir_all(&legacy).unwrap();
        assert!(!eligible(dir.path(), &key()).unwrap());
        let other = SiteKey {
            origin: "https://other.example".into(),
            partition_key: None,
        };
        assert!(eligible(dir.path(), &other).unwrap());
        std::fs::create_dir_all(dir.path().join("Default/WebStorage/42/IndexedDB")).unwrap();
        assert!(!eligible(dir.path(), &other).unwrap());
        let partitioned = SiteKey {
            origin: other.origin,
            partition_key: Some(super::super::browser_session::Partition {
                top_level_site: "https://top.example".into(),
                has_cross_site_ancestor: true,
            }),
        };
        assert!(!eligible(dir.path(), &partitioned).unwrap());
    }
    #[test]
    fn unrelated_bucket_indexeddb_does_not_force_ntp_navigation() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Default/WebStorage/42/IndexedDB")).unwrap();
        let db =
            rusqlite::Connection::open(dir.path().join("Default/WebStorage/QuotaManager")).unwrap();
        db.execute_batch("CREATE TABLE buckets(id INTEGER PRIMARY KEY, storage_key TEXT); INSERT INTO buckets VALUES(42,'https://other.example/');").unwrap();
        assert!(eligible(dir.path(), &key()).unwrap());
        db.execute("UPDATE buckets SET storage_key='https://ntp.msn.com/'", [])
            .unwrap();
        assert!(!eligible(dir.path(), &key()).unwrap());
    }
    #[test]
    fn sqlite_localstorage_backend_is_never_mistaken_for_empty_leveldb() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Default")).unwrap();
        std::fs::write(
            dir.path().join("Default/LocalStorage"),
            b"SQLite format 3\0",
        )
        .unwrap();
        assert!(!eligible(dir.path(), &key()).unwrap());
    }
    #[test]
    fn invalid_native_input_does_not_replace_existing_state() {
        let dir = tempfile::tempdir().unwrap();
        let original = json!({"local":[["keep","existing"]],"databases":[]});
        write(dir.path(), &key(), &original).unwrap();
        for bad in [
            json!({"local":[["x",4]],"databases":[]}),
            json!({"local":[["x","1"],["x","2"]],"databases":[]}),
            json!({"local":[],"databases":[{}]}),
        ] {
            assert!(write(dir.path(), &key(), &bad).is_err());
            assert_eq!(read(dir.path(), &key()).unwrap(), original);
        }
    }
    #[test]
    fn native_decoder_and_version_reject_corruption() {
        assert!(decode(&[2, 65]).is_err());
        assert!(decode(&[0, 65]).is_err());
        assert!(decode(&[0, 0, 0xd8]).is_err());
        let dir = tempfile::tempdir().unwrap();
        let state = json!({"local":[["keep","existing"]],"databases":[]});
        write(dir.path(), &key(), &state).unwrap();
        let path = dir.path().join("Default/Local Storage/leveldb");
        {
            let mut db = DB::open(&path, Options::default()).unwrap();
            db.put(b"VERSION", b"2").unwrap();
            db.flush().unwrap();
        }
        assert!(read(dir.path(), &key()).unwrap_err().contains("版本不支持"));
        assert!(write(dir.path(), &key(), &state).is_err());
    }
}
