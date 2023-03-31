mod commands;

use anyhow::Result;
use commands::*;
use poise::serenity_prelude as serenity;

pub struct Data {
    database: sqlx::SqlitePool,
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
            commands: vec![eightball(), duel(), duelstats()],
            prefix_options: poise::PrefixFrameworkOptions {
                prefix: Some(String::from(">")),
                mention_as_prefix: false,
                ..Default::default()
            },
            event_handler: |ctx, event, framework, user_data| {
                Box::pin(event_event_handler(ctx, event, framework, user_data))
            },
            ..Default::default()
        })
        .token(std::env::var("DISCORD_TOKEN").expect("missing DISCORD_TOKEN"))
        .intents(serenity::GatewayIntents::non_privileged())
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(Data { database })
            })
        });

    framework.run().await.unwrap();
}

async fn event_event_handler(
    ctx: &serenity::Context,
    event: &poise::Event<'_>,
    framework: poise::FrameworkContext<'_, Data, Error>,
    _user_data: &Data,
) -> Result<(), Error> {
    match event {
        poise::Event::Ready { data_about_bot } => {
            println!("{} is connected!", data_about_bot.user.name);
            let commands = &framework.options().commands;
            let create_commands = poise::builtins::create_application_commands(&commands);

            serenity::Command::set_global_application_commands(ctx, |builder| {
                *builder = create_commands;
                builder
            })
            .await?;
        }
        _ => {}
    }

    Ok(())
}
