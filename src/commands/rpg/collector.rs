use poise::futures_util::StreamExt;
use poise::serenity_prelude as serenity;
use poise::serenity_prelude::ComponentInteractionCollector;
use poise::serenity_prelude::CreateAttachment;
use poise::serenity_prelude::CreateInteractionResponseMessage;
use sqlx::SqlitePool;

use crate::common::ephemeral_text_message;
use crate::common::response;
use crate::Data;
use crate::Result;

pub async fn setup_rpg_summary(ctx: &serenity::Context, user_data: &Data) -> Result<()> {
    let mut collector = ComponentInteractionCollector::new(ctx)
        .filter(|f| f.data.custom_id == "rpg-summary")
        .stream();

    println!("Setup rpg summary collector");

    while let Some(interaction) = collector.next().await {
        let mut cache = user_data.rpg_summary_cache.lock().await;
        let message_id = interaction.message.id;

        let btn_response = match cache.get(&message_id.get()).cloned() {
            Some(log) => log,
            None => {
                let retrieved =
                    retrieve_fight_record(&user_data.database, interaction.message.id.to_string())
                        .await;

                if let Some(log) = retrieved.ok().flatten() {
                    cache.put(message_id.get(), log.clone());
                    log
                } else {
                    "This fight was lost to history or maybe it never happened".to_string()
                }
            }
        };

        // Discord has a 2000 character limit on messages, summaries can get
        // quite long so if they do send them as files instead.
        let response_message = if btn_response.len() < 2000 {
            ephemeral_text_message(btn_response)
        } else {
            let file = CreateAttachment::bytes(btn_response.as_bytes(), "fight_log.md".to_string());
            CreateInteractionResponseMessage::default()
                .add_file(file)
                .ephemeral(true)
        };

        if let Err(e) = interaction
            .create_response(ctx, response(response_message))
            .await
        {
            eprintln!("[RPG COLLECTOR ERROR] {e:?}")
        }
    }

    Ok(())
}

async fn retrieve_fight_record(db: &SqlitePool, message_id: String) -> Result<Option<String>> {
    let row = sqlx::query!("SELECT log FROM RPGFight WHERE message_id = ?", message_id)
        .fetch_optional(db)
        .await?;

    Ok(row.map(|r| r.log))
}
