//! The LAN view's HTTPS identity: a self-signed certificate, made once and
//! kept in `%APPDATA%\winsploit` so its fingerprint stays the same and each
//! guest device only clicks through the browser's warning once.
//!
//! There is no certificate authority on a LAN, so browsers can't verify it
//! on their own. What they can do is show its SHA-256 fingerprint on the
//! warning page; the terminal prints the same fingerprint, and a guest who
//! compares them knows they're talking to this machine.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::ServerConfig;

/// Files under the settings directory.
const CERT_FILE: &str = "lan-cert.pem";
const KEY_FILE: &str = "lan-key.pem";
/// How long a certificate is valid. Browsers ignore the dates of a
/// certificate they don't trust anyway, but stay well inside the 825 days
/// some of them cap trusted ones at, in case a user does trust it.
const VALID_DAYS: i64 = 730;
/// Regenerate this long before it runs out.
const RENEW_DAYS: u64 = 700;

/// What the server needs for TLS, and what the terminal shows.
pub struct Identity {
    pub config: Arc<ServerConfig>,
    /// SHA-256 of the certificate, upper-case hex pairs joined by `:`
    /// (Firefox's format; Chrome shows the same pairs with spaces).
    pub fingerprint: String,
}

/// Load the saved certificate, or make (and save) a new one. `dir: None`
/// keeps it in memory only (tests, or no `%APPDATA%`).
pub fn load_or_create(dir: Option<&Path>) -> Result<Identity> {
    if let Some(dir) = dir {
        let (cert, key) = (dir.join(CERT_FILE), dir.join(KEY_FILE));
        if fresh(&cert) {
            if let Ok(id) = load(&cert, &key) {
                return Ok(id);
            }
        }
        let (cert_pem, key_pem) = generate()?;
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        std::fs::write(&key, &key_pem).with_context(|| format!("writing {}", key.display()))?;
        std::fs::write(&cert, &cert_pem).with_context(|| format!("writing {}", cert.display()))?;
        return from_pem(cert_pem.as_bytes(), key_pem.as_bytes());
    }
    let (cert_pem, key_pem) = generate()?;
    from_pem(cert_pem.as_bytes(), key_pem.as_bytes())
}

/// `%APPDATA%\winsploit`, where settings.yaml lives too.
pub fn default_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("winsploit"))
}

/// The certificate file exists and isn't close to running out.
fn fresh(cert: &Path) -> bool {
    let age = std::fs::metadata(cert)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok());
    age.is_some_and(|a| a < Duration::from_secs(RENEW_DAYS * 24 * 3600))
}

fn load(cert: &Path, key: &Path) -> Result<Identity> {
    let cert = std::fs::read(cert)?;
    let key = std::fs::read(key)?;
    from_pem(&cert, &key)
}

/// A new ECDSA P-256 key and a certificate for it, both as PEM. The names
/// are only informative: browsers show the warning either way.
fn generate() -> Result<(String, String)> {
    let mut params = rcgen::CertificateParams::new(vec!["localhost".to_string()])
        .context("certificate parameters")?;
    let host = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "this PC".into());
    params.distinguished_name.push(
        rcgen::DnType::CommonName,
        format!("Winsploit LAN view on {host}"),
    );
    params
        .distinguished_name
        .push(rcgen::DnType::OrganizationName, "Winsploit (self-signed)");
    for ip in [
        std::net::IpAddr::from([127, 0, 0, 1]),
        std::net::IpAddr::from(std::net::Ipv6Addr::LOCALHOST),
    ]
    .into_iter()
    .chain(crate::web::lan_ip())
    {
        params.subject_alt_names.push(rcgen::SanType::IpAddress(ip));
    }
    let now = time::OffsetDateTime::now_utc();
    params.not_before = now - time::Duration::days(1);
    params.not_after = now + time::Duration::days(VALID_DAYS);
    let key = rcgen::KeyPair::generate().context("generating a key")?;
    let cert = params
        .self_signed(&key)
        .context("signing the certificate")?;
    Ok((cert.pem(), key.serialize_pem()))
}

fn from_pem(cert_pem: &[u8], key_pem: &[u8]) -> Result<Identity> {
    let cert = CertificateDer::from_pem_slice(cert_pem).context("reading the certificate")?;
    let key = PrivateKeyDer::from_pem_slice(key_pem).context("reading the private key")?;
    let fingerprint = fingerprint(&cert);
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .context("TLS versions")?
        .with_no_client_auth()
        .with_single_cert(vec![cert], key)
        .context("TLS certificate")?;
    Ok(Identity {
        config: Arc::new(config),
        fingerprint,
    })
}

fn fingerprint(cert: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, cert)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn makes_saves_and_reuses_a_certificate() {
        let dir = std::env::temp_dir().join(format!("winsploit-tls-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let a = load_or_create(Some(&dir)).unwrap();
        assert!(dir.join(CERT_FILE).is_file() && dir.join(KEY_FILE).is_file());
        // the same certificate next time: guests don't see a new warning
        let b = load_or_create(Some(&dir)).unwrap();
        assert_eq!(a.fingerprint, b.fingerprint);
        assert_eq!(a.fingerprint.len(), 32 * 3 - 1);
        // a broken file is replaced, not fatal
        std::fs::write(dir.join(KEY_FILE), "garbage").unwrap();
        let c = load_or_create(Some(&dir)).unwrap();
        assert_ne!(a.fingerprint, c.fingerprint);
        let _ = std::fs::remove_dir_all(&dir);
        // in memory: a new one each time
        let m = load_or_create(None).unwrap();
        assert_ne!(m.fingerprint, c.fingerprint);
    }
}
