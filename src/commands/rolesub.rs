use serenity::all::{EditRole, GuildId, RoleId};

use crate::{common::bail_reply, Context, Result};

const ROLE_SUFFIX: &str = "[BOT]";

#[poise::command(
    guild_only,
    slash_command,
    subcommands("create", "delete"),
    required_permissions = "ADMINISTRATOR",
    default_member_permissions = "ADMINISTRATOR"
)]
pub async fn role(_ctx: Context<'_>) -> Result<()> {
    Ok(())
}

/// MOD ONLY: Create bot role for this server
#[poise::command(guild_only, slash_command)]
async fn create(
    ctx: Context<'_>,
    #[description = "Name of the role to create"] role: String,
) -> Result<()> {
    let guild_id = ctx.guild_id().expect("/role create was not run in a guild");

    let role_name = to_role_name(&role);
    if get_server_role(ctx, guild_id, &role_name).await.is_some() {
        return bail_reply(ctx, format!("`{role_name}` already exists.")).await;
    }

    if let Err(e) = guild_id
        .create_role(ctx, EditRole::default().name(&role_name).mentionable(true))
        .await
    {
        eprintln!("Failed to create {role_name}: {e:?}");
        return bail_reply(ctx, format!("Failed to create `{role_name}` role.")).await;
    }

    let success_message =
        format!("`{role_name}` was successfully created! Use `/rolesub {role}` to join it.",);
    ctx.say(success_message).await?;

    Ok(())
}

/// MOD ONLY: Delete bot role from the server
#[poise::command(guild_only, slash_command)]
async fn delete(
    ctx: Context<'_>,
    #[description = "The name of the role to be deleted"] role: String,
) -> Result<()> {
    let guild_id = ctx.guild_id().expect("/role delete was not run in a guild");

    let role_name = to_role_name(&role);
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

/// Add/Remove a bot role from your roles
#[poise::command(guild_only, slash_command)]
pub async fn rolesub(
    ctx: Context<'_>,
    #[description = "Select the emoji for the role you want to join"] role: String,
) -> Result<()> {
    let guild_id = ctx.guild_id().expect("/rolesub was not run on a guild");
    let role_name = to_role_name(&role);

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
            if let Err(e) = author.add_role(ctx, role_id).await {
                eprintln!("Failed to add the {role_name} role, {e:?}");
                return bail_reply(ctx, "Failed to add the role :(").await;
            };
            bail_reply(ctx, format!("The `{role_name}` role has been added!")).await
        }
    }
}

fn to_role_name(role_name: &str) -> String {
    format!("{role_name} {ROLE_SUFFIX}")
}

async fn get_server_role(ctx: Context<'_>, guild_id: GuildId, role_name: &str) -> Option<RoleId> {
    let guild_roles = guild_id.roles(ctx).await.ok()?;

    guild_roles
        .values()
        .find(|r| r.name == role_name)
        .map(|r| r.id)
}
