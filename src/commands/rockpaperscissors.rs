use std::{str::FromStr, sync::Arc, time::Duration};

use crate::{
    common::{
        ephemeral_interaction_response, send_interaction_update, send_message_with_row, Score,
    },
    Context,
};
use anyhow::{bail, Result};
use poise::serenity_prelude::{ButtonStyle, MessageComponentInteraction, ReactionType};
use serenity::{builder::CreateActionRow, collector::CollectComponentInteraction};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Weapon {
    Rock,
    Paper,
    Scissors,
}

impl Weapon {
    fn compare(self, other: Weapon) -> Score {
        use Weapon::*;
        match (self, other) {
            (Rock, Paper) | (Paper, Scissors) | (Scissors, Rock) => Score::Loss,
            (Paper, Rock) | (Scissors, Paper) | (Rock, Scissors) => Score::Win,
            _ => Score::Draw,
        }
    }

    fn to_str(self) -> &'static str {
        match self {
            Weapon::Rock => "rock",
            Weapon::Paper => "paper",
            Weapon::Scissors => "scissors",
        }
    }
}

impl FromStr for Weapon {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::prelude::v1::Result<Self, Self::Err> {
        let choice = match s {
            ROCK_BTN => Self::Rock,
            PAPER_BTN => Self::Paper,
            SCISSORS_BTN => Self::Scissors,
            _ => bail!("Invalid weapon choice"),
        };
        Ok(choice)
    }
}

const ACCEPT_BTN: &str = "rps-accept";
const ROCK_BTN: &str = "rps-rock";
const PAPER_BTN: &str = "rps-paper";
const SCISSORS_BTN: &str = "rps-scissors";

const ACCEPT_TIMEOUT: Duration = Duration::from_secs(600);
const CHOICE_TIMEOUT: Duration = Duration::from_secs(300);

/// Challenge someone to a rock paper scissors battle
#[poise::command(slash_command)]
pub async fn rps(ctx: Context<'_>) -> Result<()> {
    let challenger = ctx.author();
    let initial_msg = format!("{challenger} is looking for a rock-paper-scissors opponent!");
    let first_message = send_message_with_row(ctx, initial_msg, create_accept_button()).await?;
    let message_id = first_message.message().await?.id;

    let Some(interaction) = find_opponent(ctx, message_id.0, challenger.id.0).await else {
        let timeout_message = format!("Nobody was brave enough to challenge {challenger}");
        first_message
            .edit(ctx, |m| m.content(timeout_message).components(|c| c))
            .await?;

        return Ok(());
    };

    let accepter = &interaction.user;
    let weapon_request = "Choose your weapon!";
    let row = create_weapons_buttons();

    let (challenger_msg, _) = tokio::try_join!(
        ctx.send(|f| {
            f.content(weapon_request)
                .ephemeral(true)
                .components(|c| c.set_action_row(row.clone()))
        }),
        interaction.create_interaction_response(ctx, |r| {
            r.interaction_response_data(|d| {
                d.content(weapon_request)
                    .ephemeral(true)
                    .components(|c| c.set_action_row(row.clone()))
            })
        }),
    )?;

    let (challenger_msg, accepter_msg) = tokio::try_join!(
        challenger_msg.message(),
        interaction.get_interaction_response(ctx)
    )?;

    let (Some(challenger_choice), Some(accepter_choice)) = tokio::try_join!(
        get_user_weapon_choice(ctx, challenger_msg.id.0, challenger.id.0),
        get_user_weapon_choice(ctx, accepter_msg.id.0, accepter.id.0)
    )?
    else {
        let msg = "Someone didn't pick their weapon in time :(";
        first_message.edit(ctx, |m| m.content(msg)).await?;
        return Ok(());
    };

    let mut end_msg = format!(
        "{challenger} picks {}, {accepter} picks {}\n",
        challenger_choice.to_str(),
        accepter_choice.to_str()
    );
    end_msg.push_str(&match challenger_choice.compare(accepter_choice) {
        Score::Win => format!("{challenger} wins!"),
        Score::Loss => format!("{accepter} wins!"),
        Score::Draw => "It's a draw!".to_owned(),
    });

    first_message
        .edit(ctx, |m| m.content(end_msg).components(|c| c))
        .await?;

    Ok(())
}

async fn find_opponent(
    ctx: Context<'_>,
    message_id: u64,
    challenger_id: u64,
) -> Option<Arc<MessageComponentInteraction>> {
    while let Some(interaction) = CollectComponentInteraction::new(ctx)
        .timeout(ACCEPT_TIMEOUT)
        .message_id(message_id)
        .filter(move |f| f.data.custom_id == ACCEPT_BTN)
        .await
    {
        if interaction.user.id == challenger_id {
            ephemeral_interaction_response(&ctx, interaction, "You cannot fight yourself.")
                .await
                .ok()?;
            continue;
        }

        return Some(interaction);
    }

    None
}

async fn get_user_weapon_choice(
    ctx: Context<'_>,
    message_id: u64,
    author_id: u64,
) -> Result<Option<Weapon>> {
    let weapon_button_interaction = CollectComponentInteraction::new(ctx)
        .message_id(message_id)
        .timeout(CHOICE_TIMEOUT)
        .collect_limit(1)
        .filter(move |f| {
            f.user.id.0 == author_id
                && [ROCK_BTN, PAPER_BTN, SCISSORS_BTN].contains(&f.data.custom_id.as_str())
        })
        .await;

    let Some(weapon_button_interaction) = weapon_button_interaction else {
        // Collector timed out
        return Ok(None);
    };

    send_interaction_update(ctx, &weapon_button_interaction, "Great choice!").await?;
    let weapon = Weapon::from_str(&weapon_button_interaction.data.custom_id)?;

    Ok(Some(weapon))
}

fn create_accept_button() -> CreateActionRow {
    let mut row = CreateActionRow::default();
    row.create_button(|f| {
        f.custom_id(ACCEPT_BTN)
            .emoji('💪')
            .label("Accept Battle".to_string())
            .style(ButtonStyle::Primary)
    });

    row
}

fn create_weapons_buttons() -> CreateActionRow {
    let mut row = CreateActionRow::default();
    row.create_button(|f| {
        f.custom_id(ROCK_BTN)
            .emoji('🪨')
            .label("Rock")
            .style(ButtonStyle::Primary)
    });
    row.create_button(|f| {
        f.custom_id(PAPER_BTN)
            .emoji('🧻')
            .label("Paper")
            .style(ButtonStyle::Primary)
    });
    row.create_button(|f| {
        f.custom_id(SCISSORS_BTN)
            .emoji(ReactionType::Unicode("✂️".to_owned()))
            .label("Scissors")
            .style(ButtonStyle::Primary)
    });

    row
}
