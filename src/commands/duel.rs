use crate::common::{
    avatar_url, bail_reply, colour, ephemeral_text_message, name, reply_with_buttons, response,
    text_message, update_response,
};
use crate::Context;

use anyhow::{bail, Result};
use chrono::{DateTime, NaiveDateTime, Utc};
use poise::serenity_prelude::{
    ButtonStyle, CreateActionRow, CreateButton, CreateEmbed, CreateEmbedAuthor, Member, User,
    UserId,
};
use poise::CreateReply;
use rand::Rng;
use serenity::all::{ComponentInteraction, ComponentInteractionCollector, MessageId};
use sqlx::{Connection, SqliteExecutor, Transaction};
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
        return bail_reply(ctx, "A duel is already in progress").await;
    }

    if let Err(e) = challenger.ensure_outside_cooldown(ctx).await {
        return bail_reply(ctx, e.to_string()).await;
    }

    let initial_msg = format!("{challenger} is looking for a duel, press the button to accept.",);
    let accept_reply = ctx
        .send(reply_with_buttons(
            initial_msg,
            vec![create_accept_button()],
        ))
        .await?;

    update_in_progress_status(custom_data_lock, true).await;

    let opponent = find_opponent(
        ctx,
        custom_data_lock,
        accept_reply.message().await?.id,
        challenger.id.get(),
    )
    .await;

    let Some((interaction, accepter)) = opponent else {
        let duel_timeout_msg = format!("{challenger} failed to find someone to duel.");
        accept_reply
            .edit(ctx, reply_with_buttons(duel_timeout_msg, Vec::new()))
            .await?;

        update_in_progress_status(custom_data_lock, false).await;
        return Ok(());
    };

    let (challenger_score, accepter_score) = pick_scores();

    let mut conn = ctx.data().database.acquire().await?;
    let mut transaction = conn.begin().await?;

    let winner_text = match challenger_score.cmp(&accepter_score) {
        Ordering::Greater => {
            let (winner_id, loser_id) = (&challenger.string_id, &accepter.string_id);
            update_users_win_loss(&mut transaction, winner_id, loser_id).await?;
            update_last_loss(&mut transaction, &accepter.string_id).await?;

            format!("{challenger} has won!")
        }
        Ordering::Less => {
            let (winner_id, loser_id) = (&accepter.string_id, &challenger.string_id);
            update_users_win_loss(&mut transaction, winner_id, loser_id).await?;
            update_last_loss(&mut transaction, loser_id).await?;

            format!("{accepter} has won!")
        }
        Ordering::Equal => {
            update_users_drawn(&mut transaction, &challenger.string_id, &accepter.string_id)
                .await?;

            let timeout_end_time = Utc::now() + chrono::Duration::from_std(TIMEOUT_DURATION)?;
            let challenger_member = ctx.author_member().await.map(|m| m.into_owned());
            timeout_user(ctx, challenger_member, timeout_end_time).await;
            timeout_user(ctx, interaction.member.clone(), timeout_end_time).await;

            "It's a draw! Now go sit in a corner for 10 mintues and think about your actions..."
                .into()
        }
    };
    transaction.commit().await?;

    let final_message = format!("{accepter} has rolled a {accepter_score} and {challenger} has rolled a {challenger_score}. {winner_text}");
    let update_resp = update_response(text_message(final_message));
    interaction.create_response(ctx, update_resp).await?;

    update_in_progress_status(custom_data_lock, false).await;

    Ok(())
}

async fn find_opponent(
    ctx: Context<'_>,
    custom_data: &RwLock<DuelData>,
    message_id: MessageId,
    challenger_id: u64,
) -> Option<(ComponentInteraction, DuelUser)> {
    while let Some(interaction) = ComponentInteractionCollector::new(ctx)
        .message_id(message_id)
        .filter(move |f| f.data.custom_id == "duel-btn")
        .timeout(DEAD_DUEL_COOLDOWN)
        .await
    {
        // NOTE: responding with an ephemeral message does not trigger the
        // `iteraction failed` error but I'd like to find a way to just ignore
        // the click entirely with no response.
        if interaction.user.id == challenger_id {
            let resp = response(ephemeral_text_message("You cannot join your own duel."));
            interaction.create_response(ctx, resp).await.ok()?;
            continue;
        }

        if !custom_data.read().await.in_progress {
            let resp = response(text_message("Someone beat you to the challenge already"));
            interaction.create_response(ctx, resp).await.ok()?;
            continue;
        }

        let accepter = DuelUser::from(ctx, &interaction.user).await;
        if let Err(e) = accepter.ensure_outside_cooldown(ctx).await {
            let resp = response(text_message(e.to_string()));
            interaction.create_response(ctx, resp).await.ok()?;
            continue;
        }

        return Some((interaction, accepter));
    }

    None
}

/// Display your duel statistics
#[poise::command(slash_command)]
pub async fn duelstats(ctx: Context<'_>) -> Result<()> {
    let user = ctx.author();
    let conn = &mut ctx.data().database.acquire().await?;

    let Some(stats) = get_duel_stats(conn, user.id.to_string()).await? else {
        return bail_reply(ctx, "You have never dueled before.").await;
    };

    let name = name(&ctx, user).await;
    let colour = colour(&ctx).await.unwrap_or_else(|| 0x77618F.into());
    let embed = CreateEmbed::default()
        .colour(colour)
        .description(format!(
            "{}\n{}\n{}",
            stats.current_streak(),
            stats.best_streak(),
            stats.worst_streak()
        ))
        .author(
            CreateEmbedAuthor::new(format!(
                "{name}'s scoresheet: {}-{}-{}",
                stats.wins, stats.losses, stats.draws
            ))
            .icon_url(avatar_url(user)),
        );

    ctx.send(CreateReply::default().embed(embed)).await?;

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

async fn update_users_win_loss(
    executor: &mut Transaction<'_, sqlx::Sqlite>,
    winner_id: &str,
    loser_id: &str,
) -> Result<()> {
    sqlx::query!(
        r#"INSERT OR IGNORE INTO Duels (user_id) VALUES (?);
        UPDATE Duels SET
            wins = wins + 1,
            win_streak = win_streak + 1,
            win_streak_max = MAX(win_streak_max, win_streak + 1),
            loss_streak = 0
        WHERE user_id = ?;
        INSERT OR IGNORE INTO Duels (user_id) VALUES (?);
        UPDATE Duels SET
            losses = losses + 1,
            loss_streak = loss_streak + 1,
            loss_streak_max = MAX(loss_streak_max, loss_streak + 1),
            win_streak = 0
        WHERE user_id = ?"#,
        winner_id,
        winner_id,
        loser_id,
        loser_id
    )
    .execute(&mut *executor)
    .await?;

    Ok(())
}

async fn update_users_drawn(
    executor: &mut Transaction<'_, sqlx::Sqlite>,
    challenger_id: &str,
    accepter_id: &str,
) -> Result<()> {
    sqlx::query!(
        r#"INSERT OR IGNORE INTO Duels (user_id) VALUES (?);
        UPDATE Duels SET draws = draws + 1, win_streak = 0, loss_streak = 0 WHERE user_id = ?"#,
        challenger_id,
        challenger_id
    )
    .execute(&mut *executor)
    .await?;

    sqlx::query!(
        r#"INSERT OR IGNORE INTO Duels (user_id) VALUES (?);
        UPDATE Duels SET draws = draws + 1, win_streak = 0, loss_streak = 0 WHERE user_id = ?"#,
        accepter_id,
        accepter_id
    )
    .execute(&mut *executor)
    .await?;

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

async fn timeout_user(ctx: Context<'_>, member: Option<Member>, until: DateTime<Utc>) {
    let Some(mut member) = member else {
        return;
    };

    if let Err(e) = member
        .disable_communication_until_datetime(ctx, until.into())
        .await
    {
        eprintln!("Failed to timeout {}, reason: {e:?}", member.user.name);
    }
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
    let btn = CreateButton::new("duel-btn")
        .emoji('🎲')
        .label("Accept Duel".to_string())
        .style(ButtonStyle::Primary);

    CreateActionRow::Buttons(vec![btn])
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
            let time_until_duel = (last_loss + loss_cooldown_duration).and_utc().timestamp();
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

fn pick_scores() -> (usize, usize) {
    let mut rng = rand::thread_rng();
    (rng.gen_range(0..=100), rng.gen_range(0..=100))
}
