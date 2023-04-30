use std::collections::HashMap;
use std::fmt::Display;

use poise::futures_util::StreamExt;
use poise::serenity_prelude::{self as serenity, MessageComponentInteraction};
use poise::serenity_prelude::{ComponentInteractionCollectorBuilder, CreateEmbed};
use sqlx::SqliteConnection;

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
            if let Err(e) = handle_dino_collector(ctx, user_data, &interaction).await {
                eprintln!("Error while handling dino collection: {e}");
            }

            interaction
        })
        .collect()
        .await;

    Ok(())
}

async fn handle_dino_collector(
    ctx: &serenity::Context,
    user_data: &Data,
    interaction: &MessageComponentInteraction,
) -> Result<()> {
    let custom_id = &interaction.data.custom_id;
    let split_btn_id = custom_id.split(':').collect::<Vec<_>>();
    let dino_id = split_btn_id.get(1);

    let Some(dino_id) = dino_id else {
        interaction
            .create_interaction_response(&ctx, |r| {
                r.interaction_response_data(|d| d.content("Could not find a dino id.").ephemeral(true))
            })
            .await?;
        return Ok(());
    };

    let Ok(dino_id) = dino_id.parse::<i64>() else {
        interaction
            .create_interaction_response(&ctx, |r| {
                r.interaction_response_data(|d| d.content("Dino id is not valid.").ephemeral(true))
            })
            .await?;
        return Ok(());
    };

    let mut transaction = user_data.database.begin().await?;
    let dino = fetch_dino_names(&mut transaction, dino_id).await?;

    if dino.is_none() {
        let old_embed = interaction.message.embeds[0].clone();
        let dino_name = old_embed.title.clone().unwrap();
        let old_image = old_embed.image.clone().unwrap();
        let split_url = old_image.url.split('/').collect::<Vec<_>>();
        let dino_image_name = split_url.last().unwrap();

        let mut new_embed = CreateEmbed::from(old_embed);
        new_embed.attachment(dino_image_name);
        new_embed.title(format!("{dino_name} is no longer with us 😔"));

        interaction
            .create_interaction_response(&ctx, |response| {
                response
                    .kind(serenity::InteractionResponseType::UpdateMessage)
                    .interaction_response_data(|d| d.set_embed(new_embed).components(|c| c))
            })
            .await?;
        return Ok(());
    }

    let (dino_name, dino_image_name) = dino.unwrap();

    let button_type = match &custom_id {
        b if b.starts_with(COVET_BUTTON) => TransactionType::Covet,
        b if b.starts_with(SHUN_BUTTON) => TransactionType::Shun,
        b if b.starts_with(FAVOURITE_BUTTON) => TransactionType::Favourite,
        _ => return Err(anyhow::anyhow!("unknown dino button pressed")),
    };

    let response = handle_button_press(interaction, &mut transaction, dino_id, button_type).await?;

    if let Some(content) = response {
        interaction
            .create_interaction_response(&ctx, |r| {
                r.interaction_response_data(|d| d.content(&content).ephemeral(true))
            })
            .await?;
    } else {
        let (worth, hotness) = calculate_dino_score(&mut transaction, dino_id).await?;

        // NOTE: to update the old embed the attachment must be set the
        // name of the file of the old image otherwise two images will
        // appear, one outside the embed (the old file) and one in the
        // embed with the new url discord gave it.
        let old_embed = interaction.message.embeds[0].clone();
        let mut new_embed = CreateEmbed::from(old_embed);
        new_embed.title(&dino_name);
        new_embed.footer(|f| {
            f.text(format!(
                "{dino_name} is worth {worth} Dino Bucks!\nHotness Rating: {hotness}"
            ))
        });
        new_embed.attachment(dino_image_name);

        interaction
            .create_interaction_response(&ctx, |response| {
                response
                    .kind(serenity::InteractionResponseType::UpdateMessage)
                    .interaction_response_data(|d| d.set_embed(new_embed))
            })
            .await?;
    }

    transaction.commit().await?;

    Ok(())
}

async fn handle_button_press(
    interaction: &MessageComponentInteraction,
    conn: &mut SqliteConnection,
    dino_id: i64,
    button_type: TransactionType,
) -> Result<Option<String>> {
    let user_id = interaction.user.id.to_string();

    let same_type_transaction = fetch_transaction(conn, &user_id, dino_id, &button_type).await?;

    match same_type_transaction {
        Some(id) => {
            delete_transaction(conn, id).await?;
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
                    fetch_transaction(conn, &user_id, dino_id, &opposite_type).await?;
                if let Some(id) = opposite_transaction {
                    delete_transaction(conn, id).await?;
                }
            };

            create_transaction(conn, &user_id, dino_id, &button_type).await?;
            if matches!(button_type, TransactionType::Favourite) {
                return Ok(Some("Dino has been added to your favourites".to_string()));
            }
            Ok(None)
        }
    }
}

async fn fetch_transaction(
    conn: &mut SqliteConnection,
    user_id: &str,
    dino_id: i64,
    transaction_type: &TransactionType,
) -> Result<Option<i64>> {
    let transaction_type = transaction_type.to_string();
    let row = sqlx::query!(
        r#"INSERT OR IGNORE INTO DinoUser (id) VALUES (?);
        SELECT id FROM DinoTransactions WHERE type = ? AND dino_id = ? AND user_id = ?"#,
        user_id,
        transaction_type,
        dino_id,
        user_id
    )
    .fetch_optional(&mut *conn)
    .await?;

    Ok(row.map(|r| r.id))
}

async fn delete_transaction(conn: &mut SqliteConnection, transaction_id: i64) -> Result<()> {
    sqlx::query!("DELETE FROM DinoTransactions WHERE id = ?", transaction_id)
        .execute(&mut *conn)
        .await?;

    Ok(())
}

async fn create_transaction(
    conn: &mut SqliteConnection,
    user_id: &str,
    dino_id: i64,
    transaction_type: &TransactionType,
) -> Result<()> {
    let transaction_type = transaction_type.to_string();
    sqlx::query!(
        "INSERT INTO DinoTransactions (user_id, dino_id, type) VALUES (?, ?, ?)",
        user_id,
        dino_id,
        transaction_type
    )
    .execute(&mut *conn)
    .await?;

    Ok(())
}

async fn calculate_dino_score(conn: &mut SqliteConnection, dino_id: i64) -> Result<(i64, i64)> {
    let row = sqlx::query!(
        r#"SELECT COUNT(id) as count, type as type_ FROM DinoTransactions WHERE dino_id = ? GROUP BY type"#,
        dino_id
    )
    .fetch_all(&mut *conn)
    .await?;

    let mut map = HashMap::new();
    let mut total_transactions = 0;
    for entry in row.into_iter() {
        map.insert(entry.type_, entry.count);
        total_transactions += entry.count;
    }

    let hotness = map.get("COVET").unwrap_or(&0) - map.get("SHUN").unwrap_or(&0);
    update_dino_score(conn, dino_id, total_transactions, hotness).await?;

    Ok((total_transactions, hotness))
}

async fn update_dino_score(
    conn: &mut SqliteConnection,
    dino_id: i64,
    worth: i64,
    hotness: i64,
) -> Result<()> {
    sqlx::query!(
        "UPDATE Dino SET worth = ?, hotness = ? WHERE id = ?",
        worth,
        hotness,
        dino_id
    )
    .execute(&mut *conn)
    .await?;

    Ok(())
}

async fn fetch_dino_names(
    conn: &mut SqliteConnection,
    dino_id: i64,
) -> Result<Option<(String, String)>> {
    let row = sqlx::query!("SELECT name, filename FROM Dino WHERE id = ?", dino_id)
        .fetch_optional(&mut *conn)
        .await?;

    Ok(row.map(|r| (r.name, r.filename)))
}
