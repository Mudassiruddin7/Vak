use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::{request::Parts, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use hmac::{Hmac, KeyInit, Mac};
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::sync::Semaphore;
use vak_adapters::cache::Cache;
use vak_adapters::sarvam::Sarvam;
use vak_adapters::supabase::{Supabase, User};

use crate::config::Config;

pub struct AppState {
    pub cfg: Config,
    pub sarvam: Sarvam,
    pub supa: Option<Supabase>,
    pub quota: Quota,
    pub audits: Arc<Semaphore>,
    pub sessions: Cache<User>,
}

impl AppState {
    pub fn new(cfg: Config) -> Arc<AppState> {
        let sarvam = Sarvam::new(&cfg.sarvam_key, &cfg.sarvam_base, cfg.rpm);
        let supa = Supabase::new(&cfg.supabase_url, &cfg.supabase_key);
        Arc::new(AppState {
            quota: Quota::new(cfg.anon_units_per_day, cfg.user_units_per_day, cfg.global_units_per_day, cfg.per_minute),
            audits: Arc::new(Semaphore::new(cfg.max_concurrent_audits.max(1))),
            sessions: Cache::new(2000, Duration::from_secs(60)),
            sarvam,
            supa,
            cfg,
        })
    }

    pub fn sign(&self, data: &[u8]) -> String {
        let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(&self.cfg.signing_key).expect("hmac takes any key length");
        mac.update(data);
        hex::encode(mac.finalize().into_bytes())
    }

    pub fn verify(&self, data: &[u8], sig: &str) -> bool {
        let Ok(bytes) = hex::decode(sig) else { return false };
        let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(&self.cfg.signing_key).expect("hmac takes any key length");
        mac.update(data);
        mac.verify_slice(&bytes).is_ok()
    }

    /// The Sarvam client to use for this visitor: their own key if they brought one, otherwise the server's.
    pub fn sarvam_for(&self, who: &Principal) -> Sarvam {
        match &who.own_key {
            Some(k) => self.sarvam.with_key(k, 40),
            None => self.sarvam.clone(),
        }
    }
}

/// Who is asking, for rate limits and quotas.
#[derive(Clone, Debug)]
pub struct Principal {
    pub id: String,
    pub user: Option<User>,
    pub own_key: Option<String>,
}

pub struct ApiFail(pub StatusCode, pub String);

impl IntoResponse for ApiFail {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error": self.1}))).into_response()
    }
}

pub fn fail(code: u16, msg: impl Into<String>) -> ApiFail {
    ApiFail(StatusCode::from_u16(code).unwrap_or(StatusCode::BAD_REQUEST), msg.into())
}

fn valid_key(k: &str) -> bool {
    (16..=160).contains(&k.len()) && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn client_ip(parts: &Parts, trust_proxy: bool, peer: Option<SocketAddr>) -> String {
    let forwarded = parts
        .headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .or_else(|| parts.headers.get("x-real-ip").and_then(|v| v.to_str().ok()))
        .and_then(|v| v.trim().parse::<IpAddr>().ok());
    let ip = if trust_proxy { forwarded.or(peer.map(|p| p.ip())) } else { peer.map(|p| p.ip()) };
    ip.map(|i| i.to_string()).unwrap_or_else(|| "unknown".into())
}

impl FromRequestParts<Arc<AppState>> for Principal {
    type Rejection = ApiFail;

    async fn from_request_parts(parts: &mut Parts, state: &Arc<AppState>) -> Result<Self, ApiFail> {
        let peer = ConnectInfo::<SocketAddr>::from_request_parts(parts, state).await.ok().map(|c| c.0);
        let ip = client_ip(parts, state.cfg.trust_proxy, peer);
        let own_key = parts.headers.get("x-sarvam-key").and_then(|v| v.to_str().ok()).map(str::trim).filter(|k| !k.is_empty()).map(String::from);
        if own_key.as_deref().is_some_and(|k| !valid_key(k)) {
            return Err(fail(400, "That doesn't look like a Sarvam API key."));
        }
        let jwt: Option<String> = parts.headers.get("authorization").and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer ")).map(str::trim).filter(|t| !t.is_empty()).map(String::from);
        let mut user = None;
        if let (Some(supa), Some(token)) = (&state.supa, &jwt) {
            let key = hex::encode(Sha256::digest(token.as_bytes()));
            user = match state.sessions.get(&key) {
                Some(u) => Some(u),
                None => {
                    let u = supa.user(token).await;
                    if let Some(u) = &u {
                        state.sessions.put(key, u.clone());
                    }
                    u
                }
            };
        }
        let id = match &user {
            Some(u) => format!("user:{}", u.id),
            None => format!("ip:{}", hex::encode(&Sha256::digest(ip.as_bytes())[..8])),
        };
        Ok(Principal { id, user, own_key })
    }
}

struct Bucket {
    day: u64,
    used: f64,
    recent: Vec<Instant>,
}

/// Per-visitor limits, in "units" (a light audit is 1, a thorough one 4, a translation a small fraction), plus one
/// shared pool for everyone using the server's own key. In memory, so on serverless each warm instance counts
/// separately: a soft guard that errs on the strict side, not an accounting system.
pub struct Quota {
    anon: f64,
    user: f64,
    global_cap: f64,
    per_min: usize,
    buckets: Mutex<HashMap<String, Bucket>>,
    global: Mutex<(u64, f64)>,
}

pub enum Denied {
    Daily,
    Fast,
    Everyone,
}

impl Quota {
    pub fn new(anon: f64, user: f64, global_cap: f64, per_min: usize) -> Self {
        Quota { anon, user, global_cap, per_min: per_min.max(1), buckets: Mutex::new(HashMap::new()), global: Mutex::new((today(), 0.0)) }
    }

    pub fn left(&self, who: &Principal) -> f64 {
        let day = today();
        let cap = self.cap(who);
        let g = self.buckets.lock().expect("quota");
        let mine = g.get(&who.id).filter(|b| b.day == day).map(|b| (cap - b.used).max(0.0)).unwrap_or(cap);
        if who.own_key.is_some() {
            return mine;
        }
        let pool = self.global.lock().expect("global");
        let pool_left = if pool.0 == day { (self.global_cap - pool.1).max(0.0) } else { self.global_cap };
        mine.min(pool_left)
    }

    fn cap(&self, who: &Principal) -> f64 {
        if who.own_key.is_some() { f64::INFINITY } else if who.user.is_some() { self.user } else { self.anon }
    }

    /// Spend `units`, or say why not. Visitors bringing their own key skip the daily caps but not the minute limit.
    pub fn spend(&self, who: &Principal, units: f64) -> Result<(), Denied> {
        let (day, now) = (today(), Instant::now());
        let cap = self.cap(who);
        let mut g = self.buckets.lock().expect("quota");
        if g.len() > 20_000 {
            g.retain(|_, b| b.day == day);
        }
        let b = g.entry(who.id.clone()).or_insert(Bucket { day, used: 0.0, recent: vec![] });
        if b.day != day {
            *b = Bucket { day, used: 0.0, recent: vec![] };
        }
        b.recent.retain(|t| now.duration_since(*t) < Duration::from_secs(60));
        if b.recent.len() >= self.per_min {
            return Err(Denied::Fast);
        }
        if b.used + units > cap + 1e-9 {
            return Err(Denied::Daily);
        }
        if who.own_key.is_none() {
            let mut pool = self.global.lock().expect("global");
            if pool.0 != day {
                *pool = (day, 0.0);
            }
            if pool.1 + units > self.global_cap + 1e-9 {
                return Err(Denied::Everyone);
            }
            pool.1 += units;
        }
        b.used += units;
        b.recent.push(now);
        Ok(())
    }
}

fn today() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() / 86_400).unwrap_or(0)
}

pub fn spend_or_429(state: &AppState, who: &Principal, units: f64) -> Result<(), ApiFail> {
    state.quota.spend(who, units).map_err(|d| match d {
        Denied::Fast => fail(429, "That's a lot of requests in a minute. Give it a moment."),
        Denied::Everyone => fail(429, "Today's free credits for everyone are used up. They come back tomorrow, or add your own Sarvam key in Settings to keep going."),
        Denied::Daily => fail(
            429,
            if who.user.is_some() {
                "You've used today's free allowance. It resets tomorrow, or add your own Sarvam key in Settings to keep going."
            } else {
                "You've used today's free allowance. Sign in for a little more, or add your own Sarvam key in Settings."
            },
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn who(id: &str, byok: bool) -> Principal {
        Principal { id: id.into(), user: None, own_key: byok.then(|| "k".repeat(20)) }
    }

    #[test]
    fn daily_cap_and_burst_limit() {
        let q = Quota::new(3.0, 10.0, 1000.0, 30);
        let a = who("ip:a", false);
        assert!(q.spend(&a, 2.0).is_ok() && q.spend(&a, 1.0).is_ok());
        assert!(matches!(q.spend(&a, 0.5), Err(Denied::Daily)));
        assert!(q.spend(&who("ip:b", false), 1.0).is_ok(), "buckets are per visitor");
        let own = who("ip:c", true);
        for _ in 0..30 {
            assert!(q.spend(&own, 50.0).is_ok(), "bringing your own key lifts the daily cap");
        }
        assert!(matches!(q.spend(&own, 1.0), Err(Denied::Fast)), "but not the per-minute limit");
        assert!((q.left(&a) - 0.0).abs() < 1e-9);
    }

    #[test]
    fn one_shared_pool_protects_the_credits() {
        let q = Quota::new(6.0, 20.0, 10.0, 30);
        assert!(q.spend(&who("ip:a", false), 6.0).is_ok());
        assert!(q.spend(&who("ip:b", false), 4.0).is_ok());
        assert!(matches!(q.spend(&who("ip:c", false), 1.0), Err(Denied::Everyone)), "the pool is empty for strangers");
        assert!(q.spend(&who("ip:d", true), 5.0).is_ok(), "a visitor with their own key is not drawing on it");
        assert!(q.left(&who("ip:e", false)) < 1e-9);
    }

    #[test]
    fn the_minute_limit_is_configurable() {
        let q = Quota::new(100.0, 100.0, 1000.0, 3);
        let a = who("ip:a", false);
        for _ in 0..3 {
            assert!(q.spend(&a, 0.1).is_ok());
        }
        assert!(matches!(q.spend(&a, 0.1), Err(Denied::Fast)));
    }

    #[test]
    fn signatures_round_trip() {
        let cfg = Config::from_env();
        let s = AppState::new(Config { signing_key: b"k".to_vec(), ..cfg });
        let sig = s.sign(b"report");
        assert!(s.verify(b"report", &sig) && !s.verify(b"reporT", &sig) && !s.verify(b"report", "zz"));
    }

    #[test]
    fn keys_are_shape_checked() {
        assert!(valid_key("sk_abcdefghijklmnop_1234") && !valid_key("short") && !valid_key("has space in it 12345678") && !valid_key("bad;chars;and;stuff;1234"));
    }
}
