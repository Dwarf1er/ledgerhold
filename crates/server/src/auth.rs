//! The household password, signed session cookies, first-run setup and login throttling.
//!
//! The password lives in one of three states:
//! * **Open**: no authentication (loopback only, or explicitly allowed).
//! * **Setup**: nothing is configured yet. A random one-time code is printed to the server log and
//!   whoever can read it may choose the password. A stranger who finds the port first gets nothing.
//! * **Password**: an argon2 hash stored in the data directory (or derived from `LEDGERHOLD_PASSWORD`,
//!   which then can't be changed from the UI).
//!
//! A session token is `<expiry-unix>.<hex hmac-sha256(session-key, expiry)>`: stateless, so a restart
//! doesn't log people out. The session key is derived from the stored signing key *and* the current
//! password hash, so changing the password signs everybody out.

use anyhow::{Context, Result, anyhow};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng};
use argon2::Argon2;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};
use std::time::{Duration, Instant};

type HmacSha256 = Hmac<Sha256>;

pub const COOKIE: &str = "lh_session";
pub const SESSION_SECS: i64 = 30 * 24 * 3600;
pub const MIN_PASSWORD: usize = 8;
const MAX_PASSWORD: usize = 256;
const MAX_FAILURES: usize = 10;
const WINDOW: Duration = Duration::from_secs(60);
/// No 0/O or 1/I, so a code read off a log is hard to mistype.
const CODE_ALPHABET: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

enum Credential {
    Open,
    Setup { code: String },
    /// `binding` ties sessions to the current password: the stored hash itself, or for an environment
    /// password (re-hashed with a new salt at every start) a keyed digest of the password.
    Password { phc: String, binding: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Open,
    Setup,
    Password,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SetupError {
    AlreadyDone,
    BadCode,
}

pub struct Auth {
    key: Vec<u8>,
    cred: RwLock<Credential>,
    /// Where the hash is persisted. `None` = the password comes from the environment.
    hash_file: Option<PathBuf>,
    failures: Mutex<VecDeque<Instant>>,
}

/// Where the password hash lives for a given data directory.
pub fn hash_path(data_dir: &Path) -> PathBuf {
    data_dir.join("password.hash")
}

pub fn validate_password(p: &str) -> Result<(), String> {
    match p.chars().count() {
        n if n < MIN_PASSWORD => Err(format!("the password must be at least {MIN_PASSWORD} characters")),
        n if n > MAX_PASSWORD => Err(format!("the password can be at most {MAX_PASSWORD} characters")),
        _ => Ok(()),
    }
}

pub fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default().hash_password(password.as_bytes(), &salt).map(|h| h.to_string()).map_err(|e| anyhow!("hashing the password: {e}"))
}

fn verify_phc(phc: &str, password: &str) -> bool {
    PasswordHash::new(phc).is_ok_and(|h| Argon2::default().verify_password(password.as_bytes(), &h).is_ok())
}

fn mac(key: &[u8], parts: &[&[u8]]) -> HmacSha256 {
    let mut m = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    for p in parts {
        m.update(p);
    }
    m
}

fn random_code() -> String {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
    bytes.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
    // 12 symbols from a 32-letter alphabet is 60 bits; `& 31` is unbiased for a power-of-two alphabet.
    let symbols: Vec<char> = bytes.iter().take(12).map(|b| CODE_ALPHABET[(*b & 31) as usize] as char).collect();
    symbols.chunks(4).map(|c| c.iter().collect::<String>()).collect::<Vec<_>>().join("-")
}

/// Compare ignoring case, dashes and spaces so a hand-typed code still works.
fn normalise_code(c: &str) -> String {
    c.chars().filter(|ch| ch.is_ascii_alphanumeric()).map(|ch| ch.to_ascii_uppercase()).collect()
}

impl Auth {
    pub fn open(key: Vec<u8>) -> Auth {
        Auth::from(Credential::Open, key, None)
    }

    /// First run: generate a one-time setup code.
    pub fn setup(key: Vec<u8>, hash_file: PathBuf) -> Auth {
        Auth::from(Credential::Setup { code: random_code() }, key, Some(hash_file))
    }

    /// A password that is managed outside the app (environment variable); the UI can't change it.
    pub fn with_env_password(password: &str, key: Vec<u8>) -> Result<Auth> {
        let binding = hex::encode(mac(&key, &[b"env-password:", password.as_bytes()]).finalize().into_bytes());
        Ok(Auth::from(Credential::Password { phc: hash_password(password)?, binding }, key, None))
    }

    /// A previously stored hash.
    pub fn with_stored_hash(phc: String, key: Vec<u8>, hash_file: PathBuf) -> Result<Auth> {
        PasswordHash::new(&phc).map_err(|e| anyhow!("{} is not a valid password hash ({e})", hash_file.display()))?;
        Ok(Auth::from(Credential::Password { binding: phc.clone(), phc }, key, Some(hash_file)))
    }

    fn from(cred: Credential, key: Vec<u8>, hash_file: Option<PathBuf>) -> Auth {
        Auth { key, cred: RwLock::new(cred), hash_file, failures: Mutex::new(VecDeque::new()) }
    }

    pub fn mode(&self) -> Mode {
        match &*self.cred.read().unwrap_or_else(|e| e.into_inner()) {
            Credential::Open => Mode::Open,
            Credential::Setup { .. } => Mode::Setup,
            Credential::Password { .. } => Mode::Password,
        }
    }

    /// Whether requests need to be authenticated (true during setup: nobody is signed in yet).
    pub fn required(&self) -> bool {
        self.mode() != Mode::Open
    }

    pub fn setup_required(&self) -> bool {
        self.mode() == Mode::Setup
    }

    /// The password can be changed from the UI only when the app owns it.
    pub fn can_change_password(&self) -> bool {
        self.mode() == Mode::Password && self.hash_file.is_some()
    }

    /// The one-time code to print in the server log, while in setup mode.
    pub fn setup_code(&self) -> Option<String> {
        match &*self.cred.read().unwrap_or_else(|e| e.into_inner()) {
            Credential::Setup { code } => Some(code.clone()),
            _ => None,
        }
    }

    /// Check the household password (slow by design: run it off the async threads).
    pub fn check_password(&self, supplied: &str) -> bool {
        let phc = match &*self.cred.read().unwrap_or_else(|e| e.into_inner()) {
            Credential::Open => return true,
            Credential::Setup { .. } => return false,
            Credential::Password { phc, .. } => phc.clone(),
        };
        verify_phc(&phc, supplied)
    }

    /// Constant-time check of the setup code.
    pub fn check_setup_code(&self, supplied: &str) -> bool {
        match &*self.cred.read().unwrap_or_else(|e| e.into_inner()) {
            Credential::Setup { code } => {
                let want = mac(&self.key, &[b"setup:", normalise_code(code).as_bytes()]).finalize().into_bytes();
                mac(&self.key, &[b"setup:", normalise_code(supplied).as_bytes()]).verify_slice(&want).is_ok()
            }
            _ => false,
        }
    }

    /// Finish first-run setup. The code is re-checked under the write lock, so two simultaneous
    /// attempts can't both win and the code can't be used twice.
    pub fn complete_setup(&self, code: &str, phc: String) -> Result<(), SetupError> {
        let mut cred = self.cred.write().unwrap_or_else(|e| e.into_inner());
        let Credential::Setup { code: want } = &*cred else { return Err(SetupError::AlreadyDone) };
        if normalise_code(want) != normalise_code(code) {
            return Err(SetupError::BadCode);
        }
        if let Some(path) = &self.hash_file {
            // If we can't persist, stay in setup mode rather than running with a password that vanishes on restart.
            persist(path, &phc).map_err(|e| {
                eprintln!("could not save the password: {e:#}");
                SetupError::AlreadyDone
            })?;
        }
        *cred = Credential::Password { binding: phc.clone(), phc };
        Ok(())
    }

    /// Replace the stored password (the caller has already verified the old one).
    pub fn change_password(&self, phc: String) -> Result<()> {
        let mut cred = self.cred.write().unwrap_or_else(|e| e.into_inner());
        let (Credential::Password { .. }, Some(path)) = (&*cred, &self.hash_file) else {
            anyhow::bail!("the password is managed outside Ledgerhold")
        };
        persist(path, &phc)?;
        *cred = Credential::Password { binding: phc.clone(), phc };
        Ok(())
    }

    fn session_mac(&self, parts: &[&[u8]]) -> HmacSha256 {
        let key = match &*self.cred.read().unwrap_or_else(|e| e.into_inner()) {
            Credential::Password { binding, .. } => mac(&self.key, &[b"session-key:", binding.as_bytes()]).finalize().into_bytes().to_vec(),
            _ => self.key.clone(),
        };
        mac(&key, parts)
    }

    pub fn issue(&self, now_unix: i64) -> String {
        let exp = (now_unix + SESSION_SECS).to_string();
        let sig = hex::encode(self.session_mac(&[b"session:", exp.as_bytes()]).finalize().into_bytes());
        format!("{exp}.{sig}")
    }

    pub fn verify(&self, token: &str, now_unix: i64) -> bool {
        let Some((exp, sig)) = token.split_once('.') else { return false };
        let Ok(exp_n) = exp.parse::<i64>() else { return false };
        let Ok(sig) = hex::decode(sig) else { return false };
        exp_n > now_unix && self.session_mac(&[b"session:", exp.as_bytes()]).verify_slice(&sig).is_ok()
    }

    /// True once too many bad attempts happened in the last minute.
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

/// Write `bytes` to a file that is owner-only from the moment it exists (no chmod-after-write window).
pub fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(path).with_context(|| format!("writing {}", path.display()))?;
    f.write_all(bytes).with_context(|| format!("writing {}", path.display()))
}

/// Write the hash atomically with owner-only permissions.
fn persist(path: &Path, phc: &str) -> Result<()> {
    let tmp = path.with_extension("hash.tmp");
    write_private(&tmp, format!("{phc}\n").as_bytes())?;
    std::fs::rename(&tmp, path).with_context(|| format!("saving {}", path.display()))
}

/// The stored hash, if any. A file that exists but is empty or unreadable is an error, never "no password".
pub fn load_hash(path: &Path) -> Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(s) if s.trim().is_empty() => anyhow::bail!("{} is empty; run `ledgerhold-server reset-password`", path.display()),
        Ok(s) => Ok(Some(s.trim().to_string())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

/// Extract our cookie from a `Cookie:` header value.
pub fn cookie_value(header: &str) -> Option<&str> {
    header.split(';').filter_map(|kv| kv.trim().split_once('=')).find(|(k, _)| *k == COOKIE).map(|(_, v)| v)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &[u8] = b"0123456789abcdef0123456789abcdef";

    fn auth() -> Auth {
        Auth::with_env_password("correct horse", KEY.to_vec()).unwrap()
    }

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("lh-auth-{name}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d.join("password.hash")
    }

    #[test]
    fn password_check() {
        let a = auth();
        assert!(a.check_password("correct horse"));
        assert!(!a.check_password("correct hors"));
        assert!(!a.check_password(""));
        assert!(Auth::open(vec![1]).check_password("anything"), "open = no password");
        assert!(!Auth::open(vec![1]).required());
        assert!(!Auth::setup(vec![1], scratch("x")).check_password("anything"), "nothing is accepted during setup");
    }

    #[test]
    fn passwords_are_stored_as_salted_argon2_hashes() {
        let (a, b) = (hash_password("same password").unwrap(), hash_password("same password").unwrap());
        assert!(a.starts_with("$argon2"));
        assert_ne!(a, b, "a fresh salt each time");
        assert!(!a.contains("same password"));
    }

    #[test]
    fn password_rules() {
        assert!(validate_password("1234567").is_err());
        assert!(validate_password("12345678").is_ok());
        assert!(validate_password("é".repeat(8).as_str()).is_ok(), "counted in characters, not bytes");
        assert!(validate_password(&"x".repeat(257)).is_err());
    }

    #[test]
    fn setup_codes_look_right_and_differ() {
        let (a, b) = (random_code(), random_code());
        assert_ne!(a, b);
        let groups: Vec<&str> = a.split('-').collect();
        assert_eq!(groups.len(), 3);
        assert!(groups.iter().all(|g| g.len() == 4), "{a}");
        assert!(a.chars().all(|c| c == '-' || CODE_ALPHABET.contains(&(c as u8))));
    }

    #[test]
    fn setup_accepts_the_code_once_and_tolerates_typing_differences() {
        let path = scratch("once");
        let a = Auth::setup(KEY.to_vec(), path.clone());
        assert!(a.setup_required() && a.required());
        let code = a.setup_code().unwrap();
        assert!(!a.check_setup_code("AAAA-AAAA-AAAA"));
        assert_eq!(a.complete_setup("AAAA-AAAA-AAAA", hash_password("hunter2hunter2").unwrap()), Err(SetupError::BadCode));
        assert!(a.setup_required(), "a wrong code changes nothing");

        let sloppy = code.to_lowercase().replace('-', " ");
        assert!(a.check_setup_code(&sloppy));
        assert_eq!(a.complete_setup(&sloppy, hash_password("hunter2hunter2").unwrap()), Ok(()));
        assert_eq!(a.mode(), Mode::Password);
        assert!(a.check_password("hunter2hunter2"));
        assert!(a.setup_code().is_none());
        assert_eq!(a.complete_setup(&code, hash_password("another-one-1").unwrap()), Err(SetupError::AlreadyDone), "the code is single use");
        assert!(a.check_password("hunter2hunter2"), "the second attempt didn't replace the password");
        assert!(load_hash(&path).unwrap().unwrap().starts_with("$argon2"), "it is persisted");
    }

    #[test]
    fn a_failed_save_keeps_setup_open() {
        let a = Auth::setup(KEY.to_vec(), PathBuf::from("/nonexistent-dir-for-ledgerhold-tests/password.hash"));
        let code = a.setup_code().unwrap();
        assert!(a.complete_setup(&code, hash_password("hunter2hunter2").unwrap()).is_err());
        assert!(a.setup_required());
    }

    #[test]
    fn changing_the_password_signs_everyone_out() {
        let path = scratch("change");
        let a = Auth::with_stored_hash(hash_password("old password 1").unwrap(), KEY.to_vec(), path.clone()).unwrap();
        let token = a.issue(1_000);
        assert!(a.verify(&token, 1_000));
        assert!(a.can_change_password());
        a.change_password(hash_password("new password 2").unwrap()).unwrap();
        assert!(!a.verify(&token, 1_000), "old sessions stop working");
        assert!(a.verify(&a.issue(1_000), 1_000));
        assert!(a.check_password("new password 2") && !a.check_password("old password 1"));
        assert!(verify_phc(&load_hash(&path).unwrap().unwrap(), "new password 2"));
    }

    #[test]
    fn an_environment_password_cannot_be_changed_here() {
        let a = auth();
        assert!(!a.can_change_password());
        assert!(a.change_password(hash_password("new password 2").unwrap()).is_err());
        assert!(a.check_password("correct horse"));
    }

    #[test]
    fn stored_hash_handling() {
        let path = scratch("load");
        assert_eq!(load_hash(&path).unwrap(), None);
        std::fs::write(&path, "  \n").unwrap();
        assert!(load_hash(&path).is_err(), "an empty file is never treated as 'no password'");
        assert!(Auth::with_stored_hash("garbage".into(), KEY.to_vec(), path).is_err());
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
        let other = Auth::with_env_password("correct horse", b"another-key-another-key-another-k".to_vec()).unwrap();
        assert!(!other.verify(&t, 1_000), "a token from a different key");
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
