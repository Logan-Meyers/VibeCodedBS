use super::{MetadataProvider, TrackMetadata, TrackQuery};
use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use std::time::Duration;
use tokio::time::sleep;

pub struct MusicBrainzProvider {
    client: Client,
    user_agent: String,
    rate_limit: Duration,
}

impl MusicBrainzProvider {
    pub fn new(user_agent: &str, rate_limit_ms: u64) -> Self {
        Self {
            client: Client::new(),
            user_agent: user_agent.to_string(),
            rate_limit: Duration::from_millis(rate_limit_ms),
        }
    }
}

#[async_trait]
impl MetadataProvider for MusicBrainzProvider {
    fn name(&self) -> &str {
        "musicbrainz"
    }

    async fn fetch(&self, query: &TrackQuery) -> Result<Option<TrackMetadata>> {
        sleep(self.rate_limit).await;

        let title = match &query.title {
            Some(t) => t.clone(),
            None => return Ok(None),
        };

        let mut lucene = format!("recording:{}", title);
        if let Some(artist) = &query.artist {
            lucene.push_str(&format!(" AND artist:{}", artist));
        }

        let url = format!(
            "https://musicbrainz.org/ws/2/recording/?query={}&fmt=json&limit=1",
            urlencoding::encode(&lucene)
        );

        let resp = self
            .client
            .get(&url)
            .header("User-Agent", &self.user_agent)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Ok(None);
        }

        let json: serde_json::Value = resp.json().await?;
        let recording = match json["recordings"].as_array().and_then(|a| a.first()) {
            Some(r) => r,
            None => return Ok(None),
        };

        let fetched_title = recording["title"].as_str().map(String::from);
        let artist = recording["artist-credit"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|c| c["artist"]["name"].as_str())
            .map(String::from);

        let release = recording["releases"].as_array().and_then(|a| a.first());
        let album = release
            .and_then(|r| r["title"].as_str())
            .map(String::from);
        let year = release
            .and_then(|r| r["date"].as_str())
            .and_then(|d| d.split('-').next())
            .and_then(|y| y.parse::<u32>().ok());
        let track_number = release
            .and_then(|r| r["media"].as_array())
            .and_then(|m| m.first())
            .and_then(|m| m["track"].as_array())
            .and_then(|t| t.first())
            .and_then(|t| t["number"].as_str())
            .and_then(|n| n.parse::<u32>().ok());

        Ok(Some(TrackMetadata {
            title: fetched_title,
            artist,
            album,
            year,
            track_number,
            ..Default::default()
        }))
    }
}
