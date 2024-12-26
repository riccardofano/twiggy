use serenity::all::{EditRole, Emoji};

use crate::{common::bail_reply, Context, Result};

#[poise::command(guild_only, slash_command, subcommands("create"))]
pub async fn icon(_ctx: Context<'_>) -> Result<()> {
    Ok(())
}

#[poise::command(guild_only, slash_command)]
async fn create(
    ctx: Context<'_>,
    #[description = "The emote to create an icon role frofm"] emoji: Emoji,
) -> Result<()> {
    let guild_id = ctx.guild_id().expect("/icon create was not run on a guild");

    if !guild_id
        .emojis(ctx)
        .await
        .expect("Failed to get emojis of this guild")
        .iter()
        .any(|i| i.name == emoji.name)
    {
        return bail_reply(ctx, format!("`{}` is not from this server!", emoji.name)).await;
    };

    let guild_roles = guild_id
        .roles(ctx)
        .await
        .expect("Failed to roles for this guild");

    let role_name = format!("{} [ICON]", emoji.name);
    if guild_roles.values().any(|r| r.name == role_name) {
        return bail_reply(ctx, format!("`{role_name}` already exists.")).await;
    }

    let success_message = format!(
        "`{role_name}` was successfully created! Use /icon {} to join it.",
        emoji.name
    );

    if let Err(e) = guild_id
        .create_role(
            ctx,
            EditRole::default()
                .name(&role_name)
                .unicode_emoji(Some(emoji.name))
                .mentionable(false),
        )
        .await
    {
        return bail_reply(ctx, format!("Failed to create `{role_name}` role. {e:?}")).await;
    }

    ctx.say(success_message).await?;

    Ok(())
}
