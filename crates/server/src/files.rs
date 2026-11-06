//! Temporary files for web imports (uploads) and exports (downloads).
//!
//! Clients never name a path: an upload yields an opaque token `upload:<uuid>/<file>`, an
//! export session is a uuid, and every name is re-validated against the sanitiser before it is
//! joined to a directory, so `..` and absolute paths cannot escape the temp root.

use anyhow::{Context, Result, bail};
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use uuid::Uuid;

pub const ZIP_NAME: &str = "all.zip";

pub struct Files {
    uploads: PathBuf,
    exports: PathBuf,
}

/// Keep letters, digits and a few harmless symbols; everything else becomes `_`.
pub fn sanitize(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = base
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ' | '(' | ')') { c } else { '_' })
        .take(120)
        .collect();
    let cleaned = cleaned.trim_matches(|c| c == '.' || c == ' ').to_string();
    if cleaned.is_empty() { "file".into() } else { cleaned }
}

fn valid_name(name: &str) -> bool {
    !name.is_empty() && sanitize(name) == name
}

fn parse_id(id: &str) -> Result<Uuid> {
    Uuid::parse_str(id).context("invalid id")
}

impl Files {
    /// Creates (and empties) `<data_dir>/tmp/{uploads,exports}`.
    pub fn new(data_dir: &Path) -> Result<Files> {
        let root = data_dir.join("tmp");
        let _ = std::fs::remove_dir_all(&root);
        let (uploads, exports) = (root.join("uploads"), root.join("exports"));
        std::fs::create_dir_all(&uploads).with_context(|| format!("creating {}", uploads.display()))?;
        std::fs::create_dir_all(&exports)?;
        Ok(Files { uploads, exports })
    }

    pub fn save_upload(&self, name: &str, bytes: &[u8]) -> Result<String> {
        let id = Uuid::new_v4();
        let name = sanitize(name);
        let dir = self.uploads.join(id.to_string());
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join(&name), bytes)?;
        Ok(format!("upload:{id}/{name}"))
    }

    /// Map an upload token back to its file. Anything else is refused.
    pub fn resolve_upload(&self, token: &str) -> Result<PathBuf> {
        let rest = token.strip_prefix("upload:").context("not an uploaded file; upload it first")?;
        let (id, name) = rest.split_once('/').context("malformed upload token")?;
        let id = parse_id(id)?;
        if !valid_name(name) {
            bail!("malformed upload token");
        }
        let path = self.uploads.join(id.to_string()).join(name);
        if !path.is_file() {
            bail!("the uploaded file is no longer available; upload it again");
        }
        Ok(path)
    }

    /// Directory for an export session (a uuid chosen by the client, so several
    /// exports can accumulate into one zip).
    pub fn export_dir(&self, session: &str) -> Result<PathBuf> {
        let id = parse_id(session)?;
        let dir = self.exports.join(id.to_string());
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    pub fn export_file(&self, session: &str, name: &str) -> Result<PathBuf> {
        let id = parse_id(session)?;
        if !valid_name(name) {
            bail!("invalid file name");
        }
        let path = self.exports.join(id.to_string()).join(name);
        if !path.is_file() {
            bail!("no such file");
        }
        Ok(path)
    }

    pub fn zip_session(&self, session: &str) -> Result<Vec<u8>> {
        let id = parse_id(session)?;
        let dir = self.exports.join(id.to_string());
        let mut names: Vec<String> = std::fs::read_dir(&dir)
            .with_context(|| "no such export")?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();
        names.sort();
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for n in names {
            zip.start_file(&n, opts)?;
            zip.write_all(&std::fs::read(dir.join(&n))?)?;
        }
        Ok(zip.finish()?.into_inner())
    }

    /// Delete uploads and exports older than `max_age`.
    pub fn sweep(&self, max_age: Duration) {
        for root in [&self.uploads, &self.exports] {
            let Ok(entries) = std::fs::read_dir(root) else { continue };
            for e in entries.flatten() {
                let old = e
                    .metadata()
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| SystemTime::now().duration_since(t).ok())
                    .is_some_and(|age| age > max_age);
                if old {
                    let _ = std::fs::remove_dir_all(e.path());
                }
            }
        }
    }

    /// Drop an upload once it has been imported.
    pub fn discard_upload(&self, token: &str) {
        if let Ok(p) = self.resolve_upload(token)
            && let Some(dir) = p.parent()
        {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files() -> (Files, PathBuf) {
        let dir = std::env::temp_dir().join(format!("lh-files-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        (Files::new(&dir).unwrap(), dir)
    }

    #[test]
    fn sanitize_strips_paths_and_odd_characters() {
        assert_eq!(sanitize("../../etc/passwd"), "passwd");
        assert_eq!(sanitize("C:\\Users\\me\\releve.csv"), "releve.csv");
        assert_eq!(sanitize("relevé <1>.ofx"), "relevé _1_.ofx");
        assert_eq!(sanitize(""), "file");
        assert_eq!(sanitize(".."), "file");
        assert_eq!(sanitize(&"a".repeat(500)).len(), 120);
    }

    #[test]
    fn upload_roundtrip_and_cleanup() {
        let (f, dir) = files();
        let token = f.save_upload("My Statement (1).csv", b"x").unwrap();
        assert!(token.starts_with("upload:") && token.ends_with("/My Statement (1).csv"));
        let p = f.resolve_upload(&token).unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"x");
        f.discard_upload(&token);
        assert!(f.resolve_upload(&token).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn tokens_cannot_escape_the_upload_directory() {
        let (f, dir) = files();
        let good = f.save_upload("a.csv", b"x").unwrap();
        let id = good.strip_prefix("upload:").unwrap().split('/').next().unwrap().to_string();
        for evil in [
            "/etc/passwd".to_string(),
            "upload:../../etc/passwd".to_string(),
            format!("upload:{id}/../../../etc/passwd"),
            format!("upload:{id}/..%2f..%2fetc"),
            format!("upload:{id}/a.csv/../a.csv"),
            "upload:not-a-uuid/a.csv".to_string(),
            format!("upload:{id}/"),
            format!("upload:{id}"),
            "relative/path.csv".to_string(),
        ] {
            assert!(f.resolve_upload(&evil).is_err(), "{evil}");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn export_sessions_zip_and_validate() {
        let (f, dir) = files();
        let session = Uuid::new_v4().to_string();
        let d = f.export_dir(&session).unwrap();
        std::fs::write(d.join("b.ofx"), "BBB").unwrap();
        std::fs::write(d.join("a.ofx"), "AAA").unwrap();
        assert_eq!(std::fs::read(f.export_file(&session, "a.ofx").unwrap()).unwrap(), b"AAA");
        assert!(f.export_file(&session, "../a.ofx").is_err());
        assert!(f.export_file("nope", "a.ofx").is_err());
        let zip = f.zip_session(&session).unwrap();
        assert_eq!(&zip[..2], b"PK");
        let mut z = zip::ZipArchive::new(Cursor::new(zip)).unwrap();
        assert_eq!(z.len(), 2);
        assert_eq!(&*z.by_index(0).unwrap().name().unwrap(), "a.ofx", "sorted for stable output");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn sweep_removes_only_old_entries() {
        let (f, dir) = files();
        let t = f.save_upload("a.csv", b"x").unwrap();
        f.sweep(Duration::from_secs(3600));
        assert!(f.resolve_upload(&t).is_ok(), "fresh files survive");
        f.sweep(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(5));
        f.sweep(Duration::ZERO);
        assert!(f.resolve_upload(&t).is_err(), "everything older than the limit goes");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
