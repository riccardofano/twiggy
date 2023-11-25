use crate::Context;

use poise::serenity_prelude::{Colour, Error, Member, MessageComponentInteraction, User};
use poise::ReplyHandle;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rand_seeder::Seeder;
use std::borrow::Cow;
use std::sync::Arc;

pub async fn ephemeral_message<S: AsRef<str>>(
    ctx: Context<'_>,
    content: S,
) -> Result<ReplyHandle, Error> {
    ctx.send(|message| message.content(content.as_ref()).ephemeral(true))
        .await
}

pub async fn ephemeral_interaction_response<S: AsRef<str>>(
    ctx: &Context<'_>,
    interaction: Arc<MessageComponentInteraction>,
    content: S,
) -> Result<(), Error> {
    interaction
        .create_interaction_response(&ctx, |r| {
            r.interaction_response_data(|d| d.content(content.as_ref()).ephemeral(true))
        })
        .await
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

pub async fn member<'a>(ctx: &'a Context<'_>) -> Option<Cow<'a, Member>> {
    ctx.author_member().await
}

pub async fn colour(ctx: &Context<'_>) -> Option<Colour> {
    member(ctx).await?.colour(ctx)
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
