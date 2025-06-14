mod commands;
mod common;
pub mod config;
mod events;

use std::num::NonZeroUsize;

use anyhow::Result;
use common::bail_reply;
use lru::LruCache;
use poise::serenity_prelude::{self as serenity, FullEvent, GatewayIntents};
use tokio::sync::{Mutex, RwLock};

pub struct Data {
    database: sqlx::SqlitePool,
    rpg_summary_cache: Mutex<LruCache<u64, String>>,
    simple_commands: RwLock<commands::SimpleCommands>,
}
pub type Context<'a> = poise::Context<'a, Data, anyhow::Error>;
pub type Error = anyhow::Error;

#[tokio::main]
async fn main() {
    let token = std::env::var("DISCORD_TOKEN").expect("missing DISCORD_TOKEN");
    let intents = GatewayIntents::non_privileged()
        | GatewayIntents::MESSAGE_CONTENT
        | GatewayIntents::GUILD_PRESENCES;

    let database = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename("database.sqlite")
                .create_if_missing(true),
        )
        .await
        .expect("Expected to be able to connect to the database");

    // Initialize default commands
    let commands = commands::initialize_commands(&database).await;
    commands::set_system_commands(&commands);
    // Initialize event data
    events::initialize_event_data().await;

    let options = poise::FrameworkOptions {
        commands,
        prefix_options: poise::PrefixFrameworkOptions {
            prefix: Some(String::from(">")),
            mention_as_prefix: false,
            ..Default::default()
        },
        event_handler: |ctx, event, framework, user_data| {
            Box::pin(event_event_handler(ctx, event, framework, user_data))
        },
        on_error: |err| Box::pin(on_error(err)),
        ..Default::default()
    };

    let user_data = Data {
        database,
        rpg_summary_cache: Mutex::new(LruCache::new(NonZeroUsize::new(10).unwrap())),
        simple_commands: RwLock::default(),
    };
    let framework = poise::Framework::builder()
        .options(options)
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                // let no_commands: Vec<poise::Command<Data, Error>> = Vec::new();
                // poise::builtins::register_globally(ctx, &no_commands).await?;
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(user_data)
            })
        })
        .build();

    let client = serenity::ClientBuilder::new(token, intents)
        .framework(framework)
        .await;
    client.unwrap().start().await.unwrap();
}

async fn on_error(error: poise::FrameworkError<'_, Data, Error>) {
    match error {
        poise::FrameworkError::Command { error, .. } => {
            eprintln!("Command error: {}", error);
        }
        poise::FrameworkError::ArgumentParse {
            error, input, ctx, ..
        } => {
            // Overrides the framework's default with same error message except usage info
            // But it sends it as an ephemeral message instead of one visible to everyone.
            let response = if let Some(input) = input {
                format!("**Cannot parse `{}` as argument: {}**", input, error)
            } else {
                format!("**{}**", error)
            };
            bail_reply(ctx, response).await.unwrap();
        }
        _ => {
            if let Err(e) = poise::builtins::on_error(error).await {
                eprintln!("Error while trying to handle poise error: {e}")
            }
        }
    }
}

async fn event_event_handler<'a>(
    ctx: &'a serenity::Context,
    event: &'a serenity::FullEvent,
    _framework: poise::FrameworkContext<'a, Data, Error>,
    user_data: &Data,
) -> Result<(), Error> {
    match event {
        FullEvent::Ready { data_about_bot } => {
            println!("{} is connected!", data_about_bot.user.name);
            commands::register_dynamic_commands_for_every_guild(ctx, user_data).await;
            commands::setup_collectors(ctx, user_data).await;
        }
        FullEvent::Message { new_message } => events::handle_new_message_event(ctx, new_message),
        FullEvent::PresenceUpdate { new_data } => events::handle_presence_update(ctx, new_data),
        FullEvent::InteractionCreate { interaction } => {
            commands::try_intercepting_command_call(ctx, user_data, interaction).await?;
        }
        _ => {}
    }

    Ok(())
}
