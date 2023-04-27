use std::collections::HashMap;
use std::fmt::Display;

use poise::futures_util::StreamExt;
use poise::serenity_prelude::{self as serenity, MessageComponentInteraction};
use poise::serenity_prelude::{ComponentInteractionCollectorBuilder, CreateEmbed};
use sqlx::SqlitePool;

use crate::commands::dino::{COVET_BUTTON, FAVOURITE_BUTTON, SHUN_BUTTON};
use crate::common::Score;
use crate::Data;
use crate::Result;

enum TransactionType {
    Covet,
    Shun,
    Favourite,
}

impl TransactionType {
    fn opposite(&self) -> Option<Self> {
        match self {
            Self::Covet => Some(Self::Shun),
            Self::Shun => Some(Self::Covet),
            Self::Favourite => None,
        }
    }
}

impl Display for TransactionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match self {
            TransactionType::Covet => "COVET",
            TransactionType::Shun => "SHUN",
            TransactionType::Favourite => "FAVOURITE",
        };

        write!(f, "{}", kind)
    }
}

pub async fn setup_dino_collector(ctx: &serenity::Context, user_data: &Data) -> Result<()> {
    let collector = ComponentInteractionCollectorBuilder::new(ctx)
        .filter(|f| f.data.custom_id.starts_with("dino-"))
        .build();
    println!("Setup dino collector");

    let _: Vec<_> = collector
        .then(|interaction| async move {
            let db = &user_data.database;
            let custom_id = &interaction.data.custom_id;
            let dino_id = custom_id.split(':').collect::<Vec<_>>();
            let dino_id = dino_id.get(1);

            if dino_id.is_none() {
                if let Err(e) = interaction
                    .create_interaction_response(&ctx, |r| {
                        r.interaction_response_data(|d| {
                            d.content("Could not find dino id").ephemeral(true)
                        })
                    })
                    .await
                {
                    eprintln!("Could not send failed to find dino id message {:?}", e)
                };
                return interaction;
            }

            let button_type = match &custom_id {
                b if b.starts_with(COVET_BUTTON) => TransactionType::Covet,
                b if b.starts_with(SHUN_BUTTON) => TransactionType::Shun,
                b if b.starts_with(FAVOURITE_BUTTON) => TransactionType::Favourite,
                _ => return interaction,
            };

            let dino_id = dino_id.unwrap();
            let response = handle_button_press(&interaction, db, dino_id, button_type).await;

            if let Err(e) = response {
                eprintln!("Failed to handle button ({custom_id}): {e}");
                return interaction;
            }

            let response = response.unwrap();
            if let Some(content) = response {
                let interaction_response = interaction
                    .create_interaction_response(&ctx, |r| {
                        r.interaction_response_data(|d| d.content(&content).ephemeral(true))
                    })
                    .await;

                if let Err(e) = interaction_response {
                    eprintln!(
                        "Failed to send interaction response. Content: {content}, error: {e}"
                    );
                }
            } else {
                let old_embed = interaction.message.embeds[0].clone();
                let old_image = old_embed.image.clone().unwrap();
                let old_url: Vec<_> = old_image.url.split('/').collect();
                let dino_image_name = old_url.last().unwrap();

                let hotness = match calculate_dino_score(&user_data.database, dino_id).await {
                    Ok(score) => score,
                    Err(e) => {
                        eprintln!("Failed to retrieve score. Error: {e}");
                        return interaction;
                    }
                };

                // NOTE: to update the old embed the attachment must be set the
                // name of the file of the old image otherwise two images will
                // appear, one outside the embed (the old file) and one in the
                // embed with the new url discord gave it.
                let mut new_embed = CreateEmbed::from(old_embed);
                new_embed.footer(|f| f.text(format!("new score: {}", hotness)));
                new_embed.attachment(dino_image_name);

                let interaction_response = interaction
                    .create_interaction_response(&ctx, |response| {
                        response
                            .kind(serenity::InteractionResponseType::UpdateMessage)
                            .interaction_response_data(|d| d.set_embed(new_embed))
                    })
                    .await;

                if let Err(e) = interaction_response {
                    eprintln!("Failed to update old message. Error: {e}");
                }
            }

            interaction
        })
        .collect()
        .await;

    Ok(())
}

async fn handle_button_press(
    interaction: &MessageComponentInteraction,
    db: &SqlitePool,
    dino_id: &str,
    button_type: TransactionType,
) -> Result<Option<String>> {
    let user_id = interaction.user.id.to_string();

    // TODO: use transaction instead of db
    let same_type_transaction = fetch_transaction(db, &user_id, dino_id, &button_type).await?;

    match same_type_transaction {
        Some(id) => {
            delete_transaction(db, id).await?;
            if matches!(button_type, TransactionType::Favourite) {
                return Ok(Some(
                    "Dino has been removed from your favourites".to_string(),
                ));
            }
            Ok(None)
        }
        None => {
            if let Some(opposite_type) = button_type.opposite() {
                let opposite_transaction =
                    fetch_transaction(db, &user_id, dino_id, &opposite_type).await?;
                if let Some(id) = opposite_transaction {
                    delete_transaction(db, id).await?;
                }
            };

            create_transaction(db, &user_id, dino_id, &button_type).await?;
            if matches!(button_type, TransactionType::Favourite) {
                return Ok(Some("Dino has been added to your favourites".to_string()));
            }
            Ok(None)
        }
    }
}

async fn fetch_transaction(
    db: &SqlitePool,
    user_id: &str,
    dino_id: &str,
    transaction_type: &TransactionType,
) -> Result<Option<i64>> {
    let transaction_type = transaction_type.to_string();
    let row = sqlx::query!(
        "SELECT id FROM DinoTransactions WHERE type = ? AND dino_id = ? AND user_id = ?",
        transaction_type,
        dino_id,
        user_id
    )
    .fetch_optional(db)
    .await?;

    Ok(row.map(|r| r.id))
}

async fn delete_transaction(db: &SqlitePool, transaction_id: i64) -> Result<()> {
    sqlx::query!("DELETE FROM DinoTransactions WHERE id = ?", transaction_id)
        .execute(db)
        .await?;

    Ok(())
}

async fn create_transaction(
    db: &SqlitePool,
    user_id: &str,
    dino_id: &str,
    transaction_type: &TransactionType,
) -> Result<()> {
    let transaction_type = transaction_type.to_string();
    sqlx::query!(
        "INSERT INTO DinoTransactions (user_id, dino_id, type) VALUES (?, ?, ?)",
        user_id,
        dino_id,
        transaction_type
    )
    .execute(db)
    .await?;

    Ok(())
}

async fn calculate_dino_score(db: &SqlitePool, dino_id: &str) -> Result<i64> {
    let row = sqlx::query!(
        r#"SELECT COUNT(id) as count, type as type_ FROM DinoTransactions WHERE dino_id = ? GROUP BY type"#,
        dino_id
    )
    .fetch_all(db)
    .await?;

    let mut map = HashMap::new();
    for entry in row.into_iter() {
        map.insert(entry.type_, entry.count);
    }

    let score = map.get("COVET").unwrap_or(&0) - map.get("SHUN").unwrap_or(&0);

    Ok(score)
}
