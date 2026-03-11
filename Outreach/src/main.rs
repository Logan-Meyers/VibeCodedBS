mod auth;
mod api;
mod config;
mod db;
mod tui;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    // Simple stderr logger; set RUST_LOG=debug for verbose output
    tracing_subscriber::fmt().with_writer(std::io::stderr).init();

    let config = config::Config::load()?;

    if config.auth.client_id == "YOUR_CLIENT_ID" {
        eprintln!("⚠️  No client ID configured.");
        eprintln!("   Register an Azure app at https://portal.azure.com/#blade/Microsoft_AAD_RegisteredApps");
        eprintln!("   Then edit: {}", config::Config::config_path().display());
        eprintln!("   Set auth.client_id and auth.tenant_id, then re-run.");
        std::process::exit(1);
    }

    let auth_client = auth::AuthClient::new(
        config.auth.client_id.clone(),
        config.auth.tenant_id.clone(),
    );
    let token = auth_client.load_or_login(&config::Config::token_path()).await?;

    let graph = api::GraphClient::new(token.access_token.clone());
    let db = db::Database::open(&config::Config::db_path())?;

    println!("Syncing inbox...");
    match graph.list_inbox(50).await {
        Ok(emails) => {
            db.upsert_emails(&emails)?;
            println!("✓ Synced {} emails.", emails.len());
        }
        Err(e) => {
            eprintln!("⚠️  Sync failed (showing cached): {}", e);
        }
    }

    tui::run(&db)?;
    Ok(())
}
