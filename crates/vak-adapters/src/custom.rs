//! Auditing a model on the person's own OpenAI-compatible endpoint. The URL is checked before anything is
//! sent: cloud-metadata and link-local addresses are never allowed, and private or loopback addresses only
//! when the server is configured for it (a laptop, not a public deployment). The connection is pinned to the
//! address that was checked, so a DNS answer that changes in between cannot redirect it.

use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};
use vak_app::ports::ApiError;
use vak_app::target::{Answer, Target};

fn is_private(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => v.is_private() || v.is_loopback(),
        IpAddr::V6(v) => v.is_loopback() || (v.segments()[0] & 0xfe00) == 0xfc00,
    }
}

fn is_blocked(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => v.is_link_local() || v.is_multicast() || v.is_unspecified() || v.is_broadcast() || v.octets() == [169, 254, 169, 254],
        IpAddr::V6(v) => v.is_multicast() || v.is_unspecified() || (v.segments()[0] & 0xffc0) == 0xfe80 || v.to_ipv4_mapped().is_some_and(|m| m.is_link_local() || m.is_unspecified()),
    }
}

#[derive(Debug)]
pub struct Checked {
    pub base: String,
    pub host: String,
    pub addr: SocketAddr,
}

pub async fn check_endpoint(url: &str, allow_private: bool) -> Result<Checked, String> {
    let parsed = reqwest::Url::parse(url).map_err(|_| "The endpoint must be an http(s) URL, for example http://localhost:11434/v1".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") || !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("The endpoint must be an http(s) URL without a username or password.".into());
    }
    let host = parsed.host_str().ok_or("The endpoint has no host.")?.to_string();
    let port = parsed.port_or_known_default().unwrap_or(443);
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), port)).await.map_err(|_| format!("Couldn't find the host {host:?}."))?.collect();
    if addrs.is_empty() {
        return Err(format!("Couldn't find the host {host:?}."));
    }
    for a in &addrs {
        if is_blocked(&a.ip()) {
            return Err("That address isn't allowed.".into());
        }
        if is_private(&a.ip()) && !allow_private {
            return Err("Private addresses are switched off on this server.".into());
        }
    }
    Ok(Checked { base: parsed.as_str().trim_end_matches('/').to_string(), host, addr: addrs[0] })
}

pub struct CustomTarget {
    client: reqwest::Client,
    base: String,
    key: Option<String>,
    model: String,
    system: Option<String>,
}

impl CustomTarget {
    pub async fn new(url: &str, key: Option<String>, model: &str, system: Option<String>, allow_private: bool) -> Result<Self, String> {
        let c = check_endpoint(url, allow_private).await?;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .connect_timeout(Duration::from_secs(8))
            .redirect(reqwest::redirect::Policy::none())
            .resolve(&c.host, c.addr)
            .build()
            .map_err(|e| e.to_string())?;
        Ok(CustomTarget { client, base: c.base, key, model: model.into(), system })
    }
}

#[async_trait]
impl Target for CustomTarget {
    fn label(&self) -> String {
        format!("{} (your endpoint)", self.model)
    }

    async fn ask(&self, prompt: &str, temperature: f32, n: u32, max_tokens: u32) -> Result<Vec<Answer>, ApiError> {
        let mut messages = vec![];
        if let Some(s) = &self.system {
            messages.push(json!({"role": "system", "content": s}));
        }
        messages.push(json!({"role": "user", "content": prompt}));
        let body = json!({"model": self.model, "messages": messages, "temperature": temperature, "max_tokens": max_tokens});
        let mut out = vec![];
        for _ in 0..n {
            let mut rb = self.client.post(format!("{}/chat/completions", self.base)).json(&body);
            if let Some(k) = &self.key {
                rb = rb.bearer_auth(k);
            }
            let resp = rb.send().await.map_err(|_| ApiError::Other("Your endpoint didn't answer.".into()))?;
            if !resp.status().is_success() {
                return Err(ApiError::Other(format!("Your endpoint answered HTTP {}.", resp.status().as_u16())));
            }
            let v: Value = resp.json().await.map_err(|_| ApiError::Other("Your endpoint's reply wasn't JSON.".into()))?;
            let text = vak_core::text::strip_think(v.pointer("/choices/0/message/content").and_then(|c| c.as_str()).unwrap_or(""));
            out.push(if text.is_empty() { Answer::failed("Empty answer.") } else { Answer::ok(text) });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn metadata_and_private_addresses_are_refused() {
        assert!(check_endpoint("http://169.254.169.254/latest", true).await.is_err());
        assert!(check_endpoint("http://[fe80::1]/v1", true).await.is_err());
        assert!(check_endpoint("http://0.0.0.0:80/v1", true).await.is_err());
        assert_eq!(check_endpoint("http://127.0.0.1:11434/v1", false).await.unwrap_err(), "Private addresses are switched off on this server.");
        assert!(check_endpoint("http://127.0.0.1:11434/v1", true).await.is_ok());
        assert!(check_endpoint("ftp://example.com", true).await.is_err());
        assert!(check_endpoint("http://user:pw@127.0.0.1/v1", true).await.is_err());
        assert!(check_endpoint("not a url", true).await.is_err());
    }
}
