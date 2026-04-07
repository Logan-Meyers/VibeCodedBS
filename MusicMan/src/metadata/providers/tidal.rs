use super::{MetadataProvider, TrackMetadata, TrackQuery};
use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;

pub struct TidalProvider {
    client: Client,
    client_id: String,
    client_secret: String,
    // Token cached after auth
    access_token: tokio::sync::Mutex<Option<String>>,
}

impl TidalProvider {
    pub fn new(client_id: &str, client_secret: &str) -> Self {
        Self {
            client: Client::new(),
            client_id: client_id.to_string(),
            client_secret: client_secret.to_string(),
            access_token: tokio::sync::Mutex::new(None),
        }
    }

    pub fn is_configured(&self) -> bool {
        !self.client_id.is_empty() && !self.client_secret.is_empty()
    }

    async fn get_token(&self) -> Result<String> {
        let mut token = self.access_token.lock().await;
        if let Some(t) = token.as_ref() {
            return Ok(t.clone());
        }

        // Client credentials flow
        let resp = self
            .client
            .post("https://auth.tidal.com/v1/oauth2/token")
            .basic_auth(&self.client_id, Some(&self.client_secret))
            .form(&[("grant_type", "client_credentials")])
            .send()
            .await?;

        let json: serde_json::Value = resp.json().await?;
        let t = json["access_token"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("No access token in Tidal response"))?
            .to_string();

        *token = Some(t.clone());
        Ok(t)
    }
}

#[async_trait]
impl MetadataProvider for TidalProvider {
    fn name(&self) -> &str {
        "tidal"
    }

    async fn fetch(&self, query: &TrackQuery) -> Result<Option<TrackMetadata>> {
        if !self.is_configured() {
            return Ok(None);
        }

        let title = match &query.title {
            Some(t) => t.clone(),
            None => return Ok(None),
        };

        let token = self.get_token().await?;

        let mut search_term = title.clone();
        if let Some(artist) = &query.artist {
            search_term.push(' ');
            search_term.push_str(artist);
        }

        let resp = self
            .client
            .get("https://openapi.tidal.com/v2/searchresults")
            .bearer_auth(&token)
            .query(&[
                ("query", search_term.as_str()),
                ("include", "tracks"),
                ("countryCode", "US"),
            ])
            .send()
            .await?;

        if !resp.status().is_success() {
            return Ok(None);
        }

        let json: serde_json::Value = resp.json().await?;
        let track = json["data"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|d| d["attributes"].as_object());

        match track {
            None => Ok(None),
            Some(t) => {
                let fetched_title = t.get("title").and_then(|v| v.as_str()).map(String::from);
                let artist = json["included"]
                    .as_array()
                    .and_then(|a| {
                        a.iter().find(|i| i["type"] == "artists")
                    })
                    .and_then(|a| a["attributes"]["name"].as_str())
                    .map(String::from);

                Ok(Some(TrackMetadata {
                    title: fetched_title,
                    artist,
                    ..Default::default()
                }))
            }
        }
    }
}
