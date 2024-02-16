use crate::{Context, Result};

use anyhow::bail;
use chrono::{NaiveDateTime, Utc};
use rand::seq::SliceRandom;
use serde::Deserialize;

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct QuoteResponse {
    error: bool,
    #[serde(rename = "type")]
    kind: usize,
    total: usize,
    data: Vec<Quote>,
}

#[derive(Debug, Deserialize)]
pub struct Quote {
    id: u64,
    body: String,
}

pub struct QuoteData {
    cache: Vec<Quote>,
    last_updated: NaiveDateTime,
}

impl QuoteData {
    pub fn new() -> Self {
        Self {
            cache: Vec::new(),
            last_updated: NaiveDateTime::MIN,
        }
    }
}

/// Get a random or specific quote
#[poise::command(slash_command, prefix_command)]
pub async fn quote(
    ctx: Context<'_>,
    #[description = "Quote ID"] quote_id: Option<u64>,
) -> Result<()> {
    let message = match generate_message(&ctx, quote_id).await {
        Ok(message) => message,
        Err(e) => e.to_string(),
    };

    ctx.say(message).await?;

    Ok(())
}

async fn generate_message(ctx: &Context<'_>, quote_id: Option<u64>) -> Result<String> {
    update_quotes(ctx).await;

    let quotes = &ctx.data().quote_data.read().await.cache;
    if quotes.is_empty() {
        bail!("Could not retrieve any quotes"));
    }

    let quote = match quote_id {
        Some(id) => quotes
            .iter()
            .find(|q| q.id == id)
            .ok_or(anyhow::anyhow!("Unable to find quote #{id}"))?,
        None => {
            let mut rng = rand::thread_rng();
            quotes.choose(&mut rng).expect("Quotes to not be empty")
        }
    };

    Ok(format!("[{}] {}", quote.id, quote.body))
}

async fn update_quotes(ctx: &Context<'_>) {
    let last_updated = ctx.data().quote_data.read().await.last_updated;

    if cache_expired(last_updated) {
        let response = match fetch_quotes().await {
            Ok(response) => response,
            Err(e) => {
                eprintln!("Failed to fetch quotes: {e:?}");
                return;
            }
        };

        let mut data = ctx.data().quote_data.write().await;
        data.cache = response.data;
        data.last_updated = Utc::now().naive_utc();
    }
}

fn cache_expired(last_updated: NaiveDateTime) -> bool {
    let now = Utc::now().naive_utc();
    let cache_duration = chrono::Duration::minutes(30);

    now > last_updated + cache_duration
}

async fn fetch_quotes() -> std::result::Result<QuoteResponse, reqwest::Error> {
    reqwest::get("https://api.bufutda.com/bot/quote?channel=bananasaurus_rex")
        .await?
        .json::<QuoteResponse>()
        .await
}
