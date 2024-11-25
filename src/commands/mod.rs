mod ask;
mod colors;
mod dino;
mod duel;
mod dynamic_commands;
mod eightball;
mod mixu;
mod poll;
mod quote;
mod rockpaperscissors;
mod roll;
mod rpg;
mod sudoku;

use crate::{Data, Error};
use dynamic_commands::CommandInfo;
use poise::serenity_prelude::{all::CreateCommand, Context as SerenityContext};
use poise::Command;
use std::{collections::HashMap, sync::OnceLock};

pub use dynamic_commands::{try_intercepting_command_call, CommandKind, SimpleCommands};

pub static DEFAULT_COMMANDS: OnceLock<Vec<String>> = OnceLock::new();

pub async fn initialize_commands(database: &sqlx::SqlitePool) {
    dino::setup_dinos();
    mixu::set_initial_best_mixu_score(database)
        .await
        .expect("Unable to set best mixu score");
}

pub async fn setup_collectors(ctx: &SerenityContext, user_data: &Data) {
    tokio::select! {
        _ = rpg::setup_rpg_summary(ctx, user_data) => {}
        _ = dino::setup_dino_collector(ctx, user_data) => {}
    }
}

pub fn set_system_commands(commands: &[Command<Data, Error>]) {
    DEFAULT_COMMANDS.get_or_init(|| commands.iter().map(|c| c.name.clone()).collect::<Vec<_>>());
}

pub async fn register_dynamic_commands_for_every_guild(ctx: &SerenityContext, user_data: &Data) {
    let commands_map = fetch_guild_commands(user_data)
        .await
        .expect("Could not fetch simple guild commands");

    register_guild_commands(ctx, &commands_map)
        .await
        .expect("Could not register simple guild commands");

    let mut data_commands = user_data.simple_commands.write().await;
    *data_commands = commands_map;
}

pub fn get_commands() -> Vec<Command<Data, Error>> {
    vec![
        poll::poll(),
        ask::ask(),
        mixu::bestmixu(),
        colors::color(),
        dynamic_commands::commands(),
        dino::dino(),
        duel::duel(),
        duel::duelstats(),
        eightball::eightball(),
        mixu::mikustare(),
        mixu::mixu(),
        quote::quote(),
        rpg::rpg(),
        rockpaperscissors::rps(),
        sudoku::sudoku(),
        colors::uncolor(),
        roll::roll(),
    ]
}

#[derive(Debug)]
struct GuildCommand {
    guild_id: i64,
    name: String,
    kind: CommandKind,
    content: String,
}

async fn fetch_guild_commands(user_data: &Data) -> anyhow::Result<SimpleCommands> {
    let guild_commands = sqlx::query_as!(
        GuildCommand,
        r#"SELECT guild_id, name, kind as "kind: CommandKind", content FROM SimpleCommands"#
    )
    .fetch_all(&user_data.database)
    .await?;

    let mut commands_map: SimpleCommands = HashMap::new();
    for command in guild_commands {
        let entry = commands_map.entry(command.guild_id).or_default();
        let info = CommandInfo {
            kind: command.kind,
            content: command.content,
        };

        entry.insert(command.name, info);
    }

    Ok(commands_map)
}

async fn register_guild_commands(
    ctx: &SerenityContext,
    commands_map: &SimpleCommands,
) -> anyhow::Result<()> {
    for id in ctx.cache.guilds() {
        let Some(names) = commands_map.get(&(id.get() as i64)) else {
            // Reset commands if there aren't any for this guild
            id.set_commands(&ctx.http, Vec::new()).await?;
            continue;
        };

        let commands = names
            .iter()
            .map(|(name, _content)| CreateCommand::new(name).description("A simple text command"))
            .collect::<Vec<_>>();

        id.set_commands(ctx, commands).await?;
    }

    Ok(())
}
