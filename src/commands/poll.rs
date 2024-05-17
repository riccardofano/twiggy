use crate::common::{bail_reply, ephemeral_reply};
use crate::{Context, Result};

use poise::serenity_prelude::MessageId;
use poise::{ChoiceParameter, CreateReply};
use serenity::all::{CreateEmbed, EditMessage, Message, ReactionType};
use tokio::sync::Mutex;

const ICONS_LEN: usize = 20;
const ICONS: [&str; ICONS_LEN] = [
    "📐", "💡", "🔍", "🎬", "📷", "📞", "🔫", "🏹", "🥊", "🔮", "👑", "🎩", "🎪", "🎨", "🎄", "🎀",
    "🎃", "🎊", "🎉", "✨",
];

struct Choice {
    icon: &'static str,
    owner: u64,
    text: String,
}

#[derive(Default)]
struct Poll {
    message_id: MessageId,
    question: String,
    choices: Vec<Choice>,
    available_icons: Vec<&'static str>,
}

type CustomData = Mutex<Option<Poll>>;

#[poise::command(
    guild_only,
    slash_command,
    subcommands("new", "close", "choice", "whoops"),
    custom_data = "CustomData::default()"
)]
pub async fn poll(_ctx: Context<'_>) -> Result<()> {
    Ok(())
}

#[poise::command(guild_only, slash_command, required_permissions = "MANAGE_MESSAGES")]
async fn new(
    ctx: Context<'_>,
    #[description = "What you want to ask"] question: String,
) -> Result<()> {
    let custom_data = unwrap_custom_data(ctx);
    let mut poll = custom_data.lock().await;

    if (*poll).is_some() {
        let msg = "There's a poll running already. Close it before creating a new one.";
        return bail_reply(ctx, msg).await;
    }

    let embed = CreateEmbed::new()
        .title(&question)
        .description("Add an option with /poll add_option <option>");
    let msg = ctx.send(CreateReply::default().embed(embed)).await?;
    let message_id = msg.into_message().await?.id;

    *poll = Some(Poll {
        message_id,
        question,
        choices: Vec::new(),
        available_icons: ICONS.to_vec(),
    });

    Ok(())
}

#[derive(ChoiceParameter)]
enum CloseKind {
    Announce,
    Silent,
}

#[poise::command(guild_only, slash_command, required_permissions = "MANAGE_MESSAGES")]
async fn close(
    ctx: Context<'_>,
    #[description = "Whether to announce the winner or not"] kind: Option<CloseKind>,
) -> Result<()> {
    let custom_data = unwrap_custom_data(ctx);
    let mut poll = custom_data.lock().await;

    let Some(found_poll) = &mut *poll else {
        return bail_reply(ctx, "There's no poll to close").await;
    };

    let kind = kind.unwrap_or(CloseKind::Silent);
    let reply = match kind {
        CloseKind::Silent => ephemeral_reply("The poll has been closed!"),
        CloseKind::Announce => {
            let Ok(message) = ctx.channel_id().message(ctx, found_poll.message_id).await else {
                let msg = "This channel is not the same as the one with the poll.";
                return bail_reply(ctx, msg).await;
            };
            announce_winner(found_poll, message).await
        }
    };

    *poll = None;
    ctx.send(reply).await?;

    Ok(())
}

async fn announce_winner(poll: &Poll, message: Message) -> CreateReply {
    let Some(reaction) = message
        .reactions
        .iter()
        .filter(|r| r.me)
        .max_by(|a, b| a.count.cmp(&b.count))
    else {
        return ephemeral_reply("There were no choices for this poll, I closed it now though.");
    };

    let reaction_str = reaction.reaction_type.to_string();
    let choice = poll
        .choices
        .iter()
        .find(|c| c.icon == reaction_str)
        .expect("Expected a reaction sent by the bot to be in the poll in memory.");

    let embed = CreateEmbed::default()
        .title(format!("{} winner:", poll.question))
        .description(format!(
            "{} {} with {} votes!",
            choice.icon,
            choice.text,
            reaction.count - 1
        ));

    CreateReply::default().embed(embed)
}

#[poise::command(guild_only, slash_command)]
async fn choice(
    ctx: Context<'_>,
    #[description = "The choice you want to add to the poll"]
    #[max_length = 25]
    choice: String,
) -> Result<()> {
    let custom_data = unwrap_custom_data(ctx);
    let mut poll = custom_data.lock().await;

    let Some(poll) = &mut *poll else {
        let msg = "There's no poll running, create one with /poll new <question>";
        return bail_reply(ctx, msg).await;
    };

    let Ok(mut message) = ctx.channel_id().message(ctx, poll.message_id).await else {
        let msg = "Couldn't find the poll in this channel";
        return bail_reply(ctx, msg).await;
    };

    let Some(icon) = poll.available_icons.pop() else {
        let msg = "Sorry buddy but there are enough options already.";
        return bail_reply(ctx, msg).await;
    };

    poll.choices.push(Choice {
        icon,
        owner: ctx.author().id.get(),
        text: choice,
    });

    message
        .edit(ctx, EditMessage::default().embed(poll_embed(poll)))
        .await?;

    tokio::try_join!(
        message.react(ctx, ReactionType::Unicode(icon.to_string())),
        ctx.send(ephemeral_reply("Choice added."))
    )?;

    Ok(())
}

#[poise::command(guild_only, slash_command)]
async fn whoops(
    ctx: Context<'_>,
    #[description = "The choice you want to remove"] choice: String,
) -> Result<()> {
    let custom_data = unwrap_custom_data(ctx);
    let mut poll = custom_data.lock().await;

    let Some(poll) = &mut *poll else {
        let msg = "There's no poll available my guy.";
        return bail_reply(ctx, msg).await;
    };

    let Ok(mut message) = ctx.channel_id().message(ctx, poll.message_id).await else {
        let msg = "Couldn't find the poll in this channel.";
        return bail_reply(ctx, msg).await;
    };

    let choice = choice.to_lowercase();
    let Some(position) = poll
        .choices
        .iter()
        .position(|c| c.text.to_lowercase() == choice)
    else {
        return bail_reply(ctx, "I couldn't find the choice.").await;
    };

    if poll.choices[position].owner != ctx.author().id.get() {
        return bail_reply(ctx, "That wasn't a choice you submitted.").await;
    }

    let choice = poll.choices.remove(position);
    poll.available_icons.push(choice.icon);

    message
        .edit(ctx, EditMessage::default().embed(poll_embed(poll)))
        .await?;
    message
        .delete_reaction_emoji(ctx, ReactionType::Unicode(choice.icon.to_string()))
        .await?;

    ctx.send(ephemeral_reply("That choice has been removed."))
        .await?;

    Ok(())
}

#[inline(always)]
fn unwrap_custom_data(ctx: Context<'_>) -> &CustomData {
    ctx.parent_commands()[0]
        .custom_data
        .downcast_ref::<CustomData>()
        .expect("Expected to have passed the poll data as custom_data")
}

fn poll_embed(poll: &Poll) -> CreateEmbed {
    let description = poll
        .choices
        .iter()
        .map(|c| format!("{} {}", c.icon, c.text))
        .collect::<Vec<_>>()
        .join("\n");

    CreateEmbed::default()
        .title(&poll.question)
        .description(description)
}
