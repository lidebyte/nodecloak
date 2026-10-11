use super::*;

#[test]
fn announcements_are_retained_and_validated_in_native_cache() {
    let mut catalog = bundled();
    catalog.announcements.push(Announcement {
        id: "news".into(),
        title: "Notice".into(),
        body: "History".into(),
        revision: 1,
        published_at: "2026-10-11T10:00:00Z".into(),
    });
    let root = tempfile::tempdir().unwrap();
    replace(root.path(), catalog.clone()).unwrap();
    assert_eq!(cached(root.path()).announcements[0].body, "History");
    catalog.announcements[0].revision = 0;
    assert!(validate(&catalog).is_err());
    catalog.announcements[0].revision = 1;
    catalog.announcements[0].published_at = "invalid".into();
    assert!(validate(&catalog).is_err());
}

#[test]
fn catalog_destinations_reject_local_and_non_https_urls() {
    for url in [
        "http://example.com",
        "https://127.0.0.1",
        "https://10.2.3.4",
        "https://[::1]",
        "https://[::ffff:192.168.1.1]",
        "https://localhost",
        "https://box.local",
        "https://name:pass@example.com",
        "https://example.com:444",
        "https://0x7f000001",
        "https://metadata.google.internal",
        "https://127.1",
    ] {
        assert!(!safe_url(url), "unsafe: {url}");
    }
    assert!(safe_url("https://example.com/path?query=yes"));
}

#[test]
fn publication_cache_matches_launch_and_retains_empty_publication() {
    let root = tempfile::tempdir().unwrap();
    let mut published = bundled();
    published.revision = 7;
    published.groups[0].sites[0].url = "https://example.com/new".into();
    replace(root.path(), published).unwrap();
    assert_eq!(
        destination_url(root.path(), "site:claude@7").unwrap(),
        "https://example.com/new"
    );
    assert_eq!(destination_url(root.path(), "site:claude@6"), None);
    replace(
        root.path(),
        Catalog {
            revision: 8,
            groups: vec![],
            support_url: String::new(),
            announcement: String::new(),
            announcements: vec![],
        },
    )
    .unwrap();
    assert_eq!(destination_url(root.path(), "site:claude"), None);
    assert_eq!(cached(root.path()).revision, 8);
}

#[test]
fn unlisted_destinations_disabled_entries_and_unsafe_icons_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    for destination in [
        "site:unknown",
        "site:https://example.com",
        "https://chatgpt.com/",
        "site:chatgpt/../gemini",
        "site:ChatGPT",
    ] {
        assert_eq!(destination_url(root.path(), destination), None);
    }
    let mut catalog = bundled();
    catalog.groups[0].sites[0].enabled = false;
    catalog.groups[1].enabled = false;
    replace(root.path(), catalog).unwrap();
    assert_eq!(destination_url(root.path(), "site:claude"), None);
    assert_eq!(destination_url(root.path(), "site:amazon"), None);
    assert!(!safe_icon("/v1/catalog/icons/../admin"));
    assert!(!safe_icon("https://other.example/logo.svg"));
    assert!(safe_icon(&format!(
        "/v1/catalog/icons/{}.png",
        "a".repeat(64)
    )));
}

#[test]
fn corrupt_disk_cache_falls_back_and_valid_cache_survives_new_process_key() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join(CACHE_FILE), b"{} not json").unwrap();
    assert_eq!(cached(root.path()).revision, 0);
    let another = tempfile::tempdir().unwrap();
    let mut catalog = bundled();
    catalog.revision = 12;
    std::fs::write(
        another.path().join(CACHE_FILE),
        serde_json::to_vec(&catalog).unwrap(),
    )
    .unwrap();
    assert_eq!(cached(another.path()).revision, 12);
    assert_eq!(
        destination_url(another.path(), "site:chatgpt").unwrap(),
        "https://chatgpt.com/"
    );
}

#[test]
fn builtin_catalog_has_unique_safe_sites_and_rejects_duplicate_ids() {
    let catalog = bundled();
    validate(&catalog).unwrap();
    assert_eq!(
        catalog.groups.iter().map(|g| g.sites.len()).sum::<usize>(),
        11
    );
    let mut invalid = catalog.clone();
    invalid.groups[0].sites[0].id = invalid.groups[0].id.clone();
    assert!(validate(&invalid).is_err());
    invalid = catalog.clone();
    invalid.groups[0].sites[0].url = "https://localhost".into();
    assert!(validate(&invalid).is_err());
}
