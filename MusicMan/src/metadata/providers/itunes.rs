use super::{MetadataProvider, TrackMetadata, TrackQuery};
use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use std::time::Duration;
use tokio::time::sleep;

pub struct ItunesProvider {
    client: Client,
    rate_limit: Duration,
}

impl ItunesProvider {
    pub fn new(rate_limit_ms: u64) -> Self {
        Self {
            client: Client::new(),
            rate_limit: Duration::from_millis(rate_limit_ms),
        }
    }
}

#[async_trait]
impl MetadataProvider for ItunesProvider {
    fn name(&self) -> &str {
        "itunes"
    }

    async fn fetch(&self, query: &TrackQuery) -> Result<Option<TrackMetadata>> {
        sleep(self.rate_limit).await;

        let title = match &query.title {
            Some(t) => t.clone(),
            None => return Ok(None),
        };

        let mut term = title.clone();
        if let Some(artist) = &query.artist {
            term.push(' ');
            term.push_str(artist);
        }

        let url = format!(
            "https://itunes.apple.com/search?term={}&entity=song&limit=1",
            urlencoding::encode(&term)
        );

        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Ok(None);
        }

        let json: serde_json::Value = resp.json().await?;
        let result = match json["results"].as_array().and_then(|a| a.first()) {
            Some(r) => r,
            None => return Ok(None),
        };

        let fetched_title = result["trackName"].as_str().map(String::from);
        let artist = result["artistName"].as_str().map(String::from);
        let album = result["collectionName"].as_str().map(String::from);
        let year = result["releaseDate"]
            .as_str()
            .and_then(|d| d.split('-').next())
            .and_then(|y| y.parse::<u32>().ok());
        let track_number = result["trackNumber"].as_u64().map(|n| n as u32);
        let genre = result["primaryGenreName"].as_str().map(String::from);

        // iTunes returns 100x100 art; bump to 600x600
        let album_art_url = result["artworkUrl100"]
            .as_str()
            .map(|u| u.replace("100x100bb", "600x600bb"));

        Ok(Some(TrackMetadata {
            title: fetched_title,
            artist,
            album,
            year,
            track_number,
            genre,
            album_art_url: Some(album_art_url.unwrap_or_default()),
            ..Default::default()
        }))
    }
}
