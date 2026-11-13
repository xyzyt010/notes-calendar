//! Password-derived local vault encryption.
//! PBKDF2-HMAC-SHA256 derives a single 256-bit key; AES-256-GCM protects text
//! and binary attachments. Empty strings stay empty, matching the frontend.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::{engine::general_purpose::STANDARD, Engine};
use pbkdf2::pbkdf2_hmac;
use rand::RngCore;
use sha2::Sha256;
use zeroize::{Zeroize, Zeroizing};

const ENC_PREFIX: &str = "v1.";
const VERIFY_PREFIX: &str = "notes-verify:local";
pub const PBKDF2_ITERS: u32 = 600_000;

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct VaultConfig {
    pub enabled: bool,
    pub salt: String,
    pub verifier: String,
}

#[derive(Clone, PartialEq, Eq)]
pub struct VaultKey([u8; 32]);

impl VaultKey {
    pub fn bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Debug for VaultKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("VaultKey(<redacted>)")
    }
}

impl Drop for VaultKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

pub fn looks_encrypted(value: &str) -> bool {
    value.starts_with(ENC_PREFIX)
}

pub fn derive_key(password: &str, salt: &[u8]) -> VaultKey {
    let mut out = Zeroizing::new([0u8; 32]);
    pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, PBKDF2_ITERS, out.as_mut());
    VaultKey(*out)
}

pub fn new_vault(password: &str) -> (VaultKey, VaultConfig) {
    let mut salt = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut salt);
    let key = derive_key(password, &salt);
    let verifier = encrypt_text(&key, VERIFY_PREFIX).unwrap_or_default();
    (
        key,
        VaultConfig {
            enabled: true,
            salt: STANDARD.encode(salt),
            verifier,
        },
    )
}

pub fn unlock(password: &str, cfg: &VaultConfig) -> anyhow::Result<VaultKey> {
    if !cfg.enabled || cfg.salt.is_empty() {
        anyhow::bail!("vault is not configured")
    }
    let salt = STANDARD.decode(&cfg.salt)?;
    let key = derive_key(password, &salt);
    let plain = decrypt_text(&key, &cfg.verifier).map_err(|_| anyhow::anyhow!("wrong password"))?;
    if plain != VERIFY_PREFIX {
        anyhow::bail!("wrong password");
    }
    Ok(key)
}

pub fn change_vault(password: &str, cfg: &VaultConfig) -> (VaultKey, VaultConfig) {
    let _ = cfg;
    new_vault(password)
}

pub fn encrypt_text(key: &VaultKey, plain: &str) -> anyhow::Result<String> {
    if plain.is_empty() {
        return Ok(String::new());
    }
    let mut iv = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut iv);
    let cipher = Aes256Gcm::new_from_slice(key.bytes()).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let ct = cipher.encrypt(Nonce::from_slice(&iv), plain.as_bytes()).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    Ok(format!("{ENC_PREFIX}{}.{}", STANDARD.encode(iv), STANDARD.encode(ct)))
}

pub fn decrypt_text(key: &VaultKey, encrypted: &str) -> anyhow::Result<String> {
    if encrypted.is_empty() || !encrypted.starts_with(ENC_PREFIX) {
        return Ok(encrypted.to_string()); // legacy/plain text
    }
    let rest = &encrypted[ENC_PREFIX.len()..];
    let mut parts = rest.splitn(2, '.');
    let iv = STANDARD.decode(parts.next().unwrap_or_default())?;
    let ct = STANDARD.decode(parts.next().unwrap_or_default())?;
    if iv.len() != 12 {
        anyhow::bail!("invalid vault nonce");
    }
    let cipher = Aes256Gcm::new_from_slice(key.bytes()).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let pt = cipher.decrypt(Nonce::from_slice(&iv), ct.as_ref()).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    Ok(String::from_utf8(pt)?)
}

pub fn encrypt_bytes(key: &VaultKey, plain: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut iv = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut iv);
    let cipher = Aes256Gcm::new_from_slice(key.bytes()).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let ct = cipher.encrypt(Nonce::from_slice(&iv), plain).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let mut out = Vec::with_capacity(12 + ct.len());
    out.extend_from_slice(&iv);
    out.extend_from_slice(&ct);
    Ok(out)
}

pub fn decrypt_bytes(key: &VaultKey, encrypted: &[u8]) -> anyhow::Result<Vec<u8>> {
    if encrypted.len() < 29 {
        anyhow::bail!("invalid encrypted blob");
    }
    let (iv, ct) = encrypted.split_at(12);
    let cipher = Aes256Gcm::new_from_slice(key.bytes()).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    cipher.decrypt(Nonce::from_slice(iv), ct).map_err(|e| anyhow::anyhow!(e.to_string()))
}
