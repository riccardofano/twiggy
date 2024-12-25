use crate::common::{
    ephemeral_text_message, reply_with_buttons, response, text_message, update_response,
};
use crate::core::{CoreCollector, CoreContext, CoreInteraction, CoreReplyHandle};
use crate::Context as DiscordContext;

use anyhow::{bail, Context, Result};
use chrono::{NaiveDateTime, Utc};
use poise::futures_util::StreamExt;
use poise::serenity_prelude::{
    ButtonStyle, CreateActionRow, CreateButton, CreateEmbed, CreateEmbedAuthor, UserId,
};
use poise::CreateReply;
use rand::Rng;
use serenity::all::{Colour, Mention, MessageId};
use sqlx::{Connection, SqliteExecutor, Transaction};
use std::cmp::Ordering;
use std::fmt::Display;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::time::Duration;

// TODO: this should be replaced with a const chrono::Duration when that gets stabilized
const LOSS_COOLDOWN: i64 = 60;
const DEAD_DUEL_COOLDOWN: Duration = Duration::from_secs(5 * 60);
const TIMEOUT_DURATION: Duration = Duration::from_secs(10 * 60);

static IN_PROGRESS: AtomicBool = AtomicBool::new(false);

/// Challenge the chat to a duel
#[poise::command(slash_command, guild_only)]
pub async fn duel(ctx: DiscordContext<'_>) -> Result<()> {
    duel_impl(ctx, &IN_PROGRESS).await
}
async fn duel_impl<C: CoreContext>(ctx: C, in_progress: &AtomicBool) -> Result<()> {
    let challenger = match find_challenger(&ctx, in_progress).await {
        Ok(challenger) => challenger,
        Err(e) => return ctx.bail(&e.to_string()).await,
    };

    let reply_content = format!("{challenger} is looking for a duel, press the button to accept.");
    let builder = reply_with_buttons(reply_content, vec![create_accept_button()]);
    let reply_handle = ctx.reply_with_handle(builder).await?;
    let message = reply_handle.message().await?;
    let message_id = C::ReplyHandle::message_id(&message);

    // Make sure the in_progress status gets updated even on failure
    in_progress.store(true, AtomicOrdering::Release);
    let Some((interaction, opponent)) =
        find_opponent(&ctx, message_id, challenger.id.get(), in_progress).await
    else {
        let duel_timeout_msg = format!("{challenger} failed to find someone to duel.");
        in_progress.store(false, AtomicOrdering::Release);
        return reply_handle
            .edit(ctx, reply_with_buttons(duel_timeout_msg, vec![]))
            .await;
    };

    if let Err(e) = finish_duel_in_progress(ctx, interaction, challenger, opponent).await {
        eprintln!("Failed to run duel to completion: {e:?}");
    };

    in_progress.store(false, AtomicOrdering::Release);

    Ok(())
}

async fn find_challenger(ctx: &impl CoreContext, in_progress: &AtomicBool) -> Result<DuelUser> {
    let author = ctx.author();
    let challenger = DuelUser::from(ctx.user_id(author), ctx.user_name(author).await);

    if in_progress.load(AtomicOrdering::Acquire) {
        bail!("a duel is already in progress");
    }

    let mut conn = ctx
        .acquire_db_connection()
        .await
        .context("There was a problem with the database, sorry!")?;
    if let Err(e) = challenger.ensure_outside_cooldown(&mut conn).await {
        bail!(e.to_string());
    }

    Ok(challenger)
}

async fn find_opponent<C: CoreContext>(
    ctx: &C,
    message_id: MessageId,
    challenger_id: u64,
    in_progress: &AtomicBool,
) -> Option<(C::Interaction, DuelUser)> {
    let collector = &mut ctx
        .create_collector()
        .message_id(message_id)
        .filter(Box::new(|f: &C::Interaction| f.custom_id() == "duel-btn"))
        .timeout(DEAD_DUEL_COOLDOWN)
        .stream();

    while let Some(interaction) = collector.next().await {
        // NOTE: responding with an ephemeral message does not trigger the
        // `iteraction failed` error but I'd like to find a way to just ignore
        // the click entirely with no response.

        let author = interaction.author();
        let author_id = ctx.user_id(author);
        if author_id == challenger_id {
            let resp = response(ephemeral_text_message("You cannot join your own duel."));
            ctx.respond(interaction, resp).await.ok()?;
            continue;
        }

        if !in_progress.load(AtomicOrdering::Acquire) {
            let resp = response(ephemeral_text_message(
                "Someone beat you to the challenge already",
            ));
            if let Err(e) = ctx.respond(interaction, resp).await {
                eprintln!("Failed to report duel was in progress, {e:?}");
            };
            continue;
        }

        let author = interaction.author();
        let accepter = DuelUser::from(ctx.user_id(author), ctx.user_name(author).await);
        let Ok(mut conn) = ctx.acquire_db_connection().await else {
            eprintln!("Failed to acquire database connection");
            continue;
        };

        if let Err(e) = accepter.ensure_outside_cooldown(&mut conn).await {
            let resp = response(ephemeral_text_message(e.to_string()));
            if let Err(ie) = ctx.respond(interaction, resp).await {
                eprintln!("Failed to report accepter was outside cooldown, {ie:?}");
            }
            continue;
        }

        return Some((interaction, accepter));
    }

    None
}

async fn finish_duel_in_progress<C: CoreContext>(
    ctx: C,
    interaction: C::Interaction,
    challenger: DuelUser,
    accepter: DuelUser,
) -> Result<()> {
    let mut conn = ctx
        .acquire_db_connection()
        .await
        .context("Failed to acquire database connection")?;
    let mut transaction = conn.begin().await.context("Failed to begin transaction")?;

    let (challenger_score, accepter_score) = pick_scores();

    let winner_text = match challenger_score.cmp(&accepter_score) {
        Ordering::Greater => {
            let (winner_id, loser_id) = (&challenger.string_id, &accepter.string_id);
            update_users_win_loss(&mut transaction, winner_id, loser_id)
                .await
                .context("Failed to update users win/loss")?;

            format!("{} has won!", Mention::User(challenger.id))
        }
        Ordering::Less => {
            let (winner_id, loser_id) = (&accepter.string_id, &challenger.string_id);
            update_users_win_loss(&mut transaction, winner_id, loser_id)
                .await
                .context("Failed to update users win/loss")?;

            format!("{} has won!", Mention::User(accepter.id))
        }
        Ordering::Equal => {
            update_users_drawn(&mut transaction, &challenger.string_id, &accepter.string_id)
                .await
                .context("Failed to update users draws")?;

            let timeout_end_time =
                Utc::now() + chrono::Duration::from_std(TIMEOUT_DURATION).unwrap();
            let challenger_member = ctx.author_member().await;
            let accepter_member = ctx.user_member(accepter.id).await;

            ctx.timeout(challenger_member, timeout_end_time).await;
            ctx.timeout(accepter_member, timeout_end_time).await;

            "It's a draw! Now go sit in a corner for 10 minutes and think about your actions..."
                .into()
        }
    };

    let final_message = format!(
        "{} has rolled a {accepter_score} and {} has rolled a {challenger_score}. {winner_text}",
        Mention::User(accepter.id),
        Mention::User(challenger.id)
    );
    let update_resp = update_response(text_message(final_message).components(Vec::new()));
    ctx.respond(interaction, update_resp)
        .await
        .context("Failed to respond to duel end")?;

    Ok(())
}

/// Display your duel statistics
#[poise::command(slash_command)]
pub async fn duelstats(ctx: DiscordContext<'_>) -> Result<()> {
    duelstats_impl(ctx).await
}
async fn duelstats_impl(ctx: impl CoreContext) -> Result<()> {
    let user = ctx.author();
    let conn = &mut ctx.acquire_db_connection().await?;
    let user_id = ctx.user_id(user);
    let Some(stats) = DuelStats::from_database(conn, user_id).await? else {
        return ctx.bail("You have never dueled before.").await;
    };

    let user_name = ctx.user_name(user).await;
    let user_colour = ctx.user_colour(user).await;
    let user_avatar_url = ctx.user_avatar_url(user);
    let embed = stats.embed(&user_name, user_colour, &user_avatar_url);

    ctx.reply(CreateReply::default().embed(embed)).await
}

async fn get_last_loss(executor: impl SqliteExecutor<'_>, user_id: &str) -> Result<NaiveDateTime> {
    // Insert a new User so that DuelStats always has a user to reference when
    // we set the wins/losses/draws after the duel
    let row = sqlx::query!(
        r#"
        INSERT OR IGNORE INTO User (id) VALUES (?);
        SELECT last_loss From User WHERE id = ?
        "#,
        user_id,
        user_id
    )
    .fetch_one(executor)
    .await
    .with_context(|| format!("Failed to get {user_id}'s last loss"))?;

    Ok(row.last_loss)
}

async fn update_users_win_loss(
    executor: &mut Transaction<'_, sqlx::Sqlite>,
    winner_id: &str,
    loser_id: &str,
) -> Result<()> {
    sqlx::query!(
        r#"INSERT INTO DuelStats (user_id, wins, win_streak, win_streak_max)
        VALUES (?, 1, 1, 1)
        ON CONFLICT(user_id) DO UPDATE SET
            wins = wins + 1,
            win_streak = win_streak + 1,
            win_streak_max = MAX(win_streak_max, win_streak + 1),
            loss_streak = 0;

        INSERT INTO DuelStats (user_id, losses, loss_streak, loss_streak_max)
        VALUES (?, 1, 1, 1)
        ON CONFLICT(user_id) DO UPDATE SET
            losses = losses + 1,
            loss_streak = loss_streak + 1,
            loss_streak_max = MAX(loss_streak_max, loss_streak + 1),
            win_streak = 0;"#,
        winner_id,
        loser_id
    )
    .execute(&mut *executor)
    .await
    .with_context(|| format!("Failed to update {winner_id} and/or {loser_id}'s wins/losses"))?;

    Ok(())
}

async fn update_users_drawn(
    executor: &mut Transaction<'_, sqlx::Sqlite>,
    challenger_id: &str,
    accepter_id: &str,
) -> Result<()> {
    sqlx::query!(
        r#"INSERT INTO DuelStats (user_id, draws) VALUES (?, 1), (?, 1)
        ON CONFLICT(user_id)
        DO UPDATE SET draws = draws + 1, win_streak = 0, loss_streak = 0;"#,
        challenger_id,
        accepter_id
    )
    .execute(&mut *executor)
    .await
    .with_context(|| format!("Failed to update {challenger_id} and {accepter_id}'s draws"))?;

    Ok(())
}

#[derive(Debug, Default)]
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
    async fn from_database(
        executor: impl SqliteExecutor<'_>,
        user_id: UserId,
    ) -> Result<Option<Self>> {
        let user_id = user_id.to_string();
        let stats = sqlx::query_as!(
            DuelStats,
            r#"SELECT * FROM DuelStats WHERE user_id = ?"#,
            user_id
        )
        .fetch_optional(executor)
        .await
        .with_context(|| format!("Failed to get {user_id}'s duel stats"))?;

        Ok(stats)
    }

    fn current_streak(&self) -> String {
        match (self.win_streak, self.loss_streak, self.draws) {
            (0, 0, 0) => "You have never dueled before".to_string(),
            (0, 0, _) => "Your last duel was a draw".to_string(),
            (0, _, _) => format!("Current streak **{} losses**", self.loss_streak),
            (_, 0, _) => format!("Current streak **{} wins**", self.win_streak),
            _ => unreachable!(),
        }
    }

    fn embed(&self, user_name: &str, user_colour: Option<Colour>, avatar_url: &str) -> CreateEmbed {
        CreateEmbed::default()
            .colour(user_colour.unwrap_or_else(|| 0x77618F.into()))
            .description(format!(
                "{}\nBest streak: **{} wins**\nWorst streak: **{} losses**",
                self.current_streak(),
                self.win_streak_max,
                self.loss_streak_max
            ))
            .author(
                CreateEmbedAuthor::new(format!(
                    "{user_name}'s scoresheet: {}-{}-{}",
                    self.wins, self.losses, self.draws
                ))
                .icon_url(avatar_url),
            )
    }
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
    fn from(user_id: UserId, user_name: impl Into<String>) -> Self {
        Self {
            id: user_id,
            string_id: user_id.to_string(),
            name: user_name.into(),
        }
    }

    async fn ensure_outside_cooldown(&self, conn: impl SqliteExecutor<'_>) -> Result<()> {
        let last_loss = match get_last_loss(conn, &self.string_id).await {
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

#[cfg(test)]
mod tests {
    use serenity::all::Colour;

    use super::*;

    #[tokio::test]
    async fn duelstats_embed_zeroed_stats() {
        let stats = DuelStats::default();

        insta::assert_debug_snapshot!(stats.embed(
            "cool_user",
            Some(Colour(0x00FF00)),
            "https:://google.com",
        ))
    }
}
