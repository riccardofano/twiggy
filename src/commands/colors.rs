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

    let Some(guild) = ctx.guild() else {
        ephemeral_message(ctx, "Could not find the guild you're in.").await?;
        return Ok(());
    };

    let Some(mut author_member) = ctx.author_member().await else {
        ephemeral_message(ctx, "Could not find your roles.").await?;
        return Ok(());
    };

    let role_name = format!("#{color:06X}");
    let role = match guild.role_by_name(&role_name) {
        Some(role) => role.clone(),
        None => {
            guild
                .create_role(ctx, |role| role.name(&role_name).colour(color as u64))
                .await?
        }
    };

    author_member.to_mut().add_role(ctx, role.id).await?;
    ephemeral_message(ctx, format!("The role color {role_name} has been added!")).await?;

    Ok(())
}
