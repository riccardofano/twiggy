use chrono::{Duration, Utc};
use rand::Rng;

use crate::common::{bail_reply, name};
use crate::{Context, Result};

/// Commit sudoku
#[poise::command(guild_only, slash_command, prefix_command)]
pub async fn sudoku(
    ctx: Context<'_>,
    #[description = "Your final words"] message: Option<String>,
) -> Result<()> {
    let guild = ctx
        .partial_guild()
        .await
        .expect("Expected /sudoku to be guild only.");

    if ctx.author().id == guild.owner_id {
        let msg = "Sadly I cannot time out the owner of the server.";
        return bail_reply(ctx, msg).await;
    }

    let Some(mut member) = ctx.author_member().await else {
        return bail_reply(ctx, "Could not get your member details.").await;
    };

    let random_timeout = {
        let mut rng = rand::thread_rng();
        rng.gen_range(420..=690)
    };

    let now = Utc::now();
    let timeout_until = now + Duration::seconds(random_timeout);
    member
        .to_mut()
        .disable_communication_until_datetime(ctx, timeout_until.into())
        .await?;

    let goodbye_message = match message {
        Some(message) => format!("\n> {message}"),
        None => String::new(),
    };
    let author_name = name(&ctx, ctx.author()).await;

    ctx.say(format!(
        "{author_name} has been timed out for {random_timeout} seconds, or until <t:{}:T>.{goodbye_message}",
        timeout_until.timestamp()
    ))
    .await?;

    Ok(())
}
