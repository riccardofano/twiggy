use std::sync::OnceLock;

use anyhow::{ensure, Context as AnyhowContext};
use reqwest::Client;

use crate::{common::bail_reply, Context, Result};

const URL_GAME_SEARCH: &str = "https://api.isthereanydeal.com/games/search/v1";
const URL_GAME_PRICES: &str = "https://api.isthereanydeal.com/games/prices/v3";
const BASE_GAME_PAGE_URL: &str = "https://isthereanydeal.com/game";

pub static ITAD_CLIENT_ID: OnceLock<String> = OnceLock::new();

#[poise::command(slash_command, prefix_command, global_cooldown = 30)]
pub async fn itad(
    ctx: Context<'_>,
    #[description = "The game you want to search"] game: String,
) -> Result<()> {
    let deals = match get_deals(&game, ITAD_CLIENT_ID.get().unwrap()).await {
        Ok(deals) => deals,
        Err(err) => return bail_reply(ctx, err.to_string()).await,
    };

    ctx.say(deals).await?;

    Ok(())
}

async fn get_deals(game: &str, client_id: &str) -> Result<String> {
    let client = Client::new();
    let search_response = client
        .get(URL_GAME_SEARCH)
        .query(&[("key", client_id), ("title", game), ("results", "1")])
        .send()
        .await
        .context("Was not able to send request for game search")?
        .error_for_status()
        .context("Bad response to game search")?;

    let search_result = search_response
        .json::<Vec<Game>>()
        .await
        .context("Failed to deserialize response into expected format")?;

    ensure!(
        search_result.len() == 1,
        "Could not find any games for that query"
    );

    let search_result_ids = search_result
        .iter()
        .map(|r| r.id.as_ref())
        .collect::<Vec<&str>>();

    let deals_response = client
        .post(URL_GAME_PRICES)
        .query(&[("key", client_id), ("country", "US"), ("deals", "true")])
        .json(&search_result_ids)
        .send()
        .await
        .context("Was not able to send request for game deals")?
        .error_for_status()
        .context("Something went wrong when trying to retrieve deals for this game")?;

    let deals_result = deals_response
        .json::<Vec<PricesResponse>>()
        .await
        .context("Failed to deserialize deals response into expected format")?;

    ensure!(
        deals_result.len() == 1 && !deals_result[0].deals.is_empty(),
        "Could not find any deals for this game"
    );

    let prices = deals_result[0]
        .deals
        .iter()
        .map(|d| format!("{}: ${:.2}", d.shop.name, d.price.amount))
        .collect::<Vec<_>>()
        .join("; ");

    Ok(format!(
        "{prices}\n{}/{}/info",
        BASE_GAME_PAGE_URL, search_result[0].slug
    ))
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct Game {
    id: String,
    slug: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct PricesResponse {
    id: String,
    deals: Vec<Deal>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct Deal {
    price: Price,
    shop: Shop,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct Price {
    amount: f64,
    currency: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct Shop {
    id: u64,
    name: String,
}
