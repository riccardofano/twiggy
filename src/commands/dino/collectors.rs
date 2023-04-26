use std::fmt::Display;

use poise::futures_util::StreamExt;
use poise::serenity_prelude::ComponentInteractionCollectorBuilder;
use poise::serenity_prelude::{self as serenity, MessageComponentInteraction};
use sqlx::SqlitePool;

use crate::commands::dino::{COVET_BUTTON, FAVOURITE_BUTTON, SHUN_BUTTON};
use crate::Data;
use crate::Result;

enum TransactionType {
    Covet,
    Shun,
    Favourite,
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

            let button_type = match &interaction.data.custom_id {
                b if b.starts_with(COVET_BUTTON) => TransactionType::Covet,
                b if b.starts_with(SHUN_BUTTON) => TransactionType::Shun,
                b if b.starts_with(FAVOURITE_BUTTON) => TransactionType::Favourite,
                _ => return interaction,
            };

            let response =
                handle_button_press(&interaction, db, dino_id.unwrap(), button_type).await;

            // TODO: edit original message with new hotness value

            if let Some(content) = response {
                if let Err(e) = interaction
                    .create_interaction_response(&ctx, |r| {
                        r.interaction_response_data(|d| d.content(content).ephemeral(true))
                    })
                    .await
                {
                    eprintln!("Failed to send interaction response: {:?}", e);
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
) -> Option<String> {
    let user_id = interaction.user.id.to_string();

    // FIXME NOTE: right now I'm not checking if there has been a old shun if I
    // recieve a new covet or viceversa, I need to remove that one first and
    // then add the covet but if there is a covet I just want to delete it to
    // toggle it

    // TODO: use transaction instead of db
    let dino_transaction = fetch_transaction(db, &user_id, dino_id, &button_type)
        .await
        .unwrap();

    // TODO: better messages
    match dino_transaction {
        Some(id) => {
            if let Err(e) = delete_transaction(db, id).await {
                eprintln!("Failed to delete/create transaction: {:?}", e);
                return Some(format!(
                    "Failed to {} dino",
                    &button_type.to_string().to_lowercase()
                ));
            };
            None
        }
        None => {
            if let Err(e) = create_transaction(db, &user_id, dino_id, &button_type).await {
                eprintln!("Failed to delete/create transaction: {:?}", e);
                return Some(format!(
                    "Failed to {} dino",
                    button_type.to_string().to_lowercase()
                ));
            };
            None
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
