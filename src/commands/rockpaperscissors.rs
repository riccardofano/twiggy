use std::{str::FromStr, time::Duration};

use crate::{
    common::{
        ephemeral_text_message, message_with_buttons, reply_with_buttons, response,
        update_response, Score,
    },
    Context,
};
use anyhow::{bail, Result};
use poise::serenity_prelude::{
    ButtonStyle, ComponentInteraction, ComponentInteractionCollector, CreateActionRow,
    CreateButton, MessageId, ReactionType,
};

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
    let first_message = ctx
        .send(reply_with_buttons(
            initial_msg,
            vec![create_accept_button()],
        ))
        .await?;
    let message_id = first_message.message().await?.id;

    let Some(interaction) = find_opponent(ctx, message_id, challenger.id.get()).await else {
        let timeout_message = format!("Nobody was brave enough to challenge {challenger}");
        first_message
            .edit(ctx, reply_with_buttons(timeout_message, Vec::new()))
            .await?;

        return Ok(());
    };

    let accepter = &interaction.user;
    let weapon_request = "Choose your weapon!";
    let row = create_weapons_buttons();

    let (challenger_msg, _) = tokio::try_join!(
        ctx.send(reply_with_buttons(weapon_request, vec![row.clone()]).ephemeral(true)),
        interaction.create_response(
            ctx,
            response(message_with_buttons(weapon_request, vec![row]).ephemeral(true))
        )
    )?;

    let (challenger_msg, accepter_msg) =
        tokio::try_join!(challenger_msg.message(), interaction.get_response(ctx))?;

    let (Some(challenger_choice), Some(accepter_choice)) = tokio::try_join!(
        get_user_weapon_choice(ctx, challenger_msg.id, challenger.id.get()),
        get_user_weapon_choice(ctx, accepter_msg.id, accepter.id.get())
    )?
    else {
        let reply = reply_with_buttons("Someone didn't pick their weapon in time :(", Vec::new());
        first_message.edit(ctx, reply).await?;
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

    let reply = reply_with_buttons(end_msg, Vec::new());
    first_message.edit(ctx, reply).await?;

    Ok(())
}

async fn find_opponent(
    ctx: Context<'_>,
    message_id: MessageId,
    challenger_id: u64,
) -> Option<ComponentInteraction> {
    while let Some(interaction) = ComponentInteractionCollector::new(ctx)
        .timeout(ACCEPT_TIMEOUT)
        .message_id(message_id)
        .filter(move |f| f.data.custom_id == ACCEPT_BTN)
        .await
    {
        if interaction.user.id == challenger_id {
            let resp = response(ephemeral_text_message("You cannot fight yourself."));
            interaction.create_response(ctx, resp).await.ok()?;
            continue;
        }

        return Some(interaction);
    }

    None
}

async fn get_user_weapon_choice(
    ctx: Context<'_>,
    message_id: MessageId,
    author_id: u64,
) -> Result<Option<Weapon>> {
    let weapon_button_interaction = ComponentInteractionCollector::new(ctx)
        .message_id(message_id)
        .timeout(CHOICE_TIMEOUT)
        .filter(move |f| {
            f.user.id.get() == author_id
                && [ROCK_BTN, PAPER_BTN, SCISSORS_BTN].contains(&f.data.custom_id.as_str())
        })
        .await;

    let Some(weapon_button_interaction) = weapon_button_interaction else {
        // Collector timed out
        return Ok(None);
    };

    let update_resp = update_response(ephemeral_text_message("Great choice!"));
    weapon_button_interaction
        .create_response(ctx, update_resp)
        .await?;
    let weapon = Weapon::from_str(&weapon_button_interaction.data.custom_id)?;

    Ok(Some(weapon))
}

fn create_accept_button() -> CreateActionRow {
    let accept_btn = CreateButton::new(ACCEPT_BTN)
        .emoji('💪')
        .label("Accept Battle".to_string())
        .style(ButtonStyle::Primary);

    CreateActionRow::Buttons(vec![accept_btn])
}

fn create_weapons_buttons() -> CreateActionRow {
    let rock = CreateButton::new(ROCK_BTN)
        .emoji('🪨')
        .label("Rock")
        .style(ButtonStyle::Primary);
    let paper = CreateButton::new(PAPER_BTN)
        .emoji('🧻')
        .label("Paper")
        .style(ButtonStyle::Primary);
    let scissors = CreateButton::new(SCISSORS_BTN)
        .emoji(ReactionType::Unicode("✂️".to_owned()))
        .label("Scissors")
        .style(ButtonStyle::Primary);

    CreateActionRow::Buttons(vec![rock, paper, scissors])
}
