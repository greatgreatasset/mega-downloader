//! Real-Debrid transport client.
//!
//! The primary byte source: RD downloads from MEGA with no 5 GB cap and returns
//! direct, already-decrypted file links. We feed it *per-node* folder links
//! (`…/folder/<id>#<key>/file/<handle>`), which RD accepts — so each
//! unrestricted file maps back to an exact position in our tree.

use std::time::Duration;

use serde::Deserialize;

use crate::{Error, Result};

const API_BASE: &str = "https://api.real-debrid.com/rest/1.0";

/// Thin Real-Debrid API client. Holds the user's API token and a shared HTTP
/// client (reused for both API calls and the actual file downloads).
#[derive(Clone)]
pub struct RealDebrid {
    api_key: String,
    http: reqwest::Client,
}

/// Result of unrestricting a link: a direct, resumable download URL plus the
/// authoritative filename/size from RD.
#[derive(Debug, Clone, Deserialize)]
pub struct Unrestricted {
    pub download: String,
    pub filename: String,
    #[serde(default)]
    pub filesize: i64,
}

/// RD's structured error body, e.g. `{"error":"unavailable_file","error_code":24}`.
#[derive(Debug, Deserialize)]
struct RdError {
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    error_code: Option<i64>,
}

impl RealDebrid {
    pub fn new(api_key: impl Into<String>) -> Self {
        // No overall timeout (downloads legitimately run for hours), but a
        // read timeout so a silently stalled connection errors out and gets
        // retried instead of pinning a worker forever.
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(20))
            .read_timeout(Duration::from_secs(60))
            .build()
            .expect("failed to build HTTP client");
        Self {
            api_key: api_key.into(),
            http,
        }
    }

    /// Whether a usable token is configured.
    pub fn has_key(&self) -> bool {
        !self.api_key.is_empty()
    }

    /// Shared HTTP client, reused to stream the unrestricted download.
    pub fn client(&self) -> &reqwest::Client {
        &self.http
    }

    /// Unrestrict a (per-node) MEGA link into a direct download.
    pub async fn unrestrict(&self, link: &str) -> Result<Unrestricted> {
        let resp = self
            .http
            .post(format!("{API_BASE}/unrestrict/link"))
            .bearer_auth(&self.api_key)
            .form(&[("link", link)])
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            // Parse RD's structured error so callers can react to `error_code`
            // (e.g. 24 = unavailable_file, a transient cold-cache condition).
            let (code, message) = match serde_json::from_str::<RdError>(&body) {
                Ok(e) => (e.error_code, e.error.unwrap_or_else(|| body.clone())),
                Err(_) => (None, body),
            };
            return Err(Error::RealDebrid {
                status,
                code,
                message,
            });
        }

        Ok(resp.json().await?)
    }
}
