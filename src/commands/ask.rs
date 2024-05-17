use crate::{common::ephemeral_reply, Context, Result};

use std::sync::atomic::{AtomicI64, Ordering};

use anyhow::bail;
use chrono::Utc;
use poise::CreateReply;
use reqwest::{StatusCode, Url};
use serenity::all::CreateEmbed;

const UNIT_STR: [&str; 2] = ["imperial", "metric"];

#[derive(poise::ChoiceParameter)]
pub enum Unit {
    Imperial,
    Metric,
}

// This looks dumb but it tells me that I'm using seconds
// instead of just being a random number
const ASK_COOLDOWN: i64 = std::time::Duration::from_secs(10).as_secs() as i64;

/// Ask a question to Wolfram Alpha
#[poise::command(slash_command, prefix_command, custom_data = "AtomicI64::new(0)")]
pub async fn ask(
    ctx: Context<'_>,
    #[description = "The question you want to ask"] question: String,
    #[description = "The units of measurement"] units: Option<Unit>,
) -> Result<()> {
    let Ok(wolfram_app_id) = std::env::var("WOLFRAM_APP_ID") else {
        let msg = "The `ask` command does not work without a Wolfram App ID.";
        ctx.say(msg).await?;
        return Ok(());
    };

    if let Err(cooldown_msg) = update_cooldown(ctx).await {
        ctx.send(ephemeral_reply(cooldown_msg.to_string())).await?;
        return Ok(());
    }

    let answer = fetch_answer(&wolfram_app_id, &question, units).await?;

    let embed = CreateEmbed::default()
        .title(truncate(question, 256))
        .description(truncate(answer, 4096))
        .color(0xFBAB00);
    ctx.send(CreateReply::default().embed(embed)).await?;

    Ok(())
}

async fn fetch_answer(app_id: &str, question: &str, units: Option<Unit>) -> Result<String> {
    let unit = units.unwrap_or(Unit::Metric);
    let url = Url::parse_with_params(
        "https://api.wolframalpha.com/v1/result",
        &[
            ("appid", app_id),
            ("i", question),
            ("units", UNIT_STR[unit as usize]),
        ],
    )?;

    let response = reqwest::get(url).await?;
    match response.status() {
        StatusCode::OK => Ok(response.text().await?),
        StatusCode::NOT_IMPLEMENTED => Ok("The bot was not able to answer.".to_owned()),
        StatusCode::BAD_REQUEST => bail!("Wolfram Alpha parameters were not set correctly"),
        _ => bail!("Something went wrong. Response: {response:?}"),
    }
}

async fn update_cooldown(ctx: Context<'_>) -> Result<()> {
    let last_called = ctx
        .command()
        .custom_data
        .downcast_ref::<AtomicI64>()
        .expect("Expected the command to have the last use timestamp");

    let now = Utc::now().timestamp();
    let cooldown_end = last_called.load(Ordering::Relaxed) + ASK_COOLDOWN;
    if cooldown_end > now {
        bail!("The command will be off cooldown <t:{cooldown_end}:R>");
    }

    last_called.store(now, Ordering::Relaxed);
    Ok(())
}

fn truncate(string: String, max_length: usize) -> String {
    if string.len() <= max_length {
        return string;
    }

    format!("{}...", &string[..max_length - 3])
}
