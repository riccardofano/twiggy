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

            let response =
                handle_button_press(&interaction, db, dino_id.unwrap(), button_type).await;

            if let Err(e) = response {
                eprintln!("Failed to handle button ({custom_id}): {e}");
                return interaction;
            }

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
