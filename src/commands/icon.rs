use serenity::all::{EditRole, Emoji, GuildId, RoleId};

use crate::{common::bail_reply, Context, Result};

#[poise::command(
    guild_only,
    slash_command,
    subcommands("create", "delete"),
    required_permissions = "ADMINISTRATOR",
    default_member_permissions = "ADMINISTRATOR"
)]
pub async fn icon(_ctx: Context<'_>) -> Result<()> {
    Ok(())
}

// TODO: It's convenient to be able to tell the user to just pick the right emote in that way I have its name and id
// But the user might not be able to use the emoji a role is tied to:
// - It could have been removed
// - It could be disabled because the boosts wore off
// - It could be a gif so it's nitro only

/// MOD ONLY: Create icon role for this server
#[poise::command(guild_only, slash_command)]
async fn create(
    ctx: Context<'_>,
    #[description = "Select an emoji from this server"] emoji: Emoji,
) -> Result<()> {
    let guild_id = ctx.guild_id().expect("/icon create was not run on a guild");

    if !is_guild_emoji(ctx, guild_id, &emoji.name).await {
        return bail_reply(ctx, format!("`{}` is not from this server!", emoji.name)).await;
    };

    let role_name = to_role_name(&emoji.name);
    if get_server_role(ctx, guild_id, &role_name).await.is_some() {
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
        eprintln!("Failed to create {role_name}: {e:?}");
        return bail_reply(ctx, format!("Failed to create `{role_name}` role.")).await;
    }

    ctx.say(success_message).await?;

    Ok(())
}

/// MOD ONLY: Delete icon role from the server
#[poise::command(guild_only, slash_command)]
async fn delete(
    ctx: Context<'_>,
    #[description = "Select an emoji from this server"] emoji: Emoji,
) -> Result<()> {
    let guild_id = ctx.guild_id().expect("/icon create was not run on a guild");

    let role_name = to_role_name(&emoji.name);
    let Some(role_id) = get_server_role(ctx, guild_id, &role_name).await else {
        return bail_reply(ctx, format!("`{role_name}` doesn't exist.")).await;
    };

    if let Err(e) = guild_id.delete_role(ctx, role_id).await {
        eprintln!("Failed to delete {role_name}: {e:?}");
        return bail_reply(ctx, format!("Failed to delete `{role_name}` role.")).await;
    }

    ctx.say(format!("`{role_name}` was successfully removed!"))
        .await?;

    Ok(())
}

/// Add/Remove an icon role from your roles
#[poise::command(guild_only, slash_command)]
pub async fn iconsub(
    ctx: Context<'_>,
    #[description = "Select the emoji for the role you want to join"] emoji: Emoji,
) -> Result<()> {
    let guild_id = ctx.guild_id().expect("/icon join was not run on a guild");
    let role_name = to_role_name(&emoji.name);

    let Some(role_id) = get_server_role(ctx, guild_id, &role_name).await else {
        let msg = format!("The role {role_name} does not exist on this server.");
        return bail_reply(ctx, msg).await;
    };

    let Some(author) = ctx.author_member().await else {
        return bail_reply(ctx, "Failed to get your member information.").await;
    };

    match author.roles.iter().find(|&&r| r == role_id) {
        Some(_) => {
            if let Err(e) = author.remove_role(ctx, role_id).await {
                eprintln!("Failed to remove the {role_name} role, {e:?}");
                return bail_reply(ctx, "Failed to remove the role :(").await;
            };
            bail_reply(ctx, format!("The `{role_name}` role has been removed!")).await
        }
        None => {
            if let Some(other_icon_roles) = get_member_icon_roles(ctx, &author).await {
                author.remove_roles(ctx, &other_icon_roles).await?;
            }

            if let Err(e) = author.add_role(ctx, role_id).await {
                eprintln!("Failed to add the {role_name} role, {e:?}");
                return bail_reply(ctx, "Failed to add the role :(").await;
            };
            bail_reply(ctx, format!("The `{role_name}` role has been added!")).await
        }
    }
}

fn to_role_name(icon_name: &str) -> String {
    format!("{icon_name} [ICON]")
}

async fn is_guild_emoji(ctx: Context<'_>, guild_id: GuildId, emoji_name: &str) -> bool {
    guild_id
        .emojis(ctx)
        .await
        .expect("Failed to get emojis of this guild")
        .iter()
        .any(|e| e.name == emoji_name)
}

async fn get_server_role(ctx: Context<'_>, guild_id: GuildId, role_name: &str) -> Option<RoleId> {
    let guild_roles = guild_id
        .roles(ctx)
        .await
        .expect("Failed to roles for this guild");

    guild_roles
        .values()
        .find(|r| r.name == role_name)
        .map(|r| r.id)
}

async fn get_member_icon_roles(ctx: Context<'_>, member: &Member) -> Option<Vec<RoleId>> {
    let roles = member
        .roles(ctx)?
        .into_iter()
        .filter(|r| r.name.ends_with("[ICON]"))
        .map(|r| r.id)
        .collect::<Vec<_>>();

    Some(roles)
}
