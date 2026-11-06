//! One shared household password, signed session cookies, and login throttling.
//!
//! A session token is `<expiry-unix>.<hex hmac-sha256(key, expiry)>`: stateless, so a restart
//! doesn't log people out as long as the signing key (persisted in the data dir) survives.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

type HmacSha256 = Hmac<Sha256>;

pub const COOKIE: &str = "lh_session";
pub const SESSION_SECS: i64 = 30 * 24 * 3600;
const MAX_FAILURES: usize = 10;
const WINDOW: Duration = Duration::from_secs(60);

pub struct Auth {
    key: Vec<u8>,
    /// `None` = authentication disabled (loopback or explicitly allowed).
    password_tag: Option<Vec<u8>>,
    failures: Mutex<VecDeque<Instant>>,
}

impl Auth {
    pub fn new(password: Option<&str>, key: Vec<u8>) -> Auth {
        let password_tag = password.map(|p| Self::mac(&key, &[b"pw:", p.as_bytes()]).finalize().into_bytes().to_vec());
        Auth { key, password_tag, failures: Mutex::new(VecDeque::new()) }
    }

    fn mac(key: &[u8], parts: &[&[u8]]) -> HmacSha256 {
        let mut m = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
        for p in parts {
            m.update(p);
        }
        m
    }

    pub fn required(&self) -> bool {
        self.password_tag.is_some()
    }

    /// Constant-time password check.
    pub fn check_password(&self, supplied: &str) -> bool {
        match &self.password_tag {
            Some(tag) => Self::mac(&self.key, &[b"pw:", supplied.as_bytes()]).verify_slice(tag).is_ok(),
            None => true,
        }
    }

    pub fn issue(&self, now_unix: i64) -> String {
        let exp = (now_unix + SESSION_SECS).to_string();
        let sig = hex::encode(Self::mac(&self.key, &[b"session:", exp.as_bytes()]).finalize().into_bytes());
        format!("{exp}.{sig}")
    }

    pub fn verify(&self, token: &str, now_unix: i64) -> bool {
        let Some((exp, sig)) = token.split_once('.') else { return false };
        let Ok(exp_n) = exp.parse::<i64>() else { return false };
        let Ok(sig) = hex::decode(sig) else { return false };
        exp_n > now_unix && Self::mac(&self.key, &[b"session:", exp.as_bytes()]).verify_slice(&sig).is_ok()
    }

    /// True once too many bad logins happened in the last minute.
    pub fn throttled(&self) -> bool {
        let mut f = self.failures.lock().unwrap_or_else(|e| e.into_inner());
        Self::prune(&mut f);
        f.len() >= MAX_FAILURES
    }

    pub fn record_failure(&self) {
        let mut f = self.failures.lock().unwrap_or_else(|e| e.into_inner());
        Self::prune(&mut f);
        f.push_back(Instant::now());
    }

    fn prune(f: &mut VecDeque<Instant>) {
        while f.front().is_some_and(|t| t.elapsed() > WINDOW) {
            f.pop_front();
        }
    }
}

/// Extract our cookie from a `Cookie:` header value.
pub fn cookie_value(header: &str) -> Option<&str> {
    header.split(';').filter_map(|kv| kv.trim().split_once('=')).find(|(k, _)| *k == COOKIE).map(|(_, v)| v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auth() -> Auth {
        Auth::new(Some("correct horse"), b"0123456789abcdef0123456789abcdef".to_vec())
    }

    #[test]
    fn password_check() {
        let a = auth();
        assert!(a.check_password("correct horse"));
        assert!(!a.check_password("correct hors"));
        assert!(!a.check_password(""));
        assert!(Auth::new(None, vec![1]).check_password("anything"), "no password configured = open");
        assert!(!Auth::new(None, vec![1]).required());
    }

    #[test]
    fn tokens_verify_until_expiry() {
        let a = auth();
        let t = a.issue(1_000);
        assert!(a.verify(&t, 1_000));
        assert!(a.verify(&t, 1_000 + SESSION_SECS - 1));
        assert!(!a.verify(&t, 1_000 + SESSION_SECS + 1), "expired");
    }

    #[test]
    fn tampered_or_foreign_tokens_are_rejected() {
        let a = auth();
        let t = a.issue(1_000);
        let (exp, sig) = t.split_once('.').unwrap();
        let later = format!("{}.{sig}", exp.parse::<i64>().unwrap() + 1000);
        assert!(!a.verify(&later, 1_000), "extending the expiry invalidates the signature");
        assert!(!a.verify(&format!("{exp}.{}", "0".repeat(sig.len())), 1_000));
        for junk in ["", "abc", ".", "1.", ".zz", "x.y", "9999999999.nothex"] {
            assert!(!a.verify(junk, 1_000), "{junk:?}");
        }
        let other = Auth::new(Some("correct horse"), b"another-key-another-key-another-k".to_vec());
        assert!(!other.verify(&t, 1_000), "a token from a different key");
    }

    #[test]
    fn a_session_token_is_not_a_valid_password_tag() {
        // Domain separation: nobody can turn a session signature into a login.
        let a = auth();
        let sig_of_exp = a.issue(0);
        assert!(!a.check_password(&sig_of_exp));
    }

    #[test]
    fn throttling_kicks_in_after_repeated_failures() {
        let a = auth();
        for _ in 0..MAX_FAILURES - 1 {
            a.record_failure();
        }
        assert!(!a.throttled());
        a.record_failure();
        assert!(a.throttled());
    }

    #[test]
    fn cookie_parsing() {
        assert_eq!(cookie_value("a=1; lh_session=tok.en; b=2"), Some("tok.en"));
        assert_eq!(cookie_value("lh_session=x"), Some("x"));
        assert_eq!(cookie_value("other=1"), None);
        assert_eq!(cookie_value(""), None);
    }
}
