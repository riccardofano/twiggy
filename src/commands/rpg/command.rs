use std::time::Duration;

use crate::{
    commands::rpg::fight::{FightResult, RPGFight},
    common::{ephemeral_interaction_response, ephemeral_message, nickname},
    Context, Data,
};
use anyhow::Result;
use poise::serenity_prelude::{ButtonStyle, CreateActionRow};
use poise::{
    futures_util::StreamExt,
    serenity_prelude::{self as serenity, ComponentInteractionCollectorBuilder},
};
use tokio::sync::RwLock;

use super::character::Character;

const DEAD_DUEL_COOLDOWN: Duration = Duration::from_secs(5 * 60);

#[poise::command(slash_command, guild_only, subcommands("challenge"))]
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
        .expect("Expected to have passed a DuelData struct as custom_data");

    if custom_data_lock.read().await.in_progress {
        ephemeral_message(ctx, "A RPG duel is already in progress").await?;
        return Ok(());
    }

    let challenger = ctx.author();
    let challenger_nick = nickname(challenger, &ctx).await;
    let challenger_name = challenger_nick.as_deref().unwrap_or(&challenger.name);

    let challenger_character = Character::new(
        challenger.id.0,
        challenger_name,
        &challenger_nick.as_deref(),
    );

    let mut row = CreateActionRow::default();
    row.create_button(|f| {
        f.custom_id("duel-btn")
            .emoji('⚔')
            .label("Accept Duel".to_string())
            .style(ButtonStyle::Primary)
    });

    let accept_reply = ctx
        .send(|r| {
            r.content(format!(
                "{challenger_name} is throwing down the gauntlet in challenge.."
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
        if interaction.data.custom_id != "duel-btn" {
            continue;
        }
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
        let accepter_nick = nickname(accepter, &ctx).await;
        let accepter_name = accepter_nick.as_deref().unwrap_or(&challenger.name);
        let accepter_character =
            Character::new(accepter.id.0, accepter_name, &accepter_nick.as_deref());

        let mut fight = RPGFight::new(challenger_character, accepter_character);

        // TODO: update db with fight results
        match fight.fight() {
            FightResult::ChallengerWin => {}
            FightResult::AccepterWin => {}
            FightResult::Draw => {}
        };

        let mut summary_row = CreateActionRow::default();
        summary_row.create_button(|f| {
            f.custom_id("rpg-summary")
                .emoji('📖')
                .label("See summary".to_string())
                .style(ButtonStyle::Secondary)
        });

        interaction
            .create_interaction_response(ctx, |r| {
                r.kind(serenity::InteractionResponseType::UpdateMessage)
                    .interaction_response_data(|d| {
                        d.content(fight.summary())
                            .components(|c| c.set_action_row(summary_row))
                    })
            })
            .await?;

        let mut cmd_data = custom_data_lock.write().await;
        cmd_data.in_progress = false;

        ctx.data()
            .rpg_summary_cache
            .lock()
            .await
            .insert(reply_msg.id.0, fight.to_string());

        return Ok(());
    }

    accept_reply
        .edit(ctx, |r| {
            r.content(format!("{challenger_name} failed to find someone to duel."))
                .components(|c| c)
        })
        .await?;

    let mut data = custom_data_lock.write().await;
    data.in_progress = false;

    Ok(())
}

pub async fn setup_rpg_summary(ctx: &serenity::Context, user_data: &Data) -> Result<()> {
    let collector = ComponentInteractionCollectorBuilder::new(ctx)
        .filter(|f| f.data.custom_id == "rpg-summary")
        .build();

    let _: Vec<_> = collector
        .then(|interaction| async move {
            let data = user_data.rpg_summary_cache.lock().await;
            let summary = data.get(&interaction.message.id.0);

            let response = match summary {
                Some(r) => r,
                None => "Could not find the summary for that fight.",
            };

            let _result = interaction
                .create_interaction_response(&ctx, |r| {
                    r.interaction_response_data(|d| d.content(response).ephemeral(true))
                })
                .await;
            interaction
        })
        .collect()
        .await;

    Ok(())
}
