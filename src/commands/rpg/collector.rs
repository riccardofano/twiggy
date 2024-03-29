use std::borrow::Cow;

use poise::futures_util::StreamExt;
use poise::serenity_prelude as serenity;
use poise::serenity_prelude::AttachmentType;
use poise::serenity_prelude::ComponentInteractionCollectorBuilder;
use sqlx::SqlitePool;

use crate::Data;
use crate::Result;

pub async fn setup_rpg_summary(ctx: &serenity::Context, user_data: &Data) -> Result<()> {
    let collector = ComponentInteractionCollectorBuilder::new(ctx)
        .filter(|f| f.data.custom_id == "rpg-summary")
        .build();

    println!("Setup rpg summary collector");

    let _: Vec<_> = collector
        .then(|interaction| async move {
            let mut cache = user_data.rpg_summary_cache.lock().await;
            let message_id = interaction.message.id;

            let response = match cache.get(message_id.as_u64()).cloned() {
                Some(log) => log,
                None => {
                    let retrieved = retrieve_fight_record(
                        &user_data.database,
                        interaction.message.id.to_string(),
                    )
                    .await;

                    if let Some(log) = retrieved.ok().flatten() {
                        cache.put(message_id.0, log.clone());
                        log
                    } else {
                        "This fight was lost to history or maybe it never happened".to_string()
                    }
                }
            };

            // Discord has a 2000 character limit on messages, summaries can get
            // quite long so if they do send them as files instead.
            let _ = if response.len() < 2000 {
                interaction
                    .create_interaction_response(&ctx, |r| {
                        r.interaction_response_data(|d| d.content(response).ephemeral(true))
                    })
                    .await
            } else {
                let file = AttachmentType::Bytes {
                    data: Cow::Borrowed(response.as_bytes()),
                    filename: "fight_log.md".to_string(),
                };
                interaction
                    .create_interaction_response(&ctx, |r| {
                        r.interaction_response_data(|d| d.add_file(file).ephemeral(true))
                    })
                    .await
            };

            interaction
        })
        .collect()
        .await;

    Ok(())
}

async fn retrieve_fight_record(db: &SqlitePool, message_id: String) -> Result<Option<String>> {
    let row = sqlx::query!("SELECT log FROM RPGFight WHERE message_id = ?", message_id)
        .fetch_optional(db)
        .await?;

    Ok(row.map(|r| r.log))
}
