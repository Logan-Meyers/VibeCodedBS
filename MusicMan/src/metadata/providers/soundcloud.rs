use super::{MetadataProvider, TrackMetadata, TrackQuery};
use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;

pub struct SoundCloudProvider {
    client: Client,
    client_id: String,
}

impl SoundCloudProvider {
    pub fn new(client_id: &str) -> Self {
        Self {
            client: Client::new(),
            client_id: client_id.to_string(),
        }
    }

    pub fn is_configured(&self) -> bool {
        !self.client_id.is_empty()
    }
}

#[async_trait]
impl MetadataProvider for SoundCloudProvider {
    fn name(&self) -> &str {
        "soundcloud"
    }

    async fn fetch(&self, query: &TrackQuery) -> Result<Option<TrackMetadata>> {
        if !self.is_configured() {
            return Ok(None);
        }

        let title = match &query.title {
            Some(t) => t.clone(),
            None => return Ok(None),
        };

        let resp = self
            .client
            .get("https://api.soundcloud.com/tracks")
            .query(&[
                ("q", title.as_str()),
                ("client_id", self.client_id.as_str()),
                ("limit", "1"),
            ])
            .send()
            .await?;

        if !resp.status().is_success() {
            return Ok(None);
        }

        let json: serde_json::Value = resp.json().await?;
        let track = match json.as_array().and_then(|a| a.first()) {
            Some(t) => t,
            None => return Ok(None),
        };

        let fetched_title = track["title"].as_str().map(String::from);
        let artist = track["user"]["username"].as_str().map(String::from);
        let genre = track["genre"].as_str().map(String::from);
        let album_art_url = track["artwork_url"]
            .as_str()
            .map(|u| u.replace("large", "t500x500"));

        Ok(Some(TrackMetadata {
            title: fetched_title,
            artist,
            genre,
            album_art_url,
            ..Default::default()
        }))
    }
}
