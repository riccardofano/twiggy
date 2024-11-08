use anyhow::Context as AnyhowContext;
use chrono::{DateTime, Utc};
use poise::serenity_prelude::{self as serenity};
use poise::CreateReply;
use serenity::all::MessageId;
use sqlx::{pool::PoolConnection, Sqlite};

use super::{CoreContext, CoreReplyHandle};
use crate::Result;

impl<'a> CoreContext for poise::Context<'a, crate::Data, crate::Error> {
    type Data = &'a crate::Data;
    type User = serenity::User;
    type Member = serenity::Member;

    type ReplyHandle = poise::ReplyHandle<'a>;
    type Collector = serenity::all::ComponentInteractionCollector;

    fn data(&self) -> Self::Data {
        poise::Context::data(*self)
    }
    async fn acquire_db_connection(&self) -> Result<PoolConnection<Sqlite>> {
        self.data()
            .database
            .acquire()
            .await
            .context("Failed to acquire database connection")
    }

    fn author(&self) -> &Self::User {
        poise::Context::author(*self)
    }
    async fn author_member(&self) -> Option<Self::Member> {
        let m = poise::Context::author_member(*self).await;
        m.map(|m| m.into_owned())
    }

    fn user_id(&self, user: &Self::User) -> serenity::UserId {
        user.id
    }
    async fn user_nickname(&self, user: &Self::User) -> Option<String> {
        let guild_id = self.guild_id()?;
        user.nick_in(&self, guild_id).await
    }
    async fn user_name(&self, user: &Self::User) -> String {
        self.user_nickname(user)
            .await
            .unwrap_or_else(|| user.name.clone())
    }
    async fn user_colour(&self, user: &Self::User) -> Option<serenity::Colour> {
        self.guild_id()?
            .member(&self, user.id)
            .await
            .ok()?
            .colour(self)
    }
    fn user_avatar_url(&self, user: &Self::User) -> String {
        user.avatar_url()
            .unwrap_or_else(|| user.default_avatar_url())
    }

    async fn reply(&self, builder: CreateReply) -> Result<()> {
        self.send(builder).await?;
        Ok(())
    }
    async fn bail(&self, content: String) -> Result<()> {
        let reply = CreateReply::default().content(content).ephemeral(true);
        self.send(reply).await?;
        Ok(())
    }
    async fn reply_with_handle(&self, builder: CreateReply) -> Result<Self::ReplyHandle> {
        self.send(builder).await.context("Failed to send reply")
    }

    fn create_collector(&self) -> Self::Collector {
        Self::Collector::new(self)
    }
    async fn timeout(&self, member: Option<Self::Member>, until: DateTime<Utc>) {
        if let Some(mut member) = member {
            if let Err(e) = member
                .disable_communication_until_datetime(self, until.into())
                .await
            {
                eprintln!("Failed to timeout {:?}, reason: {e:?}", member);
            }
        }
    }
}

impl<'a> CoreReplyHandle for poise::ReplyHandle<'a> {
    type Context = poise::Context<'a, crate::Data, crate::Error>;
    type Message = serenity::Message;

    async fn into_message(self) -> Result<Self::Message> {
        poise::ReplyHandle::into_message(self)
            .await
            .context("Failed to turn reply handle into message")
    }
    async fn message_id(&self) -> Result<MessageId> {
        self.message()
            .await
            .map(|m| m.id)
            .context("Failed to get message id")
    }
    async fn edit(&self, ctx: Self::Context, builder: CreateReply) -> Result<()> {
        poise::ReplyHandle::edit(self, ctx, builder)
            .await
            .context("Failed to edit reply")
    }
}
