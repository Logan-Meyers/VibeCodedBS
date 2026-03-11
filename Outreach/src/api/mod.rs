use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};

const GRAPH_BASE: &str = "https://graph.microsoft.com/v1.0";

/// Minimal email representation from Graph API
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Email {
    pub id: String,
    pub subject: Option<String>,
    pub body_preview: Option<String>,
    pub received_at: Option<DateTime<Utc>>,
    pub is_read: bool,
    pub from: Option<EmailAddress>,
    pub body: Option<EmailBody>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EmailAddress {
    pub name: Option<String>,
    pub address: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EmailBody {
    pub content_type: String,
    pub content: String,
}

/// Raw Graph API response shapes
#[derive(Deserialize)]
struct GraphListResponse<T> {
    value: Vec<T>,
    #[serde(rename = "@odata.nextLink")]
    next_link: Option<String>,
}

#[derive(Deserialize)]
struct GraphMessage {
    id: String,
    subject: Option<String>,
    #[serde(rename = "bodyPreview")]
    body_preview: Option<String>,
    #[serde(rename = "receivedDateTime")]
    received_date_time: Option<DateTime<Utc>>,
    #[serde(rename = "isRead")]
    is_read: bool,
    from: Option<GraphRecipient>,
    body: Option<GraphBody>,
}

#[derive(Deserialize)]
struct GraphRecipient {
    #[serde(rename = "emailAddress")]
    email_address: Option<GraphEmailAddress>,
}

#[derive(Deserialize)]
struct GraphEmailAddress {
    name: Option<String>,
    address: Option<String>,
}

#[derive(Deserialize)]
struct GraphBody {
    #[serde(rename = "contentType")]
    content_type: String,
    content: String,
}

fn to_email(msg: GraphMessage) -> Email {
    Email {
        id: msg.id,
        subject: msg.subject,
        body_preview: msg.body_preview,
        received_at: msg.received_date_time,
        is_read: msg.is_read,
        from: msg.from.and_then(|r| r.email_address).map(|a| EmailAddress {
            name: a.name,
            address: a.address,
        }),
        body: msg.body.map(|b| EmailBody {
            content_type: b.content_type,
            content: b.content,
        }),
    }
}

pub struct GraphClient {
    client: Client,
    access_token: String,
}

impl GraphClient {
    pub fn new(access_token: String) -> Self {
        Self {
            client: Client::new(),
            access_token,
        }
    }

    async fn get<T: for<'de> Deserialize<'de>>(&self, url: &str) -> Result<T> {
        let resp = self
            .client
            .get(url)
            .bearer_auth(&self.access_token)
            .send()
            .await?;

        if resp.status() == StatusCode::UNAUTHORIZED {
            return Err(anyhow!("Unauthorized — token may be expired"));
        }

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Graph API error {}: {}", status, body));
        }

        Ok(resp.json::<T>().await?)
    }

    /// Fetch the latest emails from the inbox (up to `limit`)
    pub async fn list_inbox(&self, limit: usize) -> Result<Vec<Email>> {
        let url = format!(
            "{}/me/mailFolders/inbox/messages?$top={}&$orderby=receivedDateTime desc&$select=id,subject,bodyPreview,receivedDateTime,isRead,from",
            GRAPH_BASE, limit
        );

        let resp: GraphListResponse<GraphMessage> = self.get(&url).await?;
        Ok(resp.value.into_iter().map(to_email).collect())
    }

    /// Fetch a single email with full body
    pub async fn get_email(&self, id: &str) -> Result<Email> {
        let url = format!(
            "{}/me/messages/{}?$select=id,subject,body,receivedDateTime,isRead,from",
            GRAPH_BASE, id
        );
        let msg: GraphMessage = self.get(&url).await?;
        Ok(to_email(msg))
    }

    /// Mark an email as read
    pub async fn mark_read(&self, id: &str) -> Result<()> {
        self.client
            .patch(&format!("{}/me/messages/{}", GRAPH_BASE, id))
            .bearer_auth(&self.access_token)
            .json(&serde_json::json!({ "isRead": true }))
            .send()
            .await?;
        Ok(())
    }

    /// Delete an email (moves to Deleted Items)
    pub async fn delete_email(&self, id: &str) -> Result<()> {
        self.client
            .delete(&format!("{}/me/messages/{}", GRAPH_BASE, id))
            .bearer_auth(&self.access_token)
            .send()
            .await?;
        Ok(())
    }

    /// Send a reply to an email
    pub async fn reply(&self, id: &str, body: &str) -> Result<()> {
        self.client
            .post(&format!("{}/me/messages/{}/reply", GRAPH_BASE, id))
            .bearer_auth(&self.access_token)
            .json(&serde_json::json!({
                "message": {},
                "comment": body
            }))
            .send()
            .await?;
        Ok(())
    }
}
