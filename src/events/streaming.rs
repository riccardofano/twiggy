use anyhow::Context as AnyhowContext;
use poise::serenity_prelude::{
    all::{ActivityType, Presence, RoleId},
    Context,
};
use serenity::all::GuildId;

use crate::Result;

const GUILD_ID: GuildId = GuildId::new(111135289648349184);
const STREAMING_ROLE: RoleId = RoleId::new(1324485822745022486);

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
            .context("Failed to add streaming role")?;
    } else {
        ctx.http
            .remove_member_role(guild_id, new_data.user.id, STREAMING_ROLE, None)
            .await
            .context("Failed to remove streaming role")?;
    }

    Ok(())
}
