//! A small in-memory answer cache with a time limit. It lives only as long as the process, so nothing a
//! person typed is kept anywhere durable, and on serverless it simply starts empty on a cold start.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct Cache<V: Clone> {
    inner: Mutex<Inner<V>>,
    ttl: Duration,
    cap: usize,
}

struct Inner<V> {
    map: HashMap<String, (Instant, V)>,
    order: VecDeque<String>,
}

impl<V: Clone> Cache<V> {
    pub fn new(cap: usize, ttl: Duration) -> Self {
        Cache { inner: Mutex::new(Inner { map: HashMap::new(), order: VecDeque::new() }), ttl, cap }
    }

    pub fn get(&self, key: &str) -> Option<V> {
        let mut g = self.inner.lock().ok()?;
        match g.map.get(key) {
            Some((at, v)) if at.elapsed() < self.ttl => Some(v.clone()),
            Some(_) => {
                g.map.remove(key);
                None
            }
            None => None,
        }
    }

    pub fn put(&self, key: String, value: V) {
        let Ok(mut g) = self.inner.lock() else { return };
        if g.map.insert(key.clone(), (Instant::now(), value)).is_none() {
            g.order.push_back(key);
        }
        while g.map.len() > self.cap {
            match g.order.pop_front() {
                Some(old) => {
                    g.map.remove(&old);
                }
                None => break,
            }
        }
    }

    pub fn len(&self) -> usize {
        self.inner.lock().map(|g| g.map.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evicts_oldest_and_expires() {
        let c = Cache::new(2, Duration::from_millis(40));
        c.put("a".into(), 1);
        c.put("b".into(), 2);
        c.put("c".into(), 3);
        assert_eq!((c.get("a"), c.get("b"), c.get("c")), (None, Some(2), Some(3)));
        std::thread::sleep(Duration::from_millis(60));
        assert_eq!(c.get("b"), None);
    }
}
