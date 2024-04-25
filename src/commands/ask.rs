use crate::{Context, Result};

use anyhow::bail;
use reqwest::{StatusCode, Url};

/// Ask a question to Wolfram Alpha
#[poise::command(slash_command, prefix_command)]
pub async fn ask(
    ctx: Context<'_>,
    #[description = "The question you want to ask"] question: String,
) -> Result<()> {
    let Ok(wolfram_app_id) = std::env::var("WOLFRAM_APP_ID") else {
        let msg = "The `ask` command does not work without a Wolfram App ID.";
        ctx.say(msg).await?;
        return Ok(());
    };

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
