use base64::Engine;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::Duration,
};
use tauri::Manager;

const ORIGIN: &str = "https://nodecloak.com";
const CACHE_FILE: &str = "published-browser-sites.json";
const MAX_CATALOG: usize = 2 * 1024 * 1024;
const MAX_ICON: usize = 512 * 1024;
const ICONS: &[&str] = &[
    "ai", "shopping", "claude", "chatgpt", "gemini", "grok", "muse", "amazon", "ebay", "etsy",
    "walmart", "shopee", "lazada", "globe",
];

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub revision: u64,
    pub groups: Vec<Group>,
    #[serde(default)]
    pub support_url: String,
    #[serde(default)]
    pub announcement: String,
    #[serde(default)]
    pub announcements: Vec<Announcement>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Announcement {
    pub id: String,
    pub title: String,
    pub body: String,
    pub revision: u64,
    pub published_at: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub id: String,
    pub name: String,
    pub name_en: String,
    pub icon: String,
    pub order: i64,
    pub enabled: bool,
    pub sites: Vec<Site>,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct Site {
    pub id: String,
    pub name: String,
    pub url: String,
    pub icon: String,
    pub order: i64,
    pub enabled: bool,
}

fn bundled() -> Catalog {
    serde_json::from_str(include_str!("../../src/browser-sites.json"))
        .expect("valid bundled catalog")
}
fn cache() -> &'static Mutex<HashMap<PathBuf, Catalog>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Catalog>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}
fn cached(root: &Path) -> Catalog {
    let mut catalogs = cache().lock().unwrap_or_else(|e| e.into_inner());
    catalogs
        .entry(root.to_path_buf())
        .or_insert_with(|| {
            std::fs::metadata(root.join(CACHE_FILE))
                .ok()
                .filter(|m| m.len() <= MAX_CATALOG as u64)
                .and_then(|_| std::fs::read(root.join(CACHE_FILE)).ok())
                .and_then(|bytes| serde_json::from_slice::<Catalog>(&bytes).ok())
                .filter(|catalog| validate(catalog).is_ok())
                .unwrap_or_else(bundled)
        })
        .clone()
}
fn replace(root: &Path, catalog: Catalog) -> Result<(), String> {
    validate(&catalog)?;
    let bytes = serde_json::to_vec(&catalog).map_err(|_| "目录数据无效")?;
    let mut catalogs = cache().lock().map_err(|_| "网站目录不可用")?;
    if catalogs
        .get(root)
        .is_some_and(|current| current.revision > catalog.revision)
    {
        return Ok(());
    }
    // Memory is updated only after the publication is durable. Launch and rendering
    // consume this same snapshot; an empty publication intentionally has no sites.
    crate::engine::atomic_write(&root.join(CACHE_FILE), &bytes)?;
    catalogs.insert(root.to_path_buf(), catalog);
    Ok(())
}
fn safe_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}
fn safe_icon(value: &str) -> bool {
    if ICONS.contains(&value) {
        return true;
    }
    value
        .strip_prefix("/v1/catalog/icons/")
        .and_then(|v| v.strip_suffix(".png"))
        .is_some_and(|hash| {
            hash.len() == 64
                && hash
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        })
}
fn safe_url(value: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(value) else {
        return false;
    };
    if value.len() > 2048
        || url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port_or_known_default() != Some(443)
    {
        return false;
    }
    // Domain-only destinations disallow every literal address, including numeric,
    // hexadecimal, IPv6 and IPv4-mapped IPv6 variants normalized by the URL parser.
    let Some(host) = url.domain() else {
        return false;
    };
    if host.parse::<std::net::IpAddr>().is_ok()
        || host.starts_with('[')
        || host.ends_with('.')
        || !host.contains('.')
    {
        return false;
    }
    ![
        "localhost",
        "local",
        "internal",
        "lan",
        "home",
        "test",
        "invalid",
        "onion",
    ]
    .iter()
    .any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}")))
}
fn validate(catalog: &Catalog) -> Result<(), String> {
    let mut announcement_ids = HashSet::new();
    if catalog.announcements.len() > 100
        || catalog.announcements.iter().any(|a| {
            !safe_id(&a.id)
                || !announcement_ids.insert(&a.id)
                || a.title.trim().is_empty()
                || a.title.chars().count() > 160
                || a.body.trim().is_empty()
                || a.body.chars().count() > 2000
                || a.revision == 0
                || chrono::DateTime::parse_from_rfc3339(&a.published_at).is_err()
        })
    {
        return Err("公告数据无效".into());
    }
    if catalog.groups.len() > 40
        || catalog.announcement.chars().count() > 2000
        || (!catalog.support_url.is_empty() && !safe_url(&catalog.support_url))
    {
        return Err("网站目录无效".into());
    }
    let mut ids = HashSet::new();
    let mut count = 0;
    let text = |s: &str| !s.trim().is_empty() && s.chars().count() <= 160;
    for group in &catalog.groups {
        if !safe_id(&group.id)
            || !ids.insert(&group.id)
            || !text(&group.name)
            || !text(&group.name_en)
            || !safe_icon(&group.icon)
            || group.order.unsigned_abs() > 100000
            || group.sites.len() > 100
        {
            return Err("网站组无效".into());
        }
        for site in &group.sites {
            if !safe_id(&site.id)
                || !ids.insert(&site.id)
                || !text(&site.name)
                || !safe_icon(&site.icon)
                || site.order.unsigned_abs() > 100000
                || !safe_url(&site.url)
            {
                return Err("网站无效".into());
            }
        }
        count += group.sites.len();
    }
    if count > 500 {
        return Err("网站目录过大".into());
    }
    Ok(())
}
fn visible(mut catalog: Catalog) -> Catalog {
    catalog.groups.retain(|g| g.enabled);
    catalog.groups.sort_by(|a, b| a.order.cmp(&b.order));
    for group in &mut catalog.groups {
        group.sites.retain(|s| s.enabled);
        group.sites.sort_by(|a, b| a.order.cmp(&b.order));
    }
    catalog
}
async fn download(path: &str, max: usize) -> Result<(Vec<u8>, String), String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "网站目录连接不可用")?;
    let mut response = client
        .get(format!("{ORIGIN}{path}"))
        .send()
        .await
        .map_err(|_| "网站目录暂不可用")?;
    if !response.status().is_success()
        || response
            .content_length()
            .is_some_and(|len| len > max as u64)
    {
        return Err("网站目录响应无效".into());
    }
    let mime = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| "网站目录下载失败")? {
        if bytes.len() + chunk.len() > max {
            return Err("网站目录响应过大".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok((bytes, mime))
}
#[tauri::command]
pub async fn get_browser_site_catalog(app: tauri::AppHandle) -> Result<Catalog, String> {
    let root = app.state::<crate::data_root::DataRoot>().0.clone();
    let previous = cached(&root);
    if let Ok((bytes, _)) = download("/v1/catalog", MAX_CATALOG).await {
        if let Ok(catalog) = serde_json::from_slice::<Catalog>(&bytes) {
            if catalog.revision >= previous.revision {
                let _ = replace(&root, catalog);
            }
        }
    }
    Ok(visible(cached(&root)))
}
#[tauri::command]
pub async fn get_browser_site_icon(app: tauri::AppHandle, icon: String) -> Result<String, String> {
    if !safe_icon(&icon) || !icon.ends_with(".png") {
        return Err("网站图标无效".into());
    }
    let root = app.state::<crate::data_root::DataRoot>().0.clone();
    let catalog = cached(&root);
    if !catalog
        .groups
        .iter()
        .any(|g| g.icon == icon || g.sites.iter().any(|s| s.icon == icon))
    {
        return Err("网站图标不在已发布目录中".into());
    }
    let name = icon.rsplit('/').next().ok_or("网站图标无效")?;
    let path = root.join("catalog-icons").join(name);
    let local = std::fs::metadata(&path)
        .ok()
        .filter(|m| m.len() <= MAX_ICON as u64)
        .and_then(|_| std::fs::read(&path).ok())
        .filter(|v| v.starts_with(b"\x89PNG\r\n\x1a\n"));
    let bytes = if let Some(bytes) = local {
        bytes
    } else {
        let (bytes, mime) = download(&icon, MAX_ICON).await?;
        if !mime.split(';').next().is_some_and(|m| m == "image/png")
            || !bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        {
            return Err("网站图标格式无效".into());
        }
        crate::engine::atomic_write(&path, &bytes)?;
        bytes
    };
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}
#[tauri::command]
pub fn open_catalog_support(app: tauri::AppHandle) -> Result<(), String> {
    let catalog = cached(&app.state::<crate::data_root::DataRoot>().0);
    if !safe_url(&catalog.support_url) {
        return Err("帮助链接不可用".into());
    }
    crate::platform::open_external(&catalog.support_url)
}
#[tauri::command]
pub fn open_announcement_link(url: String) -> Result<(), String> {
    if !safe_url(&url) {
        return Err("公告链接不可用".into());
    }
    crate::platform::open_external(&url)
}
pub fn destination_url(root: &Path, destination: &str) -> Option<String> {
    let requested = destination.strip_prefix("site:")?;
    let (id, revision) = if let Some((id, revision)) = requested.split_once('@') {
        (id, Some(revision.parse::<u64>().ok()?))
    } else {
        (requested, None)
    };
    if !safe_id(id) {
        return None;
    }
    let catalog = cached(root);
    if revision.is_some_and(|r| r != catalog.revision) {
        return None;
    }
    catalog
        .groups
        .iter()
        .filter(|g| g.enabled)
        .flat_map(|g| &g.sites)
        .find(|s| s.enabled && s.id == id && safe_url(&s.url))
        .map(|s| s.url.clone())
}
#[cfg(test)]
#[path = "browser_sites_tests.rs"]
mod tests;
