use super::character::Character;
use super::util::calculate_new_elo;

use crate::commands::rpg::fight::{FightResult, RPGFight};
use crate::commands::rpg::util::calculate_lp_difference;
use crate::common::{ephemeral_interaction_response, ephemeral_message, nickname, Score};
use crate::Context;

use anyhow::Result;
use chrono::{NaiveDateTime, Utc};
use poise::serenity_prelude::{self as serenity, User};
use poise::serenity_prelude::{ButtonStyle, CreateActionRow};
use sqlx::{Connection, QueryBuilder, SqliteConnection};
use std::time::Duration;
use tokio::sync::RwLock;

const DEAD_DUEL_COOLDOWN: Duration = Duration::from_secs(5 * 60);
const LOSS_COOLDOWN: Duration = Duration::from_secs(30);

#[poise::command(
    slash_command,
    guild_only,
    subcommands("challenge", "preview", "character")
)]
pub async fn rpg(_ctx: Context<'_>) -> Result<()> {
    Ok(())
}

#[derive(Default)]
struct ChallengeData {
    in_progress: bool,
}

#[poise::command(
    slash_command,
    guild_only,
    custom_data = "RwLock::new(ChallengeData::default())"
)]
async fn challenge(ctx: Context<'_>) -> Result<()> {
    let custom_data_lock = ctx
        .command()
        .custom_data
        .downcast_ref::<RwLock<ChallengeData>>()
        .expect("Expected to have passed a ChallengeData struct as custom_data");

    if custom_data_lock.read().await.in_progress {
        ephemeral_message(ctx, "A RPG fight is already in progress").await?;
        return Ok(());
    }

    let challenger = ctx.author();
    let challenger_nick = nickname(challenger, &ctx).await;
    let challenger_name = challenger_nick.as_deref().unwrap_or(&challenger.name);

    let mut conn = ctx.data().database.acquire().await?;
    let challenger_stats = match get_character_stats(&mut conn, challenger.id.to_string()).await {
        Ok(last_loss) => last_loss,
        Err(e) => {
            eprintln!(
                "Could not retrieve last loss of {} - {:?}",
                challenger.name, e
            );
            ephemeral_message(ctx, "Something went wrong when trying to join the fight.").await?;
            return Ok(());
        }
    };
    drop(conn);

    let now = Utc::now().naive_utc();
    let loss_cooldown_duration = chrono::Duration::from_std(LOSS_COOLDOWN)?;
    if challenger_stats.last_loss + loss_cooldown_duration > now {
        let time_until_duel = (challenger_stats.last_loss + loss_cooldown_duration).timestamp();
        ephemeral_message(
            ctx,
            format!(
                "{} you have recently lost a fight. Please try again <t:{}:R>.",
                challenger_name, time_until_duel
            ),
        )
        .await?;
        return Ok(());
    }

    let challenger_character = Character::new(
        challenger.id.0,
        challenger_name,
        &challenger_nick.as_deref(),
    );

    let mut row = CreateActionRow::default();
    row.create_button(|f| {
        f.custom_id("rpg-btn")
            .emoji('⚔')
            .label("Accept Fight".to_string())
            .style(ButtonStyle::Primary)
    });

    let accept_reply = ctx
        .send(|r| {
            r.content(format!(
                "{challenger_name} is throwing down the gauntlet in challenge..."
            ))
            .components(|c| c.add_action_row(row))
        })
        .await?;

    {
        let mut duel_data = custom_data_lock.write().await;
        duel_data.in_progress = true;
    }

    let reply_msg = accept_reply.message().await?;

    while let Some(interaction) = reply_msg
        .await_component_interaction(ctx)
        .timeout(DEAD_DUEL_COOLDOWN)
        .await
    {
        if interaction.data.custom_id != "rpg-btn" {
            continue;
        }

        if interaction.user.id == challenger.id {
            ephemeral_interaction_response(&ctx, interaction, "You cannot join your own fight.")
                .await?;
            continue;
        }

        if !custom_data_lock.read().await.in_progress {
            ephemeral_interaction_response(
                &ctx,
                interaction,
                "Someone beat you to the challenge already",
            )
            .await?;
            continue;
        }

        let accepter = &interaction.user;
        let accepter_nick = nickname(accepter, &ctx).await;
        let accepter_name = accepter_nick.as_deref().unwrap_or(&challenger.name);

        let mut conn = ctx.data().database.acquire().await?;
        let accepter_stats = get_character_stats(&mut conn, accepter.id.to_string()).await?;
        drop(conn);

        let now = Utc::now().naive_utc();
        if accepter_stats.last_loss + loss_cooldown_duration > now {
            let time_until_duel = (accepter_stats.last_loss + loss_cooldown_duration).timestamp();
            let content = format!(
                "{} you have recently lost a fight. Please try again <t:{}:R>.",
                accepter_name, time_until_duel
            );
            ephemeral_interaction_response(&ctx, interaction, content).await?;
            continue;
        }

        let accepter_character =
            Character::new(accepter.id.0, accepter_name, &accepter_nick.as_deref());

        let mut fight = RPGFight::new(challenger_character, accepter_character);
        let fight_result = fight.fight();

        {
            let mut cmd_data = custom_data_lock.write().await;
            cmd_data.in_progress = false;
        }

        let mut conn = ctx.data().database.acquire().await?;
        let mut transaction = conn.begin().await?;

        let new_challenger_elo = update_character_stats(
            &mut transaction,
            challenger.id.to_string(),
            challenger_stats.elo_rank,
            accepter_stats.elo_rank,
            match fight_result {
                FightResult::AccepterWin => Score::Loss,
                FightResult::ChallengerWin => Score::Win,
                FightResult::Draw => Score::Draw,
            },
        )
        .await?;

        let new_accepter_elo = update_character_stats(
            &mut transaction,
            accepter.id.to_string(),
            accepter_stats.elo_rank,
            challenger_stats.elo_rank,
            match fight_result {
                FightResult::AccepterWin => Score::Win,
                FightResult::ChallengerWin => Score::Loss,
                FightResult::Draw => Score::Draw,
            },
        )
        .await?;

        let fight_log = fight.to_string();
        new_fight_record(
            &mut transaction,
            interaction.message.id.to_string(),
            &fight_log,
        )
        .await?;

        transaction.commit().await?;

        ctx.data()
            .rpg_summary_cache
            .lock()
            .await
            .insert(reply_msg.id.0, fight_log);

        let mut summary_row = CreateActionRow::default();
        summary_row.create_button(|f| {
            f.custom_id("rpg-summary")
                .emoji('📖')
                .label("See summary".to_string())
                .style(ButtonStyle::Secondary)
        });
        let elo_change_summary = format!(
            "**{challenger_name}**{} [{new_challenger_elo}]. \
            **{accepter_name}**{} [{new_accepter_elo}].",
            calculate_lp_difference(challenger_stats.elo_rank, new_challenger_elo),
            calculate_lp_difference(accepter_stats.elo_rank, new_accepter_elo)
        );

        // NOTE: To edit the original message after a button has been pressed,
        // you first need to create an interaction response, this is allows us
        // to avoid getting the `This interaction failed` message, and then
        // using the Kind:UpdateMessage update the original message with the new
        // content/components otherwise you'd just end up sending a new message.
        interaction
            .create_interaction_response(ctx, |r| {
                r.kind(serenity::InteractionResponseType::UpdateMessage)
                    .interaction_response_data(|d| {
                        d.content(format!("{}\n{}", fight.summary(), elo_change_summary))
                            .components(|c| c.set_action_row(summary_row))
                    })
            })
            .await?;

        return Ok(());
    }

    accept_reply
        .edit(ctx, |r| {
            r.content(format!(
                "{challenger_name} failed to find someone to fight."
            ))
            .components(|c| c)
        })
        .await?;

    let mut data = custom_data_lock.write().await;
    data.in_progress = false;

    Ok(())
}

/// Preview what your character would look like with a new nickname
#[poise::command(slash_command, guild_only, prefix_command)]
async fn preview(
    ctx: Context<'_>,
    #[description = "Your new nickname"] name: String,
    #[description = "Whether the message will be shown to everyone or not"] silent: Option<bool>,
) -> Result<()> {
    if name.len() >= 256 {
        ephemeral_message(ctx, "Name have fewer than 256 characters.").await?;
        return Ok(());
    }

    let silent = silent.unwrap_or(true);
    let character = Character::new(ctx.author().id.0, &name, &Some(&name));
    ctx.send(|r| r.embed(|e| character.to_embed(e)).ephemeral(silent))
        .await?;

    Ok(())
}

/// Show your own or someone else's character stats
#[poise::command(slash_command, guild_only, prefix_command)]
async fn character(
    ctx: Context<'_>,
    #[description = "The person whose character you want to see"] user: Option<User>,
    #[description = "Whether the message will be shown to everyone or not"] silent: Option<bool>,
) -> Result<()> {
    let silent = silent.unwrap_or(true);
    let person = match user {
        Some(user) => user,
        None => ctx.author().to_owned(),
    };

    let nick = nickname(&person, &ctx).await;
    let name = nick.as_deref().unwrap_or(&person.name);
    let character = Character::new(person.id.0, name, &nick.as_deref());

    ctx.send(|r| r.embed(|e| character.to_embed(e)).ephemeral(silent))
        .await?;

    Ok(())
}

struct CharacterPastStats {
    last_loss: NaiveDateTime,
    elo_rank: i64,
}

async fn get_character_stats(
    conn: &mut SqliteConnection,
    user_id: String,
) -> Result<CharacterPastStats> {
    let row = sqlx::query_as!(
        CharacterPastStats,
        r#"
        INSERT OR IGNORE INTO RPGCharacter (user_id) VALUES (?);
        SELECT last_loss, elo_rank FROM RPGCharacter WHERE user_id = ?
        "#,
        user_id,
        user_id
    )
    .fetch_one(&mut *conn)
    .await?;

    Ok(row)
}

async fn update_character_stats(
    conn: &mut SqliteConnection,
    user_id: String,
    elo_rank: i64,
    opponent_elo_rank: i64,
    outcome: Score,
) -> Result<i64> {
    let new_elo = calculate_new_elo(elo_rank, opponent_elo_rank, &outcome);

    let update_query = match outcome {
        Score::Win => "wins = wins + 1",
        Score::Loss => "last_loss = datetime('now'), losses = losses + 1",
        Score::Draw => "draws = draws + 1",
    };

    let mut query = QueryBuilder::new("UPDATE RPGCharacter SET elo_rank = ");
    query.push_bind(new_elo);
    query.push(", peak_elo = MAX(peak_elo, ");
    query.push_bind(new_elo);
    query.push("), floor_elo = MIN(floor_elo, ");
    query.push_bind(new_elo);
    query.push("), ");
    query.push(update_query);
    query.push(" WHERE user_id = ");
    query.push_bind(&user_id);

    query.build().execute(&mut *conn).await?;

    Ok(new_elo)
}

async fn new_fight_record(
    conn: &mut SqliteConnection,
    message_id: String,
    log: &str,
) -> Result<()> {
    sqlx::query!(
        r#"INSERT INTO RPGFight (message_id, log) VALUES (?, ?)"#,
        message_id,
        log
    )
    .execute(&mut *conn)
    .await?;

    Ok(())
}