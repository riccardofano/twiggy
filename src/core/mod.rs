mod real;

use std::borrow::Cow;

use ::serenity::all::{CreateInteractionResponse, MessageId, RoleId, UserId};
use ::serenity::futures::Stream;
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
pub struct MockMessage;

#[cfg_attr(test, mockall::automock(
    type Data=crate::Data;
    type Database=Pool<Sqlite>;
    type User=MockUser;
    type Member=MockMember;
    type ReplyHandle=MockCoreReplyHandle;
    type Interaction=MockCoreInteraction;
    type Collector=MockCoreCollector;
))]
pub trait CoreContext {
    type Member;
    type User;

    type ReplyHandle: CoreReplyHandle<Context = Self>;
    type Interaction: CoreInteraction<Member = Self::Member, User = Self::User>;
    type Collector: CoreCollector<Item = Self::Interaction>;

    fn data(&self) -> &crate::Data;
    async fn acquire_db_connection(&self) -> Result<PoolConnection<Sqlite>>;

    fn author(&self) -> &Self::User;
    async fn author_member(&self) -> Option<Self::Member>;
    async fn user_from_id(&self, user_id: UserId) -> Result<Self::User>;
    fn user_id(&self, user: &Self::User) -> serenity::UserId;
    async fn user_member(&self, user_id: serenity::UserId) -> Option<Self::Member>;
    async fn user_nickname(&self, user: &Self::User) -> Option<String>;
    async fn user_name(&self, user: &Self::User) -> String;
    async fn user_colour(&self, user: &Self::User) -> Option<serenity::Colour>;
    fn user_avatar_url(&self, user: &Self::User) -> String;
    fn member_has_role(&self, member: &Self::Member, role: RoleId) -> bool;
    async fn member_role_add(&self, member: &Self::Member, role: RoleId) -> Result<()>;
    async fn member_role_remove(&self, member: &Self::Member, role: RoleId) -> Result<()>;

    async fn reply(&self, builder: CreateReply) -> Result<()>;
    async fn bail(&self, content: &str) -> Result<()>;
    async fn reply_with_handle(&self, builder: CreateReply) -> Result<Self::ReplyHandle>;
    async fn respond(
        &self,
        interaction: Self::Interaction,
        builder: CreateInteractionResponse,
    ) -> Result<()>;

    async fn timeout(&self, member: Option<Self::Member>, until: DateTime<Utc>);
    fn create_collector(&self) -> Self::Collector;
}

#[cfg_attr(test, mockall::automock(type Context = MockCoreContext; type Message = MockMessage;))]
pub trait CoreReplyHandle {
    type Context: CoreContext;
    type Message: Clone;

    // I need the lifetime for the Cow, and an elided lifetime doesn't play well the mocks
    #[allow(clippy::needless_lifetimes)]
    async fn message<'a>(&'a self) -> Result<Cow<'a, Self::Message>>;
    async fn into_message(self) -> Result<Self::Message>;
    fn message_id(message: &Self::Message) -> MessageId;
    fn message_link(message: &Self::Message) -> String;
    async fn edit(&self, ctx: Self::Context, builder: CreateReply) -> Result<()>;
}

#[cfg_attr(test, mockall::automock(type Member = MockMember; type User = MockUser;))]
pub trait CoreInteraction {
    type Member;
    type User;

    fn author(&self) -> &Self::User;
    fn author_id(&self) -> serenity::UserId;
    fn custom_id(&self) -> &str;
}

pub type FilterFn<I> = Box<dyn Fn(&I) -> bool + Send + Sync + 'static>;
#[cfg_attr(test, mockall::automock(type Item = MockCoreInteraction;))]
pub trait CoreCollector: Send + Sync {
    type Item: CoreInteraction;

    fn message_id(self, message_id: MessageId) -> Self;
    fn filter(self, handler: FilterFn<Self::Item>) -> Self;
    fn timeout(self, duration: std::time::Duration) -> Self;

    fn stream(self) -> impl Stream<Item = Self::Item> + Unpin;
}
