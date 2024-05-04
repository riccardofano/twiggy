use crate::{common::ephemeral_reply, Context, Result};

use std::sync::atomic::{AtomicI64, Ordering};

use anyhow::bail;
use chrono::Utc;
use reqwest::{StatusCode, Url};

// This looks dumb but it tells me that I'm using seconds
// instead of just being a random number
const ASK_COOLDOWN: i64 = std::time::Duration::from_secs(600).as_secs() as i64;

/// Ask a question to Wolfram Alpha
#[poise::command(slash_command, prefix_command, custom_data = "AtomicI64::new(0)")]
pub async fn ask(
    ctx: Context<'_>,
    #[description = "The question you want to ask"] question: String,
) -> Result<()> {
    let last_called = ctx
        .command()
        .custom_data
        .downcast_ref::<AtomicI64>()
        .expect("Expected the command to have the last use timestamp");

    let now = Utc::now().timestamp();
    if last_called.load(Ordering::Relaxed) + ASK_COOLDOWN > now {
        ctx.send(ephemeral_reply("The ask command is on cooldown."))
            .await?;
        return Ok(());
    }

    let Ok(wolfram_app_id) = std::env::var("WOLFRAM_APP_ID") else {
        let msg = "The `ask` command does not work without a Wolfram App ID.";
        ctx.say(msg).await?;
        return Ok(());
    };

    // Update cooldown
    last_called.store(now, Ordering::Relaxed);

    let Some(answer) = fetch_answer(&question, &wolfram_app_id).await? else {
        ctx.say("The bot was not able to answer").await?;
        return Ok(());
    };

    ctx.say(format!("[{question}] {answer}")).await?;

    Ok(())
}

async fn fetch_answer(question: &str, app_id: &str) -> Result<Option<String>> {
    let url = Url::parse_with_params(
        "https://api.wolframalpha.com/v1/result",
        &[("appid", app_id), ("i", question), ("units", "metric")],
    )?;
    let response = reqwest::get(url).await?;

    match response.status() {
        StatusCode::OK => {
            let answer = response.text().await?;
            Ok(Some(answer))
        }
        StatusCode::NOT_IMPLEMENTED => Ok(None),
        StatusCode::BAD_REQUEST => bail!("Wolfram Alpha parameters were not set correctly"),
        s => bail!("Something went wrong. Status: {s}"),
    }
}
