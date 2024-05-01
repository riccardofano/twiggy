use std::collections::hash_map::Entry;

use crate::{common::ephemeral_message, Context, Result};

pub type SimpleCommands = std::collections::HashMap<String, String>;

#[poise::command(
    guild_only,
    slash_command,
    prefix_command,
    subcommands("add", "edit", "remove"),
    required_permissions = "MODERATE_MEMBERS"
)]
pub async fn commands(_ctx: Context<'_>) -> Result<()> {
    Ok(())
}

#[poise::command(guild_only, slash_command, prefix_command, aliases("create"))]
pub async fn add(
    ctx: Context<'_>,
    #[description = "The name of the command"] name: String,
    #[description = "What the command should say"] text: String,
) -> Result<()> {
    let mut map = ctx.data().simple_commands.write().await;
    let Entry::Vacant(entry) = map.entry(name.clone()) else {
        ephemeral_message(ctx, "The command already exists.").await?;
        return Ok(());
    };

    entry.insert(text);
    drop(map);

    let guild = ctx
        .guild()
        .expect("Expected /commands add to be guild only.");
    guild
        .create_application_command(ctx, |c| c.name(name).description("A simple text command"))
        .await?;

    ephemeral_message(ctx, "Command added").await?;

    Ok(())
}

#[poise::command(guild_only, slash_command, prefix_command, aliases("modify"))]
pub async fn edit(
    ctx: Context<'_>,
    #[description = "The name of the command"] name: String,
    #[description = "What the command should say"] text: String,
) -> Result<()> {
    let mut map = ctx.data().simple_commands.write().await;

    let Some(entry) = map.get_mut(&name) else {
        ephemeral_message(ctx, "The command does not exist.").await?;
        return Ok(());
    };
    *entry = text;

    ephemeral_message(ctx, "The command has been updated.").await?;

    Ok(())
}

#[poise::command(guild_only, slash_command, prefix_command, aliases("delete"))]
pub async fn remove(
    ctx: Context<'_>,
    #[description = "The name of the command"] name: String,
) -> Result<()> {
    let mut map = ctx.data().simple_commands.write().await;
    let Some(_) = map.remove(&name) else {
        ephemeral_message(ctx, "The command does not exist.").await?;
        return Ok(());
    };
    drop(map);

    let guild = ctx
        .guild()
        .expect("Expected /commands remove should be guild only.");

    let Some(command_to_delete) = guild
        .get_application_commands(ctx)
        .await?
        .into_iter()
        .find(|c| c.name == name)
    else {
        eprintln!("Command {name} was present in the hashmap but in the guild commands");
        return Ok(());
    };

    guild
        .delete_application_command(ctx, command_to_delete.id)
        .await?;

    ephemeral_message(ctx, "The command has been removed.").await?;

    Ok(())
}
