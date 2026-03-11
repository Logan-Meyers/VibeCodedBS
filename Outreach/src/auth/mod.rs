use anyhow::{anyhow, Result};
use chrono::{DateTime, Duration, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::Path;

const SCOPES: &str = "offline_access Mail.Read Mail.ReadWrite Mail.Send User.Read";

/// Persisted token stored on disk at ~/.config/outreach/token.json
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TokenCache {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: DateTime<Utc>,
}

impl TokenCache {
    pub fn is_expired(&self) -> bool {
        Utc::now() >= self.expires_at - Duration::seconds(60)
    }
}

/// Response from the device code endpoint
#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: u64,
    message: String,
}

/// Response from the token endpoint
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    error: Option<String>,
    error_description: Option<String>,
}

pub struct AuthClient {
    client: Client,
    client_id: String,
    tenant_id: String,
}

impl AuthClient {
    pub fn new(client_id: String, tenant_id: String) -> Self {
        Self {
            client: Client::new(),
            client_id,
            tenant_id,
        }
    }

    fn token_url(&self) -> String {
        format!(
            "https://login.microsoftonline.com/{}/oauth2/v2.0/token",
            self.tenant_id
        )
    }

    fn device_code_url(&self) -> String {
        format!(
            "https://login.microsoftonline.com/{}/oauth2/v2.0/devicecode",
            self.tenant_id
        )
    }

    /// Start the device code flow. Prints instructions to stdout.
    pub async fn login(&self) -> Result<TokenCache> {
        let resp = self
            .client
            .post(&self.device_code_url())
            .form(&[("client_id", &self.client_id), ("scope", &SCOPES.to_string())])
            .send()
            .await?
            .json::<DeviceCodeResponse>()
            .await?;

        println!("\n{}\n", resp.message);

        // Attempt to open browser automatically; ignore errors (headless env)
        let _ = open::that(&resp.verification_uri);

        self.poll_for_token(&resp.device_code, resp.interval, resp.expires_in)
            .await
    }

    /// Poll until the user completes login or the code expires
    async fn poll_for_token(
        &self,
        device_code: &str,
        interval: u64,
        expires_in: u64,
    ) -> Result<TokenCache> {
        let deadline = Utc::now() + Duration::seconds(expires_in as i64);

        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(interval)).await;

            if Utc::now() > deadline {
                return Err(anyhow!("Device code expired. Please try logging in again."));
            }

            let resp = self
                .client
                .post(&self.token_url())
                .form(&[
                    ("client_id", self.client_id.as_str()),
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                    ("device_code", device_code),
                ])
                .send()
                .await?
                .json::<TokenResponse>()
                .await?;

            match resp.error.as_deref() {
                None => {
                    let cache = TokenCache {
                        access_token: resp.access_token.ok_or_else(|| anyhow!("No access token"))?,
                        refresh_token: resp.refresh_token.ok_or_else(|| anyhow!("No refresh token"))?,
                        expires_at: Utc::now() + Duration::seconds(resp.expires_in.unwrap_or(3600) as i64),
                    };
                    println!("✓ Logged in successfully.");
                    return Ok(cache);
                }
                Some("authorization_pending") => {
                    // Still waiting on user — keep polling
                    print!(".");
                }
                Some("slow_down") => {
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                }
                Some(err) => {
                    return Err(anyhow!(
                        "Auth error: {} — {}",
                        err,
                        resp.error_description.unwrap_or_default()
                    ));
                }
            }
        }
    }

    /// Refresh an existing token
    pub async fn refresh(&self, cache: &TokenCache) -> Result<TokenCache> {
        let resp = self
            .client
            .post(&self.token_url())
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("grant_type", "refresh_token"),
                ("refresh_token", cache.refresh_token.as_str()),
                ("scope", SCOPES),
            ])
            .send()
            .await?
            .json::<TokenResponse>()
            .await?;

        if let Some(err) = resp.error {
            return Err(anyhow!("Token refresh failed: {}", err));
        }

        Ok(TokenCache {
            access_token: resp.access_token.ok_or_else(|| anyhow!("No access token"))?,
            refresh_token: resp.refresh_token.unwrap_or_else(|| cache.refresh_token.clone()),
            expires_at: Utc::now() + Duration::seconds(resp.expires_in.unwrap_or(3600) as i64),
        })
    }

    /// Load token from disk, refreshing if expired
    pub async fn load_or_login(&self, token_path: &Path) -> Result<TokenCache> {
        if token_path.exists() {
            let content = std::fs::read_to_string(token_path)?;
            let mut cache: TokenCache = serde_json::from_str(&content)?;

            if cache.is_expired() {
                println!("Token expired, refreshing...");
                cache = self.refresh(&cache).await?;
                save_token(&cache, token_path)?;
            }

            return Ok(cache);
        }

        let cache = self.login().await?;
        save_token(&cache, token_path)?;
        Ok(cache)
    }
}

pub fn save_token(cache: &TokenCache, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(cache)?)?;
    Ok(())
}
