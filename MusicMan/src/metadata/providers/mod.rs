pub mod itunes;
pub mod musicbrainz;
pub mod soundcloud;
pub mod tidal;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrackQuery {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration_secs: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrackMetadata {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub track_number: Option<u32>,
    pub year: Option<u32>,
    pub genre: Option<String>,
    pub album_art_url: Option<String>,
}

#[async_trait]
pub trait MetadataProvider: Send + Sync {
    fn name(&self) -> &str;
    async fn fetch(&self, query: &TrackQuery) -> Result<Option<TrackMetadata>>;
}

/// Tries providers in order, returns first successful result
pub struct ProviderChain {
    providers: Vec<Box<dyn MetadataProvider>>,
}

impl ProviderChain {
    pub fn new(providers: Vec<Box<dyn MetadataProvider>>) -> Self {
        Self { providers }
    }

    pub async fn fetch(&self, query: &TrackQuery) -> Option<(String, TrackMetadata)> {
        for provider in &self.providers {
            match provider.fetch(query).await {
                Ok(Some(meta)) => {
                    return Some((provider.name().to_string(), meta));
                }
                Ok(None) => {
                    tracing::debug!("{}: no result for {:?}", provider.name(), query.title);
                }
                Err(e) => {
                    tracing::warn!("{}: error: {}", provider.name(), e);
                }
            }
        }
        None
    }
}
