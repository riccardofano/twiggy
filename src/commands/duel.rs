use crate::common::{
    colour, ephemeral_interaction_response, ephemeral_message, member, name, Score,
};
use crate::Context;

use anyhow::Result;
use chrono::{NaiveDateTime, Utc};
use poise::serenity_prelude::{ButtonStyle, CreateActionRow};
use rand::Rng;
use sqlx::{Connection, QueryBuilder, SqliteConnection};
use std::cmp::Ordering;
use std::time::Duration;
use tokio::sync::RwLock;

const DEAD_DUEL_COOLDOWN: Duration = Duration::from_secs(5 * 60);
const LOSS_COOLDOWN: Duration = Duration::from_secs(10 * 60);
const TIMEOUT_DURATION: Duration = Duration::from_secs(10 * 60);

#[derive(Default)]
struct DuelData {
    in_progress: bool,
}

#[poise::command(
    slash_command,
    guild_only,
    custom_data = "RwLock::new(DuelData::default())"
)]
pub async fn duel(ctx: Context<'_>) -> Result<()> {
    let challenger = ctx.author();

    let custom_data_lock = ctx
        .command()
        .custom_data
        .downcast_ref::<RwLock<DuelData>>()
        .expect("Expected to have passed a DuelData struct as custom_data");

    if custom_data_lock.read().await.in_progress {
        ephemeral_message(ctx, "A duel is already in progress").await?;
        return Ok(());
    }

    let mut conn = ctx.data().database.acquire().await?;
    let challenger_last_loss = match get_last_loss(&mut conn, challenger.id.to_string()).await {
        Ok(last_loss) => last_loss,
        Err(e) => {
            eprintln!(
                "Could not retrieve last loss of {} - {:?}",
                challenger.name, e
            );
            ephemeral_message(ctx, "Something went wrong when trying to join the duel.").await?;
            return Ok(());
        }
    };
    // NOTE: Manually drop the connection otherwise the connection would be held
    // for the entirety of the duel duration. Which meant that if, for example,
    // removing the `duel_in_progress` check, I ran 5 duels at the same time
    // (the max number of connections in the pool) the bot would stop responding on the 6th one
    drop(conn);

    let challenger_name = name(challenger, &ctx).await;

    let now = Utc::now().naive_utc();
    let loss_cooldown_duration = chrono::Duration::from_std(LOSS_COOLDOWN)?;
    if challenger_last_loss + loss_cooldown_duration > now {
        let time_until_duel = (challenger_last_loss + loss_cooldown_duration).timestamp();
        ephemeral_message(
            ctx,
            format!(
                "{} you have recently lost a duel. Please try again <t:{}:R>.",
                challenger_name, time_until_duel
            ),
        )
        .await?;
        return Ok(());
    }

    let mut row = CreateActionRow::default();
    row.create_button(|f| {
        f.custom_id("duel-btn")
            .emoji('🎲')
            .label("Accept Duel".to_string())
            .style(ButtonStyle::Primary)
    });

    let accept_reply = ctx
        .send(|r| {
            r.content(format!(
                "{challenger_name} is looking for a duel, press the button to accept."
            ))
            .components(|c| c.add_action_row(row))
        })
        .await?;

    {
        // NOTE: Scope to drop the handle to the lock
        let mut duel_data = custom_data_lock.write().await;
        duel_data.in_progress = true;
    }

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

        let accepter = &interaction.user;
        let accepter_name = &name(accepter, &ctx).await;

        let mut conn = ctx.data().database.acquire().await?;
        let accepter_last_loss = get_last_loss(&mut conn, accepter.id.to_string()).await?;
        drop(conn);

        let now = Utc::now().naive_utc();
        if accepter_last_loss + loss_cooldown_duration > now {
            let time_until_duel = (accepter_last_loss + loss_cooldown_duration).timestamp();
            let content = format!(
                "{} you have recently lost a duel. Please try again <t:{}:R>.",
                accepter_name, time_until_duel
            );
            ephemeral_interaction_response(&ctx, interaction, content).await?;
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
                update_user_score(&mut transaction, challenger.id.to_string(), Score::Win).await?;
                update_user_score(&mut transaction, accepter.id.to_string(), Score::Loss).await?;
                update_last_loss(&mut transaction, accepter.id.to_string()).await?;

                format!("{challenger_name} has won!")
            }
            Ordering::Less => {
                update_user_score(&mut transaction, accepter.id.to_string(), Score::Win).await?;
                update_user_score(&mut transaction, challenger.id.to_string(), Score::Loss).await?;
                update_last_loss(&mut transaction, challenger.id.to_string()).await?;

                format!("{accepter_name} has won!")
            }
            Ordering::Equal => {
                update_user_score(&mut transaction, challenger.id.to_string(), Score::Draw).await?;
                update_user_score(&mut transaction, accepter.id.to_string(), Score::Draw).await?;

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

        interaction.create_interaction_response(ctx, |r| {
            r.kind(poise::serenity_prelude::InteractionResponseType::UpdateMessage)
            .interaction_response_data(|d| d.content(
                format!("{accepter_name} has rolled a {accepter_score} and {challenger_name} has rolled a {challenger_score}. {winner_text}")
            ).components(|c| c))
        }).await?;

        let mut duel_data = custom_data_lock.write().await;
        duel_data.in_progress = false;

        return Ok(());
    }

    accept_reply
        .edit(ctx, |f| {
            f.content(format!("{challenger_name} failed to find someone to duel."))
                // NOTE: this is how you remove components
                .components(|c| c)
        })
        .await?;
    let mut duel_data = custom_data_lock.write().await;
    duel_data.in_progress = false;

    Ok(())
}

/// Display your duel statistics
#[poise::command(slash_command)]
pub async fn duelstats(ctx: Context<'_>) -> Result<()> {
    let user = ctx.author();
    let conn = &mut ctx.data().database.acquire().await?;
    let stats = get_duel_stats(conn, user.id.to_string()).await?;

    if stats.is_none() {
        ephemeral_message(ctx, "You have never dueled before.").await?;
        return Ok(());
    }

    let stats = stats.unwrap();
    let current_streak = match (stats.win_streak, stats.loss_streak, stats.draws) {
        (0, 0, 0) => "You have never dueled before".to_string(),
        (0, 0, _) => "Your last duel was a draw".to_string(),
        (0, _, _) => format!("Current streak **{} losses**", stats.loss_streak),
        (_, 0, _) => format!("Current streak **{} wins**", stats.win_streak),
        _ => unreachable!(),
    };
    let best_streak = format!("Best streak: **{} wins**", stats.win_streak_max);
    let worst_streak = format!("Worst streak: **{} losses**", stats.loss_streak_max);

    let name = name(user, &ctx).await;
    let colour = colour(&ctx).await.unwrap_or_else(|| 0x77618F.into());

    ctx.send(|r| {
        r.embed(|e| {
            e.colour(colour)
                .description(format!("{current_streak}\n{best_streak}\n{worst_streak}"))
                .author(|a| {
                    a.icon_url(user.avatar_url().unwrap_or_else(|| "".into()))
                        .name(format!(
                            "{name}'s scoresheet: {}-{}-{}",
                            stats.wins, stats.losses, stats.draws
                        ))
                })
        })
    })
    .await?;

    Ok(())
}

async fn get_last_loss(conn: &mut SqliteConnection, user_id: String) -> Result<NaiveDateTime> {
    let row = sqlx::query!(
        r#"
        INSERT OR IGNORE INTO User (id) VALUES (?);
        SELECT last_loss From User WHERE id = ?
        "#,
        user_id,
        user_id
    )
    .fetch_one(&mut *conn)
    .await?;

    Ok(row.last_loss)
}

async fn update_last_loss(conn: &mut SqliteConnection, user_id: String) -> Result<()> {
    sqlx::query!(
        "UPDATE User SET last_loss = datetime('now') WHERE id = ?",
        user_id
    )
    .execute(&mut *conn)
    .await?;

    Ok(())
}

async fn update_user_score(
    conn: &mut SqliteConnection,
    user_id: String,
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
    query.push_bind(&user_id);
    query.push(format!(
        "); UPDATE Duels SET {update_query} WHERE user_id = "
    ));
    query.push_bind(&user_id);
    query.build().execute(&mut *conn).await?;

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

async fn get_duel_stats(conn: &mut SqliteConnection, user_id: String) -> Result<Option<DuelStats>> {
    let stats = sqlx::query_as!(
        DuelStats,
        r#"SELECT * FROM Duels WHERE user_id = ?"#,
        user_id
    )
    .fetch_optional(&mut *conn)
    .await?;

    Ok(stats)
}
