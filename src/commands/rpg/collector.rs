use poise::futures_util::StreamExt;
use poise::serenity_prelude as serenity;
use poise::serenity_prelude::ComponentInteractionCollectorBuilder;
use sqlx::SqlitePool;

use crate::Data;
use crate::Result;

pub async fn setup_rpg_summary(ctx: &serenity::Context, user_data: &Data) -> Result<()> {
    let collector = ComponentInteractionCollectorBuilder::new(ctx)
        .filter(|f| f.data.custom_id == "rpg-summary")
        .build();

    let _: Vec<_> = collector
        .then(|interaction| async move {
            let data = user_data.rpg_summary_cache.lock().await;
            let hashmap_log = data.get(&interaction.message.id.0);

            let response = match hashmap_log {
                Some(log) => log.clone(),
                None => match retrieve_fight_record(
                    &user_data.database,
                    interaction.message.id.to_string(),
                )
                .await
                .ok()
                .flatten()
                {
                    Some(log) => log,
                    None => "This fight was lost to history or maybe it never happened".to_string(),
                },
            };

            let _result = interaction
                .create_interaction_response(&ctx, |r| {
                    r.interaction_response_data(|d| d.content(response).ephemeral(true))
                })
                .await;
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
