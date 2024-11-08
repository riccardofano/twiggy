mod real;

use ::serenity::all::MessageId;
use chrono::{DateTime, Utc};
use poise::serenity_prelude::{self as serenity};
use poise::CreateReply;
use sqlx::pool::PoolConnection;
use sqlx::Sqlite;

use crate::Result;

#[derive(Debug, Clone)]
pub struct MockUser;
#[derive(Debug, Clone)]
pub struct MockMember;
#[derive(Debug, Clone)]
pub struct MockCollector;
#[derive(Debug, Clone)]
pub struct MockMessage;

#[cfg_attr(test, mockall::automock(
    type Data=crate::Data;
    type Database=Pool<Sqlite>;
    type User=MockUser;
    type Member=MockMember;
    type ReplyHandle=MockCoreReplyHandle;
    type Collector=MockCollector;
))]
pub trait CoreContext {
    type Data;
    type User;
    type Member;

    type ReplyHandle: CoreReplyHandle<Context = Self>;
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

#[cfg_attr(test, mockall::automock(type Context = MockCoreContext; type Message = MockMessage;))]
pub trait CoreReplyHandle {
    type Context: CoreContext;
    type Message: Clone;

    async fn into_message(self) -> Result<Self::Message>;
    async fn message_id(&self) -> Result<MessageId>;
    async fn edit(&self, ctx: Self::Context, builder: CreateReply) -> Result<()>;
}
