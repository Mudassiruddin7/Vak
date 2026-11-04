//! A minimal Supabase client: verify a visitor's session, and read and write their rows through PostgREST
//! with their own token, so row-level security decides what they can see. No service key is ever needed
//! for the per-user paths.

use std::time::Duration;

use serde_json::{json, Value};

#[derive(Clone)]
pub struct Supabase {
    http: reqwest::Client,
    url: String,
    anon: String,
}

#[derive(Clone, Debug)]
pub struct User {
    pub id: String,
    pub email: Option<String>,
}

impl Supabase {
    pub fn new(url: &str, anon_key: &str) -> Option<Self> {
        let url = url.trim().trim_end_matches('/').to_string();
        (url.starts_with("https://") && !anon_key.is_empty()).then(|| Supabase {
            http: reqwest::Client::builder().timeout(Duration::from_secs(10)).build().expect("client"),
            url,
            anon: anon_key.to_string(),
        })
    }

    pub fn public_config(&self) -> Value {
        json!({"url": self.url, "anon_key": self.anon})
    }

    fn req(&self, rb: reqwest::RequestBuilder, jwt: Option<&str>) -> reqwest::RequestBuilder {
        rb.header("apikey", &self.anon).bearer_auth(jwt.unwrap_or(&self.anon))
    }

    /// The user a token belongs to, or `None` when it is missing, expired or forged.
    pub async fn user(&self, jwt: &str) -> Option<User> {
        if jwt.len() < 20 || jwt.len() > 4096 {
            return None;
        }
        let r = self.req(self.http.get(format!("{}/auth/v1/user", self.url)), Some(jwt)).send().await.ok()?;
        if !r.status().is_success() {
            return None;
        }
        let v: Value = r.json().await.ok()?;
        Some(User { id: v.get("id")?.as_str()?.to_string(), email: v.get("email").and_then(|e| e.as_str()).map(String::from) })
    }

    pub async fn rpc(&self, name: &str, jwt: &str, args: &Value) -> Result<Value, String> {
        let r = self.req(self.http.post(format!("{}/rest/v1/rpc/{name}", self.url)), Some(jwt)).json(args).send().await.map_err(|e| e.to_string())?;
        let ok = r.status().is_success();
        let v: Value = r.json().await.unwrap_or(Value::Null);
        if ok { Ok(v) } else { Err(v.get("message").and_then(|m| m.as_str()).unwrap_or("request failed").to_string()) }
    }

    /// `query` is a PostgREST query string such as `order=created_at.desc&limit=20`.
    pub async fn select(&self, table: &str, query: &str, jwt: Option<&str>) -> Result<Value, String> {
        let r = self.req(self.http.get(format!("{}/rest/v1/{table}?{query}", self.url)), jwt).send().await.map_err(|e| e.to_string())?;
        let ok = r.status().is_success();
        let v: Value = r.json().await.unwrap_or(Value::Null);
        if ok { Ok(v) } else { Err(v.get("message").and_then(|m| m.as_str()).unwrap_or("request failed").to_string()) }
    }

    pub async fn insert(&self, table: &str, row: &Value, jwt: &str) -> Result<Value, String> {
        let rb = self.req(self.http.post(format!("{}/rest/v1/{table}", self.url)), Some(jwt)).header("Prefer", "return=representation").json(row);
        let r = rb.send().await.map_err(|e| e.to_string())?;
        let ok = r.status().is_success();
        let v: Value = r.json().await.unwrap_or(Value::Null);
        if ok { Ok(v) } else { Err(v.get("message").and_then(|m| m.as_str()).unwrap_or("request failed").to_string()) }
    }

    pub async fn upsert(&self, table: &str, row: &Value, jwt: &str, on_conflict: &str) -> Result<(), String> {
        let rb = self
            .req(self.http.post(format!("{}/rest/v1/{table}?on_conflict={on_conflict}", self.url)), Some(jwt))
            .header("Prefer", "resolution=merge-duplicates")
            .json(row);
        let r = rb.send().await.map_err(|e| e.to_string())?;
        if r.status().is_success() { Ok(()) } else { Err(format!("HTTP {}", r.status().as_u16())) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_projects_with_a_key_are_accepted() {
        assert!(Supabase::new("https://abc.supabase.co/", "k").is_some());
        assert!(Supabase::new("http://abc.supabase.co", "k").is_none());
        assert!(Supabase::new("https://abc.supabase.co", "").is_none());
        assert_eq!(Supabase::new("https://abc.supabase.co/", "k").unwrap().public_config()["url"], "https://abc.supabase.co");
    }
}
