mod commands;
mod common;

use anyhow::Result;
use commands::*;
use poise::serenity_prelude as serenity;
use poise::serenity_prelude::Mutex;
use std::collections::HashMap;

pub struct Data {
    database: sqlx::SqlitePool,
    rpg_summary_cache: Mutex<HashMap<u64, String>>,
}
pub type Context<'a> = poise::Context<'a, Data, anyhow::Error>;
pub type Error = anyhow::Error;

#[tokio::main]
async fn main() {
    let database = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename("database.sqlite")
                .create_if_missing(true),
        )
        .await
        .expect("Expected to be able to connect to the database");

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![rpg(), eightball(), duel(), duelstats(), dino()],
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
        })
        .token(std::env::var("DISCORD_TOKEN").expect("missing DISCORD_TOKEN"))
        .intents(serenity::GatewayIntents::non_privileged())
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(Data {
                    database,
                    rpg_summary_cache: Mutex::new(HashMap::new()),
                })
            })
        });

    framework.run().await.unwrap();
}

async fn on_error(error: poise::FrameworkError<'_, Data, Error>) {
    match error {
        poise::FrameworkError::Command { error, ctx: _ctx } => {
            eprintln!("Command error: {}", error);
        }
        _ => {
            if let Err(e) = poise::builtins::on_error(error).await {
                eprintln!("Error while trying to handle poise error: {e}")
            }
        }
    }
}

async fn event_event_handler(
    ctx: &serenity::Context,
    event: &poise::Event<'_>,
    _framework: poise::FrameworkContext<'_, Data, Error>,
    user_data: &Data,
) -> Result<(), Error> {
    if let poise::Event::Ready { data_about_bot } = event {
        println!("{} is connected!", data_about_bot.user.name);

        setup_rpg_summary(ctx, user_data).await?;
    }

    Ok(())
}
