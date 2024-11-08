use anyhow::Context as AnyhowContext;
use chrono::{DateTime, Utc};
use poise::serenity_prelude::{self as serenity};
use poise::CreateReply;
use sqlx::pool::PoolConnection;
use sqlx::Sqlite;

use crate::Result;

pub struct MockUser;
pub struct MockMember;
pub struct MockReplyHandle;
pub struct MockCollector;

#[cfg_attr(test, mockall::automock(
    type Data=crate::Data;
    type Database=Pool<Sqlite>;
    type User=MockUser;
    type Member=MockMember;
    type ReplyHandle=MockReplyHandle;
    type Collector=MockCollector;
))]
pub trait CoreContext {
    type Data;
    type User;
    type Member;

    type ReplyHandle;
    type Collector;

    fn data(&self) -> Self::Data;
    async fn acquire_db_connection(&self) -> Result<PoolConnection<Sqlite>>;
    fn author(&self) -> &Self::User;
    async fn author_member(&self) -> Option<Self::Member>;

    fn user_id(&self, user: &Self::User) -> serenity::UserId;
    async fn user_nickname(&self, user: &Self::User) -> Option<String>;
    async fn user_name(&self, user: &Self::User) -> String;
    async fn user_colour(&self, user: &Self::User) -> Option<serenity::Colour>;
    fn user_avatar_url(&self, user: &Self::User) -> String;

    async fn reply(&self, builder: CreateReply) -> Result<()>;
    async fn bail(&self, content: String) -> Result<()>;
    async fn reply_with_handle(&self, builder: CreateReply) -> Result<Self::ReplyHandle>;

    fn create_collector(&self) -> Self::Collector;
    async fn timeout(&self, member: Option<Self::Member>, until: DateTime<Utc>);
}

/**
 * Real context implementation
 */
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
