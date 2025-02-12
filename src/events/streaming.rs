use anyhow::Context as _;
use poise::serenity_prelude::{
    all::{ActivityType, Presence},
    Context,
};

use crate::{
    config::{GUILD_ID, STREAMING_ROLE},
    Result,
};

pub async fn update_streaming_role_status(ctx: &Context, new_data: &Presence) -> Result<()> {
    let Some(guild_id) = new_data.guild_id else {
        return Ok(());
    };

    if guild_id != GUILD_ID {
        return Ok(());
    }

    let has_streaming_activity = new_data
        .activities
        .iter()
        .any(|a| a.kind == ActivityType::Streaming);

    if has_streaming_activity {
        ctx.http
            .add_member_role(guild_id, new_data.user.id, STREAMING_ROLE, None)
            .await
            .context("Failed to add streaming role")
    } else {
        ctx.http
            .remove_member_role(guild_id, new_data.user.id, STREAMING_ROLE, None)
            .await
            .context("Failed to remove streaming role")
    }
}
