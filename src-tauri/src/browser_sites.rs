use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Deserialize)]
struct Site {
    id: String,
    url: String,
}

fn sites() -> &'static [Site] {
    static SITES: OnceLock<Vec<Site>> = OnceLock::new();
    SITES.get_or_init(|| {
        serde_json::from_str(include_str!("../../src/browser-sites.json"))
            .expect("bundled browser site catalog must be valid")
    })
}

pub fn destination_url(destination: &str) -> Option<&'static str> {
    let id = destination.strip_prefix("site:")?;
    sites().iter().find(|site| site.id == id).map(|site| site.url.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_site_destinations_are_unique_https_homepages() {
        let mut ids = std::collections::HashSet::new();
        for site in sites() {
            assert!(ids.insert(&site.id), "duplicate shortcut id");
            let url = reqwest::Url::parse(&site.url).unwrap();
            assert_eq!(url.scheme(), "https");
            assert!(url.has_host());
            assert!(url.username().is_empty() && url.password().is_none());
            assert_eq!(destination_url(&format!("site:{}", site.id)), Some(site.url.as_str()));
        }
        assert_eq!(ids.len(), 11);
        assert_eq!(destination_url("site:grok"), Some("https://grok.com/"));
        assert_eq!(destination_url("site:muse"), Some("https://muse.ai/"));
    }

    #[test]
    fn browser_site_destinations_reject_unlisted_or_arbitrary_urls() {
        for destination in ["site:unknown", "site:https://example.com", "https://chatgpt.com/", "site:chatgpt/../gemini", "site:ChatGPT"] {
            assert_eq!(destination_url(destination), None);
        }
    }
}
