use super::character::Character;
use super::elo::{calculate_lp_difference, calculate_new_elo, LadderPosition};
use super::fight::RPGFight;

use crate::commands::rpg::elo::find_ladder_rank;
use crate::common::{
    avatar_url, ephemeral_interaction_response, ephemeral_message, name, nickname, Score,
};
use crate::Context;

use anyhow::Result;
use chrono::{NaiveDateTime, Utc};
use poise::serenity_prelude::{self as serenity, Mention, User, UserId};
use poise::serenity_prelude::{ButtonStyle, CreateActionRow};
use sqlx::{Connection, QueryBuilder, SqliteConnection};
use std::str::FromStr;
use std::time::Duration;
use tokio::sync::RwLock;

const DEAD_DUEL_COOLDOWN: Duration = Duration::from_secs(5 * 60);
const LOSS_COOLDOWN: Duration = Duration::from_secs(30);

#[poise::command(
    slash_command,
    guild_only,
    subcommands("challenge", "preview", "character", "stats", "ladder")
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
    let custom_data_lock = unwrap_custom_data(ctx);

    if custom_data_lock.read().await.in_progress {
        ephemeral_message(ctx, "A RPG fight is already in progress").await?;
        return Ok(());
    }

    let challenger = ctx.author();
    let challenger_nick = nickname(challenger, &ctx).await;
    let challenger_name = challenger_nick.as_deref().unwrap_or(&challenger.name);

    let challenger_stats = match retrieve_user_stats(ctx, challenger).await {
        Ok(stats) => stats,
        Err(e) => {
            eprintln!("Could not retrieve {}'s last loss: {e:?}", challenger.name);
            ephemeral_message(ctx, "Something went wrong when trying to join the fight.").await?;
            return Ok(());
        }
    };

    if let Err(e) = assert_no_recent_loss(&challenger_stats, challenger_name) {
        ephemeral_message(ctx, e.to_string()).await?;
        return Ok(());
    };

    let challenger_character = Character::new(
        challenger.id.0,
        challenger_name,
        &challenger_nick.as_deref(),
    );

    let accept_reply = ctx
        .send(|r| {
            r.content(format!(
                "{challenger_name} is throwing down the gauntlet in challenge..."
            ))
            .components(|c| c.add_action_row(create_accept_button()))
        })
        .await?;

    update_in_progress_status(custom_data_lock, true).await;

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

        let accepter_stats = match retrieve_user_stats(ctx, accepter).await {
            Ok(stats) => stats,
            Err(e) => {
                eprintln!("Could not retrieve {}'s stats {e:?}", accepter.name);
                let err_message = "Something went wrong when trying to retrieve your past stats.";
                ephemeral_interaction_response(&ctx, interaction, err_message).await?;
                continue;
            }
        };

        if let Err(e) = assert_no_recent_loss(&accepter_stats, accepter_name) {
            ephemeral_interaction_response(&ctx, interaction, e.to_string()).await?;
            continue;
        }

        let accepter_character =
            Character::new(accepter.id.0, accepter_name, &accepter_nick.as_deref());

        let mut fight = RPGFight::new(challenger_character, accepter_character);
        let fight_result = fight.fight();

        update_in_progress_status(custom_data_lock, false).await;

        let mut conn = ctx.data().database.acquire().await?;
        let mut transaction = conn.begin().await?;

        let new_challenger_elo = update_character_stats(
            &mut transaction,
            challenger.id.0,
            challenger_stats.elo_rank,
            accepter_stats.elo_rank,
            fight_result.to_score(true),
        )
        .await?;

        let new_accepter_elo = update_character_stats(
            &mut transaction,
            accepter.id.0,
            accepter_stats.elo_rank,
            challenger_stats.elo_rank,
            fight_result.to_score(false),
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
        update_summary_cache(ctx, reply_msg.id.0, &fight_log).await;

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
                            .components(|c| c.set_action_row(create_summary_button()))
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

    update_in_progress_status(custom_data_lock, false).await;

    Ok(())
}

async fn update_in_progress_status(custom_data_lock: &RwLock<ChallengeData>, new_status: bool) {
    let mut cmd_data = custom_data_lock.write().await;
    cmd_data.in_progress = new_status;
}

fn unwrap_custom_data(ctx: Context<'_>) -> &RwLock<ChallengeData> {
    ctx.command()
        .custom_data
        .downcast_ref::<RwLock<ChallengeData>>()
        .expect("Expected to have passed a ChallengeData struct as custom_data")
}

fn assert_no_recent_loss(stats: &CharacterPastStats, name: &str) -> Result<()> {
    let now = Utc::now().naive_utc();
    let loss_cooldown_duration = chrono::Duration::from_std(LOSS_COOLDOWN)?;

    if stats.last_loss + loss_cooldown_duration > now {
        let time_until_duel = (stats.last_loss + loss_cooldown_duration).timestamp();

        return Err(anyhow::anyhow!(
            "{name} you have recently lost a duel. Please try again <t:{time_until_duel}:R>."
        ));
    }

    Ok(())
}

async fn retrieve_user_stats(ctx: Context<'_>, user: &User) -> Result<CharacterPastStats> {
    let mut conn = ctx.data().database.acquire().await?;
    get_character_stats(&mut conn, user.id.0).await
}

async fn update_summary_cache(ctx: Context<'_>, message_id: u64, log: &str) {
    ctx.data()
        .rpg_summary_cache
        .lock()
        .await
        .put(message_id, log.to_string());
}

fn create_accept_button() -> CreateActionRow {
    let mut row = CreateActionRow::default();
    row.create_button(|f| {
        f.custom_id("rpg-btn")
            .emoji('⚔')
            .label("Accept Fight".to_string())
            .style(ButtonStyle::Primary)
    });

    row
}

fn create_summary_button() -> CreateActionRow {
    let mut summary_row = CreateActionRow::default();
    summary_row.create_button(|f| {
        f.custom_id("rpg-summary")
            .emoji('📖')
            .label("See summary".to_string())
            .style(ButtonStyle::Secondary)
    });

    summary_row
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

#[poise::command(guild_only, slash_command, prefix_command)]
async fn stats(ctx: Context<'_>, user: Option<User>, silent: Option<bool>) -> Result<()> {
    let silent = silent.unwrap_or(true);
    let user = match user {
        Some(user) => user,
        None => ctx.author().to_owned(),
    };

    let mut conn = ctx.data().database.acquire().await?;
    let user_name = name(&user, &ctx).await;
    let character_scoresheet =
        try_get_character_scoresheet(&mut conn, &user.id.to_string()).await?;
    let Some(user_record) = character_scoresheet else {
        ephemeral_message(
            ctx,
            &format!("Hmm, {user_name}... It seems you are yet to test your steel."),
        )
        .await?;
        return Ok(());
    };

    let CharacterScoresheet {
        wins,
        losses,
        draws,
        elo_rank,
        peak_elo,
        floor_elo,
        ..
    } = user_record;

    let rank = find_ladder_rank(elo_rank);
    let peak_rank = find_ladder_rank(peak_elo);
    let floor_rank = find_ladder_rank(floor_elo);

    let title = format!("{user_name}'s prowess in the  arena: {wins}W {losses}L {draws}D",);
    let current_desc = format!("{} - {} *{} League*", elo_rank, rank.icon, rank.name);
    let peak_desc = format!(
        "{} - {} *{} League*",
        peak_elo, peak_rank.icon, peak_rank.name
    );
    let floor_desc = format!(
        "{} - {} *{} League*",
        floor_elo, floor_rank.icon, floor_rank.name
    );

    ctx.send(|message| {
        message.ephemeral(silent).embed(|embed| {
            embed
                .colour(0x009933)
                .author(|a| a.icon_url(avatar_url(&user)).name(title))
                .fields(vec![
                    ("Current Rank", current_desc, false),
                    ("Peak Rank", peak_desc, false),
                    ("Floor rank", floor_desc, false),
                ])
        })
    })
    .await?;

    Ok(())
}

#[poise::command(guild_only, slash_command, prefix_command)]
async fn ladder(ctx: Context<'_>, silent: Option<bool>) -> Result<()> {
    let silent = silent.unwrap_or(true);
    let mut conn = ctx.data().database.acquire().await?;
    let ladder_state = get_ladder_state(&mut conn).await?;

    let mut fields: Vec<(LadderPosition, String, bool)> = vec![];
    if let Some(user) = ladder_state.top {
        let position = LadderPosition::Top;
        let result = ladder_result(&user.user_id, user.elo_rank, position);
        fields.push((position, result, false));
    };
    if let Some(user) = ladder_state.tail {
        let position = LadderPosition::Tail;
        let result = ladder_result(&user.user_id, user.elo_rank, position);
        fields.push((position, result, false));
    };
    if let Some(user) = ladder_state.wins {
        let position = LadderPosition::Wins;
        let result = ladder_result(&user.user_id, user.elo_rank, position);
        fields.push((position, result, false));
    };
    if let Some(user) = ladder_state.losses {
        let position = LadderPosition::Losses;
        let result = ladder_result(&user.user_id, user.elo_rank, position);
        fields.push((position, result, false));
    };

    if fields.is_empty() {
        ephemeral_message(ctx, "The arena is clean. No violence has happend yet.").await?;
        return Ok(());
    }

    ctx.send(|message| {
        message.ephemeral(silent).embed(|embed| {
            embed
                .colour(0x009933)
                .title("The State of the Ladder")
                .fields(fields)
        })
    })
    .await?;

    Ok(())
}

struct CharacterPastStats {
    last_loss: NaiveDateTime,
    elo_rank: i64,
}

async fn get_character_stats(
    conn: &mut SqliteConnection,
    user_id: u64,
) -> Result<CharacterPastStats> {
    let user_id = user_id.to_string();

    let row = sqlx::query_as!(
        CharacterPastStats,
        r#"
        INSERT OR IGNORE INTO RPGCharacter (user_id) VALUES (?);
        SELECT last_loss, elo_rank FROM RPGCharacter WHERE user_id = ?
        "#,
        user_id,
        user_id
    )
    .fetch_one(conn)
    .await?;

    Ok(row)
}

#[allow(dead_code)]
struct CharacterScoresheet {
    wins: i64,
    losses: i64,
    draws: i64,
    elo_rank: i64,
    peak_elo: i64,
    floor_elo: i64,
    user_id: String,
    last_loss: NaiveDateTime,
}

async fn try_get_character_scoresheet(
    conn: &mut SqliteConnection,
    user_id: &str,
) -> Result<Option<CharacterScoresheet>> {
    let row = sqlx::query_as!(
        CharacterScoresheet,
        r#"SELECT * FROM RPGCharacter WHERE user_id = ?"#,
        user_id
    )
    .fetch_optional(conn)
    .await?;

    Ok(row)
}

async fn update_character_stats(
    conn: &mut SqliteConnection,
    user_id: u64,
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
    query.push_bind(user_id.to_string());

    query.build().execute(conn).await?;

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
    .execute(conn)
    .await?;

    Ok(())
}

struct LadderState {
    top: Option<CharacterScoresheet>,
    tail: Option<CharacterScoresheet>,
    wins: Option<CharacterScoresheet>,
    losses: Option<CharacterScoresheet>,
}

async fn get_ladder_state(conn: &mut SqliteConnection) -> Result<LadderState> {
    let top = sqlx::query_as!(
        CharacterScoresheet,
        "SELECT * FROM RPGCharacter WHERE elo_rank = (SELECT MAX(elo_rank) FROM RPGCharacter)"
    )
    .fetch_optional(&mut *conn)
    .await?;
    let tail = sqlx::query_as!(
        CharacterScoresheet,
        "SELECT * FROM RPGCharacter WHERE elo_rank = (SELECT MIN(elo_rank) FROM RPGCharacter)"
    )
    .fetch_optional(&mut *conn)
    .await?;
    let wins = sqlx::query_as!(
        CharacterScoresheet,
        "SELECT * FROM RPGCharacter WHERE wins = (SELECT MAX(wins) FROM RPGCharacter)"
    )
    .fetch_optional(&mut *conn)
    .await?;
    let losses = sqlx::query_as!(
        CharacterScoresheet,
        "SELECT * FROM RPGCharacter WHERE wins = (SELECT MAX(wins) FROM RPGCharacter)"
    )
    .fetch_optional(&mut *conn)
    .await?;

    Ok(LadderState {
        top,
        tail,
        wins,
        losses,
    })
}

fn ladder_result(user_id: &str, score: i64, position: LadderPosition) -> String {
    let mention = match UserId::from_str(user_id) {
        Ok(id) => Mention::from(id).to_string(),
        Err(_) => "Some unknown user".to_string(),
    };
    format!(
        "{mention} {random_text} with {score} {suffix}",
        random_text = position.random_text(),
        suffix = position.suffix()
    )
}
