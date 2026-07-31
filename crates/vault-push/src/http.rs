//! Blocking HTTP helpers for vault auth, asset upload, and NDJSON import.

use std::path::Path;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use reqwest::blocking::Client;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct AuthInfo {
    pub account_id: String,
    pub username: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AuthCheckResponse {
    ok: bool,
    #[serde(default)]
    account_id: Option<String>,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    account_ok: Option<bool>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AssetPutResponse {
    pub ok: bool,
    #[serde(default)]
    pub already_present: bool,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ImportResponse {
    pub ok: bool,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub messages: u64,
    #[serde(default)]
    pub messages_appended: u64,
    #[serde(default)]
    #[allow(dead_code)]
    pub messages_deduped: u64,
    #[serde(default)]
    #[allow(dead_code)]
    pub conversations: u64,
    #[serde(default)]
    #[allow(dead_code)]
    pub attachments: u64,
    #[serde(default)]
    #[allow(dead_code)]
    pub assets_copied: u64,
    #[serde(default)]
    #[allow(dead_code)]
    pub assets_missing: u64,
}

#[derive(Clone)]
pub struct HttpSession {
    client: Client,
}

pub struct AssetPutRequest<'a> {
    pub base_url: &'a str,
    pub key: &'a str,
    pub username: &'a str,
    pub source: &'a str,
    pub sha256: &'a str,
    pub file: &'a Path,
    pub mime: Option<&'a str>,
}

impl HttpSession {
    pub fn new() -> Result<Self> {
        let client = Client::builder()
            .pool_max_idle_per_host(16)
            .build()
            .context("build HTTP client")?;
        Ok(Self { client })
    }
}

fn looks_like_html(body: &str) -> bool {
    let t = body.trim_start();
    t.starts_with("<!DOCTYPE") || t.starts_with("<html") || t.starts_with("<HTML")
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max])
    }
}

fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

impl HttpSession {
    pub fn auth_check(&self, base_url: &str, key: &str, username: &str) -> Result<AuthInfo> {
        let base = base_url.trim_end_matches('/');
        let url = format!("{base}/v1/auth/check?account={}", encode(username.trim()));
        let response = self
            .client
            .get(&url)
            .timeout(Duration::from_secs(15))
            .header("Authorization", format!("Bearer {}", key.trim()))
            .send()
            .with_context(|| format!("GET {url}"))?;
        let status = response.status();
        let text = response.text().context("read auth/check body")?;
        if looks_like_html(&text) {
            bail!(
                "auth/check returned HTML from {url} (HTTP {status}). \
                 Vault URL must point at `message-vault-rs serve` (usually port 8080), \
                 not the Next.js browse UI (port 3000)"
            );
        }
        if status.as_u16() == 401 {
            bail!("invalid vault key");
        }
        if status.as_u16() == 403 {
            bail!("username does not match vault key: {text}");
        }
        if !status.is_success() {
            bail!("auth/check failed (HTTP {status}): {text}");
        }
        let parsed: AuthCheckResponse = serde_json::from_str(&text).with_context(|| {
            format!(
                "parse auth/check JSON from {url} (HTTP {status}): {}",
                truncate(&text, 200)
            )
        })?;
        if !parsed.ok {
            bail!("auth/check rejected: {}", parsed.error.unwrap_or(text));
        }
        if parsed.account_ok == Some(false) {
            bail!("account not found for username {username}");
        }
        let account_id = parsed
            .account_id
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow::anyhow!("auth/check did not return account_id"))?;
        Ok(AuthInfo {
            account_id,
            username: parsed.username,
        })
    }

    pub fn put_asset(&self, request: AssetPutRequest<'_>) -> Result<AssetPutResponse> {
        let base = request.base_url.trim_end_matches('/');
        let url = format!(
            "{base}/v1/assets/{}?source={}&account={}",
            encode(request.sha256),
            encode(request.source),
            encode(request.username)
        );
        let bytes = std::fs::read(request.file)
            .with_context(|| format!("read {}", request.file.display()))?;
        let mut req = self
            .client
            .put(&url)
            .timeout(Duration::from_secs(600))
            .header("Authorization", format!("Bearer {}", request.key.trim()))
            .body(bytes);
        if let Some(mime) = request.mime.filter(|m| !m.is_empty()) {
            req = req.header("Content-Type", mime);
        } else {
            req = req.header("Content-Type", "application/octet-stream");
        }
        let response = req.send().with_context(|| format!("PUT {url}"))?;
        let status = response.status();
        let text = response.text().context("read asset response")?;
        let parsed: AssetPutResponse = serde_json::from_str(&text).unwrap_or(AssetPutResponse {
            ok: false,
            already_present: false,
            error: Some(text.clone()),
        });
        if !status.is_success() || !parsed.ok {
            bail!(
                "{}",
                parsed
                    .error
                    .unwrap_or_else(|| format!("HTTP {status}: {text}"))
            );
        }
        Ok(parsed)
    }

    pub fn post_import(
        &self,
        base_url: &str,
        key: &str,
        username: &str,
        source: &str,
        mode: &str,
        ndjson: Vec<u8>,
    ) -> Result<ImportResponse> {
        let base = base_url.trim_end_matches('/');
        let url = format!(
            "{base}/v1/import?source={}&account={}&mode={}",
            encode(source),
            encode(username),
            encode(mode)
        );
        let response = self
            .client
            .post(&url)
            .timeout(Duration::from_secs(600))
            .header("Authorization", format!("Bearer {}", key.trim()))
            .header("Content-Type", "application/jsonl")
            .body(ndjson)
            .send()
            .with_context(|| format!("POST {url}"))?;
        let status = response.status();
        let text = response.text().context("read import response")?;
        let parsed: ImportResponse = serde_json::from_str(&text).unwrap_or(ImportResponse {
            ok: false,
            error: Some(text.clone()),
            messages: 0,
            messages_appended: 0,
            messages_deduped: 0,
            conversations: 0,
            attachments: 0,
            assets_copied: 0,
            assets_missing: 0,
        });
        if !status.is_success() || !parsed.ok {
            bail!(
                "{}",
                parsed
                    .error
                    .unwrap_or_else(|| format!("HTTP {status}: {text}"))
            );
        }
        Ok(parsed)
    }
}

pub fn auth_check(base_url: &str, key: &str, username: &str) -> Result<AuthInfo> {
    HttpSession::new()?.auth_check(base_url, key, username)
}

pub fn with_retries<T, F>(max_retries: u32, mut op: F) -> Result<T>
where
    F: FnMut() -> Result<T>,
{
    let mut attempt = 0u32;
    loop {
        attempt += 1;
        match op() {
            Ok(v) => return Ok(v),
            Err(e) => {
                if attempt > max_retries {
                    return Err(e);
                }
                thread::sleep(Duration::from_secs(u64::from(attempt)));
            }
        }
    }
}
