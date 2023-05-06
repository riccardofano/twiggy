use std::borrow::Cow;

use poise::serenity_prelude::{Member, Mention, Role};

use crate::{common::ephemeral_message, Context, Result};

#[poise::command(guild_only, slash_command, prefix_command, subcommands("change"))]
pub async fn color(_ctx: Context<'_>) -> Result<()> {
    Ok(())
}

#[poise::command(guild_only, slash_command, prefix_command)]
async fn change(ctx: Context<'_>, hexcode: String) -> Result<()> {
    let hexcode = hexcode.strip_prefix('#').unwrap_or(&hexcode);

    let color = match u32::from_str_radix(hexcode, 16) {
        Ok(code) if hexcode.len() == 6 && code > 0 => code,
        _ => {
            ephemeral_message(ctx, "Please provide a valid color.").await?;
            return Ok(());
        }
    };

    let Some(author_member) = ctx.author_member().await else {
        ephemeral_message(ctx, "I could not find your roles.").await?;
        return Ok(());
    };

    let role_name = match change_color(ctx, author_member, Some(color)).await {
        Ok(name) => name,
        Err(e) => {
            eprintln!("Error while trying to change color: {e}");
            ephemeral_message(
                ctx,
                "Something went wrong while trying to change your color. :(",
            )
            .await?;
            return Ok(());
        }
    };

    ephemeral_message(ctx, format!("The role color {role_name} has been added!")).await?;

    Ok(())
}

async fn change_color(
    ctx: Context<'_>,
    mut member: Cow<'_, Member>,
    color: Option<u32>,
) -> Result<String> {
    let Some(guild) = ctx.guild() else {
        return Err(anyhow::anyhow!("Could not find the guild guild where to assign a new color role."));
    };

    // TODO: generate color if it's None
    let color = color.unwrap();

    let role_name = format!("#{color:06X}");
    let role = match guild.role_by_name(&role_name) {
        Some(role) => role.clone(),
        None => {
            guild
                .create_role(ctx, |role| role.name(&role_name).colour(color as u64))
                .await?
        }
    };
    member.to_mut().add_role(ctx, role.id).await?;

    Ok(role_name)
}

#[poise::command(
    guild_only,
    slash_command,
    prefix_command,
    required_permissions = "MODERATE_MEMBERS"
)]
pub async fn uncolor(ctx: Context<'_>, mut member: Member) -> Result<()> {
    let Some(roles) = member.roles(ctx) else {
        ephemeral_message(ctx, "This person has no roles.").await?;
        return Ok(())
    };
    let mut removed_role = false;

    for role in roles.iter() {
        if !role.name.starts_with('#') {
            continue;
        }
        member.remove_role(ctx, role.id).await?;
        removed_role = true;

        if is_role_unused(ctx, role).await? {
            role.guild_id.delete_role(ctx, role.id).await?;
        }
    }

    if !removed_role {
        ephemeral_message(ctx, "There were no roles to remove.").await?;
        return Ok(());
    }

    ephemeral_message(
        ctx,
        format!(
            "All color roles have been removed from {}.",
            Mention::from(member.user.id)
        ),
    )
    .await?;

    Ok(())
}

async fn is_role_unused(ctx: Context<'_>, role: &Role) -> Result<bool> {
    let members = role.guild_id.members(ctx, None, None).await?;
    for member in members {
        let Some(roles) = member.roles(ctx) else {
            continue
        };
        if roles.contains(role) {
            return Ok(false);
        }
    }

    Ok(true)
}
