use std::time::Duration;

use anyhow::Result;
use chrono::NaiveDateTime;
use poise::serenity_prelude::{ButtonStyle, CreateActionRow};
use rand::Rng;
use sqlx::QueryBuilder;
use tokio::sync::RwLock;

use crate::{
    common::{colour, ephemeral_interaction_response, ephemeral_message, name},
    Context,
};

const LOSS_COOLDOWN: Duration = Duration::from_secs(10 * 60);
const DEAD_DUEL_COOLDOWN: Duration = Duration::from_secs(10 * 60);

#[derive(Default)]
struct DuelData {
    in_progress: bool,
}

#[poise::command(slash_command, custom_data = "RwLock::new(DuelData::default())")]
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

    let challenger_last_loss = match get_last_loss(&ctx, challenger.id.to_string()).await {
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

    let challenger_name = name(challenger, &ctx).await;

    let now = chrono::offset::Utc::now().naive_utc();
    let dead_cooldown_duration = chrono::Duration::from_std(DEAD_DUEL_COOLDOWN)?;
    if challenger_last_loss + dead_cooldown_duration > now {
        ephemeral_message(
            ctx,
            format!(
                "{} you have recently lost a duel. Please try again later.",
                challenger_name
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
        // Scope to drop the handle to the lock
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
        // `iteraction failed` error but I'll like to find a way to just ignore
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

        let accepter_last_loss = get_last_loss(&ctx, accepter.id.to_string()).await?;
        let loss_cooldown_duration: chrono::Duration = chrono::Duration::from_std(LOSS_COOLDOWN)?;
        let now = chrono::offset::Utc::now().naive_utc();

        if accepter_last_loss + loss_cooldown_duration > now {
            let content = format!(
                "{} you have recently lost a duel. Please try again later.",
                accepter_name
            );
            ephemeral_interaction_response(&ctx, interaction, content).await?;
            continue;
        }

        let challeger_score;
        let accepter_score;
        {
            let mut rng = rand::thread_rng();
            challeger_score = rng.gen_range(0..=100);
            accepter_score = rng.gen_range(0..=100);
        }

        let winner_text = if challeger_score > accepter_score {
            update_user_score(&ctx, challenger.id.to_string(), Score::Win).await?;
            update_user_score(&ctx, accepter.id.to_string(), Score::Loss).await?;
            update_last_loss(&ctx, accepter.id.to_string()).await?;

            format!("{challenger_name} has won!")
        } else if accepter_score > challeger_score {
            update_user_score(&ctx, accepter.id.to_string(), Score::Win).await?;
            update_user_score(&ctx, challenger.id.to_string(), Score::Loss).await?;
            update_last_loss(&ctx, challenger.id.to_string()).await?;

            format!("{accepter_name} has won!")
        } else {
            update_user_score(&ctx, challenger.id.to_string(), Score::Draw).await?;
            update_user_score(&ctx, accepter.id.to_string(), Score::Draw).await?;

            "It's a draw! Now go sit in a corner for 10 mintues and think about your actions..."
                .into()
        };

        accept_reply
            .edit(ctx, |f| f.content(winner_text).components(|c| c))
            .await?;

        let mut duel_data = custom_data_lock.write().await;
        duel_data.in_progress = false;

        return Ok(());
    }

    accept_reply
        .edit(ctx, |f| {
            f.content(format!("{challenger_name} failed to find someone to duel."))
                // no components
                .components(|c| c)
        })
        .await?;
    let mut duel_data = custom_data_lock.write().await;
    duel_data.in_progress = false;

    return Ok(());
}

/// Display your duel statistics
#[poise::command(slash_command)]
pub async fn duelstats(ctx: Context<'_>) -> Result<()> {
    let user = ctx.author();
    let stats = get_duel_stats(&ctx, user.id.to_string()).await?;

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
    let colour = colour(&ctx).await.unwrap_or(0x77618F.into());

    ctx.send(|r| {
        r.embed(|e| {
            e.colour(colour)
                .description(format!("{current_streak}\n{best_streak}\n{worst_streak}"))
                .author(|a| {
                    a.icon_url(user.avatar_url().unwrap_or("".into()))
                        .name(format!(
                            "{name}'s scoresheet: {}-{}-{}",
                            stats.wins, stats.losses, stats.draws
                        ))
                })
        })
    })
    .await?;

    return Ok(());
}

async fn get_last_loss(ctx: &Context<'_>, user_id: String) -> Result<NaiveDateTime> {
    let row = sqlx::query!(
        r#"
        INSERT OR IGNORE INTO User (id) VALUES (?);
        SELECT last_loss From User WHERE id = ?
        "#,
        user_id,
        user_id
    )
    .fetch_one(&ctx.data().database)
    .await?;

    return Ok(row.last_loss);
}

async fn update_last_loss(ctx: &Context<'_>, user_id: String) -> Result<()> {
    sqlx::query!(
        "UPDATE User SET last_loss = datetime('now') WHERE id = ?",
        user_id
    )
    .execute(&ctx.data().database)
    .await?;

    return Ok(());
}

enum Score {
    Win,
    Loss,
    Draw,
}

async fn update_user_score(ctx: &Context<'_>, user_id: String, score: Score) -> Result<()> {
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
    query.build().execute(&ctx.data().database).await?;

    return Ok(());
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

async fn get_duel_stats(ctx: &Context<'_>, user_id: String) -> Result<Option<DuelStats>> {
    let stats = sqlx::query_as!(
        DuelStats,
        r#"SELECT * FROM Duels WHERE user_id = ?"#,
        user_id
    )
    .fetch_optional(&ctx.data().database)
    .await?;

    return Ok(stats);
}
