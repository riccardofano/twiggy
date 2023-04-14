use crate::Context;

use poise::serenity_prelude::{Colour, Error, Member, MessageComponentInteraction, User};
use poise::ReplyHandle;
use std::borrow::Cow;
use std::sync::Arc;

pub async fn ephemeral_message<S: AsRef<str>>(
    ctx: Context<'_>,
    content: S,
) -> Result<ReplyHandle, Error> {
    return ctx
        .send(|message| message.content(content.as_ref()).ephemeral(true))
        .await;
}

pub async fn ephemeral_interaction_response<S: AsRef<str>>(
    ctx: &Context<'_>,
    interaction: Arc<MessageComponentInteraction>,
    content: S,
) -> Result<(), Error> {
    return interaction
        .create_interaction_response(&ctx, |r| {
            r.interaction_response_data(|d| d.content(content.as_ref()).ephemeral(true))
        })
        .await;
}

pub async fn nickname(person: &User, ctx: &Context<'_>) -> Option<String> {
    let guild_id = ctx.guild_id()?;
    return person.nick_in(ctx, guild_id).await;
}

pub async fn name(person: &User, ctx: &Context<'_>) -> String {
    return nickname(person, ctx)
        .await
        .unwrap_or_else(|| person.name.clone());
}

pub async fn member<'a>(ctx: &'a Context<'_>) -> Option<Cow<'a, Member>> {
    return ctx.author_member().await;
}

pub async fn colour(ctx: &Context<'_>) -> Option<Colour> {
    return member(ctx).await?.colour(ctx);
}
