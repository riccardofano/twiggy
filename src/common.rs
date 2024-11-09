use crate::Context;

use poise::serenity_prelude::{
    CreateActionRow, CreateEmbed, CreateInteractionResponse, CreateInteractionResponseMessage, User,
};
use poise::CreateReply;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rand_seeder::Seeder;

pub fn response(message: CreateInteractionResponseMessage) -> CreateInteractionResponse {
    CreateInteractionResponse::Message(message)
}
pub fn update_response(message: CreateInteractionResponseMessage) -> CreateInteractionResponse {
    CreateInteractionResponse::UpdateMessage(message)
}

pub fn text_message(text: impl Into<String>) -> CreateInteractionResponseMessage {
    CreateInteractionResponseMessage::new().content(text)
}
pub fn ephemeral_text_message(text: impl Into<String>) -> CreateInteractionResponseMessage {
    CreateInteractionResponseMessage::new()
        .content(text)
        .ephemeral(true)
}
pub fn embed_message(embed: CreateEmbed) -> CreateInteractionResponseMessage {
    CreateInteractionResponseMessage::new().embed(embed)
}
pub fn message_with_buttons(
    text: impl Into<String>,
    rows: Vec<CreateActionRow>,
) -> CreateInteractionResponseMessage {
    CreateInteractionResponseMessage::new()
        .content(text)
        .components(rows)
}

pub fn ephemeral_reply(content: impl Into<String>) -> CreateReply {
    CreateReply::default().content(content).ephemeral(true)
}
pub fn reply_with_buttons(content: impl Into<String>, rows: Vec<CreateActionRow>) -> CreateReply {
    CreateReply::default().content(content).components(rows)
}

pub async fn bail_reply(ctx: Context<'_>, content: impl Into<String>) -> anyhow::Result<()> {
    ctx.send(ephemeral_reply(content)).await?;
    Ok(())
}

pub async fn nickname(ctx: &Context<'_>, person: &User) -> Option<String> {
    let guild_id = ctx.guild_id()?;
    person.nick_in(ctx, guild_id).await
}

pub async fn name(ctx: &Context<'_>, person: &User) -> String {
    nickname(ctx, person)
        .await
        .unwrap_or_else(|| person.name.clone())
}

pub fn avatar_url(person: &User) -> String {
    person
        .avatar_url()
        .unwrap_or_else(|| person.default_avatar_url())
}

pub enum Score {
    Win,
    Loss,
    Draw,
}

pub fn pick_best_x_dice_rolls(
    die_sides: usize,
    total_rolls: usize,
    x: usize,
    seed: Option<&str>,
) -> usize {
    let mut rng = match seed {
        Some(s) => Seeder::from(&s).make_rng(),
        None => StdRng::seed_from_u64(rand::random::<u64>()),
    };

    let mut rolls = (0..total_rolls)
        .map(|_| rng.gen_range(1..=die_sides))
        .collect::<Vec<usize>>();
    rolls.sort();

    rolls.iter().rev().take(x).sum()
}
