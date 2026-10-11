use serde_json::{Map, Value};
use std::collections::BTreeSet;
pub struct Merge {
    pub value: Value,
    pub conflicts: Vec<String>,
}
pub fn merge(base: &Value, local: &Value, remote: &Value) -> Merge {
    let mut conflicts = Vec::new();
    let value =
        field(Some(base), Some(local), Some(remote), "", &mut conflicts).unwrap_or(Value::Null);
    Merge { value, conflicts }
}
fn field(
    base: Option<&Value>,
    local: Option<&Value>,
    remote: Option<&Value>,
    path: &str,
    conflicts: &mut Vec<String>,
) -> Option<Value> {
    if local == remote {
        return local.cloned();
    };
    if local == base {
        return remote.cloned();
    };
    if remote == base {
        return local.cloned();
    };
    // Proxy settings and credentials form one unit; mixing hosts/passwords is unsafe.
    if path != "/proxy" {
        if let (Some(Value::Object(b)), Some(Value::Object(l)), Some(Value::Object(r))) =
            (base, local, remote)
        {
            let keys: BTreeSet<_> = b.keys().chain(l.keys()).chain(r.keys()).collect();
            let mut out = Map::new();
            for key in keys {
                let next = format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"));
                if let Some(v) = field(b.get(key), l.get(key), r.get(key), &next, conflicts) {
                    out.insert(key.clone(), v);
                }
            }
            return Some(Value::Object(out));
        }
    }
    conflicts.push(path.to_string());
    local.cloned()
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn independent_edits_merge_without_timestamp() {
        let base = json!({"name":"A","notes":"old","preferences":{"language":"en","privacy":false},"proxy":{"host":"a","port":80,"password":"p"}});
        let mut local = base.clone();
        local["name"] = json!("B");
        let mut remote = base.clone();
        remote["notes"] = json!("new");
        remote["preferences"]["privacy"] = json!(true);
        let result = merge(&base, &local, &remote);
        assert!(result.conflicts.is_empty());
        assert_eq!(
            result.value,
            json!({"name":"B","notes":"new","preferences":{"language":"en","privacy":true},"proxy":{"host":"a","port":80,"password":"p"}})
        );
    }
    #[test]
    fn proxy_is_atomic_and_same_field_conflicts_are_visible() {
        let base = json!({"name":"A","proxy":{"host":"a","port":80,"password":"p"}});
        let mut local = base.clone();
        local["proxy"]["host"] = json!("local");
        local["name"] = json!("local");
        let mut remote = base.clone();
        remote["proxy"]["port"] = json!(8080);
        remote["name"] = json!("remote");
        let result = merge(&base, &local, &remote);
        assert_eq!(result.conflicts, vec!["/name", "/proxy"]);
        assert_eq!(result.value, local);
    }
    #[test]
    fn removal_and_modification_conflict() {
        let base = json!({"notes":"a"});
        let local = json!({});
        let remote = json!({"notes":"b"});
        assert_eq!(merge(&base, &local, &remote).conflicts, vec!["/notes"]);
    }
}
