use super::character::Character;
use super::elo::{calculate_lp_difference, calculate_new_elo, LadderPosition};
use super::fight::RPGFight;

use crate::commands::rpg::elo::find_ladder_rank;
use crate::common::{
    avatar_url, bail_reply, ephemeral_text_message, name, nickname, reply_with_buttons, response,
    text_message, update_response, Score,
};
use crate::Context;

use anyhow::{bail, Result};
use chrono::{NaiveDateTime, Utc};
use poise::serenity_prelude::{ButtonStyle, CreateActionRow};
use poise::serenity_prelude::{
    CreateButton, CreateEmbed, CreateEmbedAuthor, Mention, User, UserId,
};
use poise::{CreateReply, ReplyHandle};
use serenity::all::{ComponentInteraction, ComponentInteractionCollector, MessageId};
use sqlx::{Connection, QueryBuilder, SqliteConnection};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const DEAD_DUEL_COOLDOWN: Duration = Duration::from_secs(5 * 60);
const LOSS_COOLDOWN: Duration = Duration::from_secs(30);

static IN_PROGRESS: AtomicBool = AtomicBool::new(false);

#[poise::command(
    slash_command,
    guild_only,
    subcommands("challenge", "preview", "character", "stats", "ladder")
)]
pub async fn rpg(_ctx: Context<'_>) -> Result<()> {
    Ok(())
}

/// Challenge other chatters and prove your strength.
#[poise::command(slash_command, guild_only)]
async fn challenge(ctx: Context<'_>) -> Result<()> {
    if IN_PROGRESS.load(Ordering::Acquire) {
        return bail_reply(ctx, "A RPG fight is already in progress").await;
    }

    let challenger = ctx.author();
    let challenger_nick = nickname(&ctx, challenger).await;
    let challenger_nick = challenger_nick.as_deref();

    let Ok(challenger_stats) = retrieve_user_stats(ctx, challenger).await else {
        return bail_reply(ctx, "Something went wrong when trying to join the fight.").await;
    };

    if let Err(e) = assert_no_recent_loss(&challenger_stats) {
        return bail_reply(ctx, e.to_string()).await;
    };

    let challenger_character = Character::new(
        challenger.id.get(),
        challenger_nick.unwrap_or(&challenger.name),
        challenger_nick,
    );

    let reply_content = format!(
        "{} is throwing down the gauntlet in challenge...",
        challenger_character.name
    );
    let reply_handle = ctx
        .send(reply_with_buttons(
            reply_content,
            vec![create_accept_button()],
        ))
        .await?;

    IN_PROGRESS.store(true, Ordering::Release);
    if let Err(e) = run_duel(ctx, challenger_character, challenger_stats, reply_handle).await {
        eprintln!("Failed to run duel to completion: {e:?}");
    }
    IN_PROGRESS.store(false, Ordering::Release);

    Ok(())
}

async fn run_duel(
    ctx: Context<'_>,
    challenger_character: Character,
    challenger_stats: CharacterPastStats,
    reply_handle: ReplyHandle<'_>,
) -> Result<()> {
    let message = reply_handle.message().await?;
    let Some(interaction) = find_opponent(ctx, message.id, challenger_character.user_id).await?
    else {
        let content = format!(
            "No one was brave enough to do battle with **{}**",
            challenger_character.name
        );
        reply_handle
            .edit(ctx, reply_with_buttons(content, Vec::new()))
            .await?;
        return Ok(());
    };

    let accepter = &interaction.user;
    let accepter_nick = nickname(&ctx, accepter).await;
    let accepter_nick = accepter_nick.as_deref();

    let accepter_character = Character::new(
        accepter.id.get(),
        accepter_nick.unwrap_or(&accepter.name),
        accepter_nick,
    );
    let accepter_stats = retrieve_user_stats(ctx, accepter).await?;

    let mut fight = RPGFight::new(challenger_character, accepter_character);
    let fight_result = fight.fight();

    let mut conn = ctx.data().database.acquire().await?;
    let mut transaction = conn.begin().await?;

    let new_challenger_elo = update_character_stats(
        &mut transaction,
        fight.challenger.user_id,
        challenger_stats.elo_rank,
        accepter_stats.elo_rank,
        fight_result.to_score(true),
    )
    .await?;

    let new_accepter_elo = update_character_stats(
        &mut transaction,
        accepter.id.get(),
        accepter_stats.elo_rank,
        challenger_stats.elo_rank,
        fight_result.to_score(false),
    )
    .await?;

    let fight_log = fight.to_string();
    new_fight_record(&mut transaction, &message.id.to_string(), &fight_log).await?;

    transaction.commit().await?;
    update_summary_cache(ctx, message.id.get(), &fight_log).await;

    let elo_change_summary = format!(
        "**{}**{} [{new_challenger_elo}]. \
            **{}**{} [{new_accepter_elo}].",
        &fight.challenger.name,
        calculate_lp_difference(challenger_stats.elo_rank, new_challenger_elo),
        &fight.accepter.name,
        calculate_lp_difference(accepter_stats.elo_rank, new_accepter_elo)
    );

    let final_message = format!("{}\n{}", fight.summary(), elo_change_summary);
    let update_resp =
        update_response(text_message(final_message).components(vec![create_summary_button()]));
    interaction.create_response(ctx, update_resp).await?;

    Ok(())
}

async fn find_opponent(
    ctx: Context<'_>,
    message_id: MessageId,
    challenger_id: u64,
) -> Result<Option<ComponentInteraction>> {
    while let Some(interaction) = ComponentInteractionCollector::new(ctx)
        .message_id(message_id)
        .filter(move |f| f.data.custom_id == "rpg-btn")
        .timeout(DEAD_DUEL_COOLDOWN)
        .await
    {
        if interaction.user.id == challenger_id {
            let resp = response(ephemeral_text_message("You cannot join your own fight."));
            interaction.create_response(ctx, resp).await?;
            continue;
        }

        if !IN_PROGRESS.load(Ordering::Acquire) {
            let resp = response(ephemeral_text_message(
                "Someone beat you to the challenge already.",
            ));
            interaction.create_response(ctx, resp).await?;
            continue;
        }

        let accepter_stats = retrieve_user_stats(ctx, &interaction.user).await?;
        if let Err(e) = assert_no_recent_loss(&accepter_stats) {
            interaction
                .create_response(ctx, response(ephemeral_text_message(e.to_string())))
                .await?;
            continue;
        }

        return Ok(Some(interaction));
    }

    Ok(None)
}

fn assert_no_recent_loss(stats: &CharacterPastStats) -> Result<()> {
    let now = Utc::now().naive_utc();
    let loss_cooldown_duration = chrono::Duration::from_std(LOSS_COOLDOWN)?;

    if stats.last_loss + loss_cooldown_duration > now {
        let time_until_duel = (stats.last_loss + loss_cooldown_duration)
            .and_utc()
            .timestamp();

        bail!("You have recently lost a duel. Please try again <t:{time_until_duel}:R>.");
    }

    Ok(())
}

async fn retrieve_user_stats(ctx: Context<'_>, user: &User) -> Result<CharacterPastStats> {
    let mut conn = ctx.data().database.acquire().await?;
    get_character_stats(&mut conn, user.id.get()).await
}

async fn update_summary_cache(ctx: Context<'_>, message_id: u64, log: &str) {
    ctx.data()
        .rpg_summary_cache
        .lock()
        .await
        .put(message_id, log.to_string());
}

fn create_accept_button() -> CreateActionRow {
    let btn = CreateButton::new("rpg-btn")
        .emoji('⚔')
        .label("Accept Fight".to_string())
        .style(ButtonStyle::Primary);

    CreateActionRow::Buttons(vec![btn])
}

fn create_summary_button() -> CreateActionRow {
    let btn = CreateButton::new("rpg-summary")
        .emoji('📖')
        .label("See summary".to_string())
        .style(ButtonStyle::Secondary);

    CreateActionRow::Buttons(vec![btn])
}

/// Preview what your character would look like with a new nickname
#[poise::command(slash_command, guild_only, prefix_command)]
async fn preview(
    ctx: Context<'_>,
    #[description = "Your new nickname"] name: String,
    #[description = "Whether the message will be shown to everyone or not"] silent: Option<bool>,
) -> Result<()> {
    if name.len() >= 256 {
        return bail_reply(ctx, "Name must have fewer than 256 characters.").await;
    }

    let silent = silent.unwrap_or(true);
    let character = Character::new(ctx.author().id.get(), &name, Some(&name));
    ctx.send(
        CreateReply::default()
            .embed(character.to_embed())
            .ephemeral(silent),
    )
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
    let user = user.as_ref().unwrap_or_else(|| ctx.author());

    let nick = nickname(&ctx, user).await;
    let name = nick.as_deref().unwrap_or(&user.name);
    let character = Character::new(user.id.get(), name, nick.as_deref());

    ctx.send(
        CreateReply::default()
            .embed(character.to_embed())
            .ephemeral(silent),
    )
    .await?;

    Ok(())
}

/// Display your fight statistics
#[poise::command(guild_only, slash_command, prefix_command)]
async fn stats(ctx: Context<'_>, user: Option<User>, silent: Option<bool>) -> Result<()> {
    let silent = silent.unwrap_or(true);
    let user = user.as_ref().unwrap_or_else(|| ctx.author());

    let mut conn = ctx.data().database.acquire().await?;
    let user_name = name(&ctx, user).await;
    let character_scoresheet =
        try_get_character_scoresheet(&mut conn, &user.id.to_string()).await?;
    let Some(user_record) = character_scoresheet else {
        let msg = format!("Hmm, {user_name}... It seems you are yet to test your steel.");
        return bail_reply(ctx, msg).await;
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
    let embed = CreateEmbed::default()
        .colour(0x009933)
        .author(CreateEmbedAuthor::new(title).icon_url(avatar_url(user)))
        .fields(vec![
            ("Current Rank", current_desc, false),
            ("Peak Rank", peak_desc, false),
            ("Floor rank", floor_desc, false),
        ]);

    ctx.send(CreateReply::default().ephemeral(silent).embed(embed))
        .await?;

    Ok(())
}

/// Who is the strongest chatter around?
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
        return bail_reply(ctx, "The arena is clean. No violence has happend yet.").await;
    }
    let embed = CreateEmbed::default()
        .colour(0x009933)
        .title("The State of the Ladder")
        .fields(fields);

    ctx.send(CreateReply::default().ephemeral(silent).embed(embed))
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

async fn new_fight_record(conn: &mut SqliteConnection, message_id: &str, log: &str) -> Result<()> {
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
