use crate::common::{
    avatar_url, colour, ephemeral_interaction_response, ephemeral_message, member, name,
    send_message_with_row, Score,
};
use crate::Context;

use anyhow::{bail, Result};
use chrono::{NaiveDateTime, Utc};
use poise::serenity_prelude::{
    ButtonStyle, CreateActionRow, InteractionResponseType, MessageComponentInteraction, User,
    UserId,
};
use rand::Rng;
use sqlx::{Connection, QueryBuilder, SqliteExecutor};
use std::cmp::Ordering;
use std::fmt::Display;
use std::time::Duration;
use tokio::sync::RwLock;

const DEAD_DUEL_COOLDOWN: Duration = Duration::from_secs(5 * 60);
// TODO: this should be replaced with a const chrono::Duration when that gets stabilized
const LOSS_COOLDOWN: i64 = 60;
const TIMEOUT_DURATION: Duration = Duration::from_secs(10 * 60);

#[derive(Default)]
struct DuelData {
    in_progress: bool,
}

/// Challenge the chat to a duel
#[poise::command(
    slash_command,
    guild_only,
    custom_data = "RwLock::new(DuelData::default())"
)]
pub async fn duel(ctx: Context<'_>) -> Result<()> {
    let challenger = DuelUser::from(ctx, ctx.author()).await;
    let custom_data_lock = unwrap_duel_data(ctx);

    if custom_data_lock.read().await.in_progress {
        ephemeral_message(ctx, "A duel is already in progress").await?;
        return Ok(());
    }

    if let Err(e) = challenger.ensure_outside_cooldown(ctx).await {
        ephemeral_message(ctx, e.to_string()).await?;
        return Ok(());
    }

    let initial_msg = format!("{challenger} is looking for a duel, press the button to accept.",);
    let accept_reply = send_message_with_row(ctx, initial_msg, create_accept_button()).await?;

    update_in_progress_status(custom_data_lock, true).await;

    while let Some(interaction) = accept_reply
        .message()
        .await?
        .await_component_interaction(ctx)
        .timeout(DEAD_DUEL_COOLDOWN)
        .await
    {
        if interaction.data.custom_id != "duel-btn" {
            continue;
        }

        // NOTE: responding with an ephemeral message does not trigger the
        // `iteraction failed` error but I'd like to find a way to just ignore
        // the click entirely with no response.
        if interaction.user.id == challenger.id {
            ephemeral_interaction_response(&ctx, interaction, "You cannot join your own duel.")
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

        let accepter = DuelUser::from(ctx, &interaction.user).await;
        if let Err(e) = accepter.ensure_outside_cooldown(ctx).await {
            ephemeral_interaction_response(&ctx, interaction, e.to_string()).await?;
            continue;
        }

        let (challenger_score, accepter_score) = {
            let mut rng = rand::thread_rng();
            (rng.gen_range(0..=100), rng.gen_range(0..=100))
        };

        let mut conn = ctx.data().database.acquire().await?;
        let mut transaction = conn.begin().await?;

        let winner_text = match challenger_score.cmp(&accepter_score) {
            Ordering::Greater => {
                update_user_score(&mut transaction, &challenger.string_id, Score::Win).await?;
                update_user_score(&mut transaction, &accepter.string_id, Score::Loss).await?;
                update_last_loss(&mut transaction, &accepter.string_id).await?;

                format!("{challenger} has won!")
            }
            Ordering::Less => {
                update_user_score(&mut transaction, &accepter.string_id, Score::Win).await?;
                update_user_score(&mut transaction, &challenger.string_id, Score::Loss).await?;
                update_last_loss(&mut transaction, &challenger.string_id).await?;

                format!("{accepter} has won!")
            }
            Ordering::Equal => {
                update_user_score(&mut transaction, &challenger.string_id, Score::Draw).await?;
                update_user_score(&mut transaction, &accepter.string_id, Score::Draw).await?;

                // NOTE: interaction fails if the user is the owner of the server
                let timeout_end_time = Utc::now() + chrono::Duration::from_std(TIMEOUT_DURATION)?;
                if let Some(mut challenger_as_member) = member(&ctx).await {
                    challenger_as_member
                        .to_mut()
                        .disable_communication_until_datetime(ctx, timeout_end_time.into())
                        .await?;
                };
                if let Some(mut accepter_as_member) = interaction.member.clone() {
                    accepter_as_member
                        .disable_communication_until_datetime(ctx, timeout_end_time.into())
                        .await?;
                };

                "It's a draw! Now go sit in a corner for 10 mintues and think about your actions..."
                    .into()
            }
        };
        transaction.commit().await?;

        let final_message = format!("{accepter} has rolled a {accepter_score} and {challenger} has rolled a {challenger_score}. {winner_text}");
        send_interaction_update(ctx, &interaction, final_message).await?;

        update_in_progress_status(custom_data_lock, false).await;

        return Ok(());
    }

    let duel_timeout_msg = format!("{challenger} failed to find someone to duel.");
    accept_reply
        .edit(ctx, |f| f.content(duel_timeout_msg).components(|c| c))
        .await?;

    update_in_progress_status(custom_data_lock, false).await;

    Ok(())
}

async fn send_interaction_update(
    ctx: Context<'_>,
    interaction: &MessageComponentInteraction,
    content: impl ToString,
) -> Result<()> {
    interaction
        .create_interaction_response(ctx, |r| {
            r.kind(InteractionResponseType::UpdateMessage)
                .interaction_response_data(|d| d.content(content).components(|c| c))
        })
        .await?;

    Ok(())
}

/// Display your duel statistics
#[poise::command(slash_command)]
pub async fn duelstats(ctx: Context<'_>) -> Result<()> {
    let user = ctx.author();
    let conn = &mut ctx.data().database.acquire().await?;

    let Some(stats) = get_duel_stats(conn, user.id.to_string()).await? else {
        ephemeral_message(ctx, "You have never dueled before.").await?;
        return Ok(());
    };

    let name = name(&ctx, user).await;
    let colour = colour(&ctx).await.unwrap_or_else(|| 0x77618F.into());

    ctx.send(|r| {
        r.embed(|e| {
            e.colour(colour)
                .description(format!(
                    "{}\n{}\n{}",
                    stats.current_streak(),
                    stats.best_streak(),
                    stats.worst_streak()
                ))
                .author(|a| {
                    a.icon_url(avatar_url(user)).name(format!(
                        "{name}'s scoresheet: {}-{}-{}",
                        stats.wins, stats.losses, stats.draws
                    ))
                })
        })
    })
    .await?;

    Ok(())
}

async fn get_last_loss(executor: impl SqliteExecutor<'_>, user_id: &str) -> Result<NaiveDateTime> {
    let row = sqlx::query!(
        r#"
        INSERT OR IGNORE INTO User (id) VALUES (?);
        SELECT last_loss From User WHERE id = ?
        "#,
        user_id,
        user_id
    )
    .fetch_one(executor)
    .await?;

    Ok(row.last_loss)
}

async fn update_last_loss(executor: impl SqliteExecutor<'_>, user_id: &str) -> Result<()> {
    sqlx::query!(
        "UPDATE User SET last_loss = datetime('now') WHERE id = ?",
        user_id
    )
    .execute(executor)
    .await?;

    Ok(())
}

async fn update_user_score(
    executor: impl SqliteExecutor<'_>,
    user_id: &str,
    score: Score,
) -> Result<()> {
    let update_query = match score {
        Score::Win => {
            r#"wins = wins + 1,
            loss_streak = 0,
            win_streak = win_streak + 1,
            win_streak_max = MAX(win_streak_max, win_streak + 1)"#
        }
        Score::Loss => {
            r#"losses = losses + 1,
            win_streak = 0,
            loss_streak = loss_streak + 1,
            loss_streak_max = MAX(loss_streak_max, loss_streak + 1)"#
        }
        Score::Draw => {
            r#"draws = draws + 1,
            win_streak = 0,
            loss_streak = 0"#
        }
    };

    let mut query = QueryBuilder::new("INSERT OR IGNORE INTO Duels (user_id) VALUES (");
    query.push_bind(user_id);
    query.push(format!(
        "); UPDATE Duels SET {update_query} WHERE user_id = "
    ));
    query.push_bind(user_id);
    query.build().execute(executor).await?;

    Ok(())
}

struct DuelStats {
    #[allow(dead_code)]
    user_id: String,
    losses: i64,
    wins: i64,
    draws: i64,
    win_streak: i64,
    loss_streak: i64,
    win_streak_max: i64,
    loss_streak_max: i64,
}

impl DuelStats {
    fn current_streak(&self) -> String {
        match (self.win_streak, self.loss_streak, self.draws) {
            (0, 0, 0) => "You have never dueled before".to_string(),
            (0, 0, _) => "Your last duel was a draw".to_string(),
            (0, _, _) => format!("Current streak **{} losses**", self.loss_streak),
            (_, 0, _) => format!("Current streak **{} wins**", self.win_streak),
            _ => unreachable!(),
        }
    }

    fn best_streak(&self) -> String {
        format!("Best streak: **{} wins**", self.win_streak_max)
    }

    fn worst_streak(&self) -> String {
        format!("Worst streak: **{} losses**", self.loss_streak_max)
    }
}

async fn get_duel_stats(
    executor: impl SqliteExecutor<'_>,
    user_id: String,
) -> Result<Option<DuelStats>> {
    let stats = sqlx::query_as!(
        DuelStats,
        r#"SELECT * FROM Duels WHERE user_id = ?"#,
        user_id
    )
    .fetch_optional(executor)
    .await?;

    Ok(stats)
}

fn unwrap_duel_data(ctx: Context<'_>) -> &RwLock<DuelData> {
    ctx.command()
        .custom_data
        .downcast_ref::<RwLock<DuelData>>()
        .expect("Expected to have passed a DuelData struct as custom_data")
}

async fn update_in_progress_status(custom_data_lock: &RwLock<DuelData>, new_status: bool) {
    let mut cmd_data = custom_data_lock.write().await;
    cmd_data.in_progress = new_status;
}

fn create_accept_button() -> CreateActionRow {
    let mut row = CreateActionRow::default();
    row.create_button(|f| {
        f.custom_id("duel-btn")
            .emoji('🎲')
            .label("Accept Duel".to_string())
            .style(ButtonStyle::Primary)
    });

    row
}

struct DuelUser {
    id: UserId,
    string_id: String,
    name: String,
}

impl DuelUser {
    async fn from(ctx: Context<'_>, user: &User) -> Self {
        let id = user.id;

        Self {
            id,
            string_id: id.to_string(),
            name: name(&ctx, user).await,
        }
    }

    async fn ensure_outside_cooldown(&self, ctx: Context<'_>) -> Result<()> {
        let last_loss = match get_last_loss(&ctx.data().database, &self.string_id).await {
            Ok(last_loss) => last_loss,
            Err(e) => {
                eprintln!("Could not get {self}'s last loss: {e:?}");
                bail!("Couldn't get your last loss, no duel for you! :<");
            }
        };

        let now = Utc::now().naive_utc();

        let loss_cooldown_duration = chrono::Duration::minutes(LOSS_COOLDOWN);
        if last_loss + loss_cooldown_duration > now {
            let time_until_duel = (last_loss + loss_cooldown_duration).timestamp();
            bail!("{self} you have recently lost a duel. Please try again <t:{time_until_duel}:R>.")
        }

        Ok(())
    }
}

impl Display for DuelUser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}
