use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Grant {
    pub id: String,
    pub name: String,
    pub profile_ids: Vec<String>,
    pub permissions: Vec<String>,
    pub origins: Vec<String>,
    pub expires_at: i64,
    pub confirm_interactions: bool,
    pub file_roots: Vec<String>,
    pub revoked: bool,
    pub token_hash: String,
}
impl Grant {
    pub fn authorize(
        &self,
        permission: &str,
        profile: Option<&str>,
        now: i64,
    ) -> Result<(), String> {
        if self.revoked || self.expires_at <= now {
            return Err("AUTHORIZATION_EXPIRED_OR_REVOKED".into());
        }
        if !self.permissions.iter().any(|p| p == permission) {
            return Err("PERMISSION_DENIED".into());
        }
        if profile.is_some_and(|id| !self.profile_ids.iter().any(|p| p == id)) {
            return Err("PROFILE_NOT_AUTHORIZED".into());
        }
        Ok(())
    }
    pub fn origin(&self, url: &str) -> Result<(), String> {
        let parsed = reqwest::Url::parse(url).map_err(|_| "INVALID_URL")?;
        if url == "about:blank" {
            return Ok(());
        }
        if !["http", "https"].contains(&parsed.scheme())
            || !parsed.username().is_empty()
            || parsed.password().is_some()
        {
            return Err("URL_NOT_ALLOWED".into());
        }
        if !self.origins.is_empty()
            && !self
                .origins
                .contains(&parsed.origin().ascii_serialization())
        {
            return Err("ORIGIN_NOT_AUTHORIZED".into());
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn grant() -> Grant {
        Grant {
            id: "client".into(),
            name: "Reader".into(),
            profile_ids: vec!["one".into()],
            permissions: vec!["read".into()],
            origins: vec![],
            expires_at: 100,
            confirm_interactions: true,
            file_roots: vec![],
            revoked: false,
            token_hash: String::new(),
        }
    }
    #[test]
    fn denied_permission_and_profile_never_execute() {
        let g = grant();
        assert!(g.authorize("manage", Some("one"), 1).is_err());
        assert!(g.authorize("read", Some("two"), 1).is_err());
        assert!(g.authorize("read", Some("one"), 1).is_ok());
    }
    #[test]
    fn expiry_and_revocation_override_permissions() {
        let mut g = grant();
        assert!(g.authorize("read", Some("one"), 100).is_err());
        g.revoked = true;
        assert!(g.authorize("read", Some("one"), 1).is_err());
    }
    #[test]
    fn origins_are_exact_and_local_files_never_navigable() {
        let mut g = grant();
        g.origins = vec!["https://example.test".into()];
        assert!(g.origin("https://example.test/page").is_ok());
        for url in [
            "https://example.test.evil/",
            "http://example.test/",
            "https://example.test:8443/",
            "file:///C:/secret",
            "javascript:alert(1)",
            "https://user:pass@example.test/",
        ] {
            assert!(g.origin(url).is_err(), "{url}");
        }
    }
}
