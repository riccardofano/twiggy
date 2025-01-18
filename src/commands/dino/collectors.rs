use std::fmt::Display;
use std::path::Path;

use ::serenity::all::CreateAttachment;
use anyhow::bail;
use poise::futures_util::StreamExt;
use poise::serenity_prelude::{
    self as serenity, ComponentInteraction, ComponentInteractionCollector,
};
use poise::serenity_prelude::{CreateEmbed, CreateEmbedFooter};
use sqlx::SqliteConnection;

use crate::commands::dino::{COVET_BUTTON, FAVOURITE_BUTTON, SHUN_BUTTON};
use crate::common::{embed_message, ephemeral_text_message, response, update_response};
use crate::Data;
use crate::Result;

use super::{get_dino_image_path, get_dino_record, quirkify_hotness, quirkify_worth, OUTPUT_PATH};

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
    let mut collector = ComponentInteractionCollector::new(ctx)
        .filter(|f| f.data.custom_id.starts_with("dino-"))
        .stream();

    println!("Setup dino collector");

    while let Some(interaction) = collector.next().await {
        if let Err(e) = handle_dino_collector(ctx, user_data, &interaction).await {
            eprintln!("Error while handling dino collection: {e}");
        }
    }

    Ok(())
}

async fn handle_dino_collector(
    ctx: &serenity::Context,
    user_data: &Data,
    interaction: &ComponentInteraction,
) -> Result<()> {
    let custom_id = &interaction.data.custom_id;
    let split_btn_id = custom_id.split(':').collect::<Vec<_>>();
    let dino_id = split_btn_id.get(1);

    let Some(dino_id) = dino_id else {
        let resp = response(ephemeral_text_message("Could not find a dino id"));
        interaction.create_response(ctx, resp).await?;
        return Ok(());
    };

    let Ok(dino_id) = dino_id.parse::<i64>() else {
        let resp = response(ephemeral_text_message("Dino id is not valid."));
        interaction.create_response(ctx, resp).await?;
        return Ok(());
    };

    let mut transaction = user_data.database.begin().await?;
    let dino = fetch_dino_names(&mut transaction, dino_id).await?;

    let Some((dino_name, dino_image_name)) = dino else {
        let old_embed = interaction.message.embeds[0].clone();

        let dino_name = &old_embed.title.as_deref().unwrap_or("Unknown dino");
        let new_title = format!("{dino_name} is no longer with us 😔");
        let new_embed = CreateEmbed::from(old_embed).title(new_title);

        let resp = update_response(embed_message(new_embed).components(Vec::new()));
        interaction.create_response(ctx, resp).await?;
        return Ok(());
    };

    let button_type = match &custom_id {
        b if b.starts_with(COVET_BUTTON) => TransactionType::Covet,
        b if b.starts_with(SHUN_BUTTON) => TransactionType::Shun,
        b if b.starts_with(FAVOURITE_BUTTON) => TransactionType::Favourite,
        _ => bail!("unknown dino button pressed"),
    };

    let press_response =
        handle_button_press(interaction, &mut transaction, dino_id, button_type).await?;

    if let Some(content) = press_response {
        interaction
            .create_response(ctx, response(ephemeral_text_message(&content)))
            .await?;
    } else {
        let (gifts, hotness) = calculate_dino_score(&mut transaction, dino_id).await?;

        let old_embed = interaction.message.embeds[0].clone();
        let old_image = old_embed.image.clone();
        let new_embed =
            CreateEmbed::from(old_embed)
                .title(&dino_name)
                .footer(CreateEmbedFooter::new(format!(
                    "{dino_name} is worth {} Dino Bucks!\nHotness Rating: {}",
                    quirkify_worth(gifts + 1), // +1 because the hatcher is also an owner
                    quirkify_hotness(hotness)
                )));

        // NOTE: Sometimes after pressing a button the image in the embed will disappear.
        // The image url is still there but the image doesn't exist anymore for some reason.
        // It might be when the attachment gets updated almost immediately,
        // maybe it thinks it was uploaded by mistakes and doesn't bother keeping it around.
        //
        // I can detect this by checking its width/height, if they are 0, the image is broken.
        // This will show the image again when it's missing but it doesn't prevent the image from going missing.
        // ):
        let should_refetch = match old_image {
            Some(image) => image.width.unwrap_or_default() == 0,
            None => true,
        };
        if should_refetch {
            let mut dino_image_path = Path::new(OUTPUT_PATH).join(dino_image_name);
            if !dino_image_path.exists() {
                let dino_record =
                    get_dino_record(&mut transaction, &dino_name)
                        .await?
                        .ok_or(anyhow::anyhow!(
                            "Failed to get record of dino named {dino_name:?}"
                        ))?;
                dino_image_path = get_dino_image_path(&dino_record)?;
            }

            let new_embed =
                new_embed.attachment(dino_image_path.file_name().unwrap().to_str().unwrap());
            let response =
                embed_message(new_embed).add_file(CreateAttachment::path(dino_image_path).await?);

            interaction
                .create_response(ctx, update_response(response))
                .await?;

            return Ok(());
        }

        interaction
            .create_response(ctx, update_response(embed_message(new_embed)))
            .await?;
    }

    transaction.commit().await?;

    Ok(())
}

async fn handle_button_press(
    interaction: &ComponentInteraction,
    conn: &mut SqliteConnection,
    dino_id: i64,
    button_type: TransactionType,
) -> Result<Option<String>> {
    let user_id = interaction.user.id.to_string();
    let same_type_transaction = fetch_transaction(conn, &user_id, dino_id, &button_type).await?;

    let message = match (&button_type, same_type_transaction) {
        (TransactionType::Favourite, Some(id)) => {
            delete_transaction(conn, id).await?;
            Some("Dino has been removed from your favourites".to_owned())
        }
        (TransactionType::Covet | TransactionType::Shun, Some(id)) => {
            delete_transaction(conn, id).await?;
            None
        }
        (TransactionType::Favourite, None) => {
            create_transaction(conn, &user_id, dino_id, &button_type).await?;
            Some("Dino has been added to your favourites".to_owned())
        }
        (TransactionType::Covet | TransactionType::Shun, None) => {
            let opposite_type = button_type
                .opposite()
                .expect("Buttons without an opposite have already been handled");

            if let Some(id) = fetch_transaction(conn, &user_id, dino_id, &opposite_type).await? {
                delete_transaction(conn, id).await?;
            }

            create_transaction(conn, &user_id, dino_id, &button_type).await?;
            None
        }
    };

    Ok(message)
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
    .fetch_optional(conn)
    .await?;

    Ok(row.map(|r| r.id))
}

async fn delete_transaction(conn: &mut SqliteConnection, transaction_id: i64) -> Result<()> {
    sqlx::query!("DELETE FROM DinoTransactions WHERE id = ?", transaction_id)
        .execute(conn)
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
    .execute(conn)
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

    let mut covets = 0;
    let mut shuns = 0;
    let mut gifts = 0; // gifts *should* be synced with the amount of owners a dino has had

    for entry in row.into_iter() {
        match entry.type_.as_ref() {
            "COVET" => covets += entry.count,
            "SHUN" => shuns += entry.count,
            "GIFT" => gifts += entry.count,
            _ => {}
        }
    }

    let hotness = covets - shuns;
    update_dino_score(conn, dino_id, hotness).await?;

    Ok((gifts, hotness))
}

async fn update_dino_score(conn: &mut SqliteConnection, dino_id: i64, hotness: i64) -> Result<()> {
    sqlx::query!("UPDATE Dino SET hotness = ? WHERE id = ?", hotness, dino_id)
        .execute(conn)
        .await?;

    Ok(())
}

async fn fetch_dino_names(
    conn: &mut SqliteConnection,
    dino_id: i64,
) -> Result<Option<(String, String)>> {
    let row = sqlx::query!("SELECT name, filename FROM Dino WHERE id = ?", dino_id)
        .fetch_optional(conn)
        .await?;

    Ok(row.map(|r| (r.name, r.filename)))
}
