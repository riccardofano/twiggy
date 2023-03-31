use std::time::Duration;

use anyhow::Result;
use chrono::NaiveDateTime;
use poise::serenity_prelude::{ButtonStyle, CreateActionRow, User};
use rand::Rng;
use sqlx::QueryBuilder;
use tokio::sync::RwLock;

use crate::Context;

// const LOSS_COOLDOWN: Duration = Duration::from_secs(10 * 60);
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
        ctx.send(|f| f.content("A duel is already in progress").ephemeral(true))
            .await?;
        return Ok(());
    }

    {
        // Scope to drop the handle to the lock
        let mut duel_data = custom_data_lock.write().await;
        duel_data.in_progress = true;
    }

    let challenger_last_loss = match get_last_loss(&ctx, challenger.id.to_string()).await {
        Ok(last_loss) => last_loss,
        Err(e) => {
            eprintln!(
                "Could not retrieve last loss of {} - {:?}",
                challenger.name, e
            );
            ctx.send(|f| {
                f.content("Something went wrong when trying to join the duel")
                    .ephemeral(true)
            })
            .await?;
            return Ok(());
        }
    };

    let now = chrono::offset::Utc::now().naive_utc();
    let dead_cooldown_duration = chrono::Duration::from_std(DEAD_DUEL_COOLDOWN)?;
    if challenger_last_loss + dead_cooldown_duration > now {
        ctx.send(|f| {
            f.content("{} you have recently lost a duel. Please try again later.")
                .ephemeral(true)
        })
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

    let challenger_name = name(challenger, &ctx).await;
    let challenger_id = challenger.id.clone();
    let accept_reply = ctx
        .send(|r| {
            r.content(format!(
                "{challenger_name} is looking for a duel, press the button to accept."
            ))
            .components(|c| c.add_action_row(row))
        })
        .await?;

    while let Some(interaction) = accept_reply
        .message()
        .await?
        .await_component_interaction(ctx)
        // NOTE: trying to join your own duel ends in an `iteraction failed`
        // something that doesn't happen in the ts version.
        // checking in filter or inside the interaction doesn't seem to be any different
        .filter(move |i| i.user.id != challenger_id)
        .timeout(DEAD_DUEL_COOLDOWN)
        .await
    {
        if interaction.data.custom_id != "duel-btn" {
            continue;
        }

        if !custom_data_lock.read().await.in_progress {
            ctx.send(|f| {
                f.content("Someone beat you to the challenge already")
                    .ephemeral(true)
            })
            .await?;
            continue;
        }

        let accepter = &interaction.user;
        let accepter_last_loss = get_last_loss(&ctx, accepter.id.to_string()).await?;
        if accepter_last_loss + dead_cooldown_duration > chrono::offset::Utc::now().naive_utc() {
            ctx.send(|f| {
                f.content("{} you have recently lost a duel. Please try again later.")
                    .ephemeral(true)
            })
            .await?;
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
            format!("{challenger_name} has won!")
        } else if accepter_score > challeger_score {
            update_user_score(&ctx, accepter.id.to_string(), Score::Win).await?;
            update_user_score(&ctx, challenger.id.to_string(), Score::Loss).await?;
            let name = &nickname(accepter, &ctx)
                .await
                .unwrap_or(accepter.name.clone());
            format!("{name} has won!")
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

    Ok(())
}

async fn nickname(person: &User, ctx: &Context<'_>) -> Option<String> {
    let guild_id = ctx.guild_id()?;
    return person.nick_in(ctx, guild_id).await;
}

async fn name(person: &User, ctx: &Context<'_>) -> String {
    nickname(person, ctx).await.unwrap_or(person.name.clone())
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

    Ok(row.last_loss)
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

    Ok(())
}
