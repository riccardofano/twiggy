use std::collections::{hash_map::Entry, HashMap};

use poise::serenity_prelude::{Context as SerenityContext, CreateCommand, GuildId};
use rand::seq::IteratorRandom;
use serenity::all::Interaction;

use super::DEFAULT_COMMANDS;
use crate::{
    common::{bail_reply, ephemeral_reply, response, text_message},
    Context, Data, Result,
};

#[derive(Debug, sqlx::Type, poise::ChoiceParameter, Clone, Copy)]
pub enum CommandKind {
    Static,
    Choice,
}

pub struct CommandInfo {
    pub kind: CommandKind,
    pub content: String,
}

pub type SimpleCommands = HashMap<i64, HashMap<String, CommandInfo>>;

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
    #[description = "What the command should say"] content: String,
    #[description = "Whether it should just say that text or choose one of the comma separated choices"]
    kind: Option<CommandKind>,
) -> Result<()> {
    let name = name.to_lowercase();
    let kind = kind.unwrap_or(CommandKind::Static);

    ensure_single_word(ctx, &name).await?;
    ensure_not_default_command(ctx, &name).await?;

    let guild = ctx
        .guild_id()
        .expect("Expected /commands add to be guild only.");

    let new_command = CommandInfo { kind, content };
    insert_command(ctx, &guild, &name, new_command).await?;
    register_command(ctx, &guild, name).await?;

    ctx.send(ephemeral_reply("Command added")).await?;

    Ok(())
}

#[poise::command(guild_only, slash_command, prefix_command, aliases("modify"))]
pub async fn edit(
    ctx: Context<'_>,
    #[description = "The name of the command"] name: String,
    #[description = "What the command should say"] content: String,
    #[description = "Whether it should just say that text or choose one of the comma separated choices"]
    kind: CommandKind,
) -> Result<()> {
    let name = name.to_lowercase();
    let guild = ctx
        .guild_id()
        .expect("Expected /commands edit to be guild only.");

    let updated_command = CommandInfo { kind, content };
    update_command(ctx, &guild, &name, updated_command).await?;
    ctx.send(ephemeral_reply("The command has been updated."))
        .await?;

    Ok(())
}

#[poise::command(guild_only, slash_command, prefix_command, aliases("delete"))]
pub async fn remove(
    ctx: Context<'_>,
    #[description = "The name of the command"] name: String,
) -> Result<()> {
    let name = name.to_lowercase();
    let guild = ctx
        .guild_id()
        .expect("Expected /commands edit to be guild only.");

    delete_command(ctx, &guild, &name).await?;
    unregister_command(ctx, &guild, &name).await?;

    ctx.send(ephemeral_reply("The command has been removed."))
        .await?;

    Ok(())
}

async fn insert_command(
    ctx: Context<'_>,
    guild_id: &GuildId,
    name: &str,
    new_command: CommandInfo,
) -> Result<()> {
    let data = ctx.data();
    let mut map = data.simple_commands.write().await;

    let guild_id = guild_id.get() as i64;
    let guild_commands = map.entry(guild_id).or_default();
    let Entry::Vacant(entry) = guild_commands.entry(name.to_owned()) else {
        // TODO: Should this be outside?
        return bail_reply(ctx, "The command already exists.").await;
    };

    let content = &new_command.content;
    let kind = new_command.kind;
    sqlx::query!(
        "INSERT INTO SimpleCommands (guild_id, name, kind, content) VALUES (?, ?, ?, ?)",
        guild_id,
        name,
        kind,
        content
    )
    .execute(&data.database)
    .await?;

    entry.insert(new_command);

    Ok(())
}
async fn update_command(
    ctx: Context<'_>,
    guild_id: &GuildId,
    name: &str,
    command: CommandInfo,
) -> Result<()> {
    let data = ctx.data();
    let mut map = data.simple_commands.write().await;

    let guild_id = guild_id.get() as i64;
    let Some(guild_commands) = map.get_mut(&guild_id) else {
        return bail_reply(ctx, "This guild does not have this command.").await;
    };
    let Some(entry) = guild_commands.get_mut(name) else {
        return bail_reply(ctx, "The command does not exist.").await;
    };

    let content = &command.content;
    let kind = &command.kind;
    sqlx::query!(
        "UPDATE OR IGNORE SimpleCommands SET kind = ?, content = ? WHERE guild_id = ? AND name = ?",
        guild_id,
        kind,
        content,
        name
    )
    .execute(&data.database)
    .await?;

    *entry = command;

    Ok(())
}
async fn delete_command(ctx: Context<'_>, guild_id: &GuildId, name: &str) -> Result<()> {
    let data = ctx.data();
    let mut map = data.simple_commands.write().await;

    let guild_id = guild_id.get() as i64;
    let Some(guild_commands) = map.get_mut(&guild_id) else {
        return bail_reply(ctx, "This guild does not have this command.").await;
    };
    let Entry::Occupied(entry) = guild_commands.entry(name.to_owned()) else {
        return bail_reply(ctx, "This command name does not exist.").await;
    };

    sqlx::query!(
        "DELETE FROM SimpleCommands WHERE guild_id = ? AND name = ?",
        guild_id,
        name
    )
    .execute(&data.database)
    .await?;

    entry.remove_entry();

    Ok(())
}

async fn register_command(ctx: Context<'_>, guild_id: &GuildId, name: String) -> Result<()> {
    guild_id
        .create_command(
            ctx,
            CreateCommand::new(name).description("A simple text command"),
        )
        .await?;

    Ok(())
}
async fn unregister_command(ctx: Context<'_>, guild_id: &GuildId, name: &str) -> Result<()> {
    let Some(command_to_delete) = guild_id
        .get_commands(ctx)
        .await?
        .into_iter()
        .find(|c| c.name == name)
    else {
        eprintln!("Command {name} was present in the hashmap but in the guild commands");
        return Ok(());
    };

    guild_id.delete_command(ctx, command_to_delete.id).await?;

    Ok(())
}

async fn ensure_not_default_command(ctx: Context<'_>, name: &str) -> Result<()> {
    if DEFAULT_COMMANDS
        .get()
        .expect("Expected default commands to be initialized.")
        .iter()
        .any(|n| n == name)
    {
        let msg =
            "Cannot add command with that name because it's already taken by a default command.";
        ctx.send(ephemeral_reply(msg)).await?;
    }

    Ok(())
}
async fn ensure_single_word(ctx: Context<'_>, name: &str) -> Result<()> {
    if !name.chars().all(|c| c.is_ascii_alphanumeric()) {
        ctx.send(ephemeral_reply("Command name must be a single word."))
            .await?;
    };

    Ok(())
}

pub async fn try_intercepting_command_call(
    ctx: &SerenityContext,
    user_data: &Data,
    interaction: &Interaction,
) -> Result<()> {
    let interaction = interaction.clone();

    let Some(command) = interaction.command() else {
        return Ok(());
    };

    let map = user_data.simple_commands.read().await;
    let Some(guild_id) = command.guild_id else {
        return Ok(());
    };
    let Some(guild_commands) = map.get(&(guild_id.get() as i64)) else {
        return Ok(());
    };

    if let Some(dynamic_command) = guild_commands.get(&command.data.name) {
        let text = respond_to_command_call(dynamic_command);
        command
            .create_response(ctx, response(text_message(text)))
            .await?;
    };

    Ok(())
}

fn respond_to_command_call(dynamic_command: &CommandInfo) -> String {
    match dynamic_command.kind {
        CommandKind::Static => dynamic_command.content.clone(),
        CommandKind::Choice => {
            let mut rng = rand::thread_rng();
            dynamic_command
                .content
                .split('|')
                .choose(&mut rng)
                .unwrap()
                .to_string()
        }
    }
}
