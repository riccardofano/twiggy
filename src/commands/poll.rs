use crate::common::ephemeral_reply;
use crate::{Context, Result};

use poise::serenity_prelude::MessageId;
use poise::CreateReply;
use serenity::all::{CreateEmbed, EditMessage, ReactionType};
use tokio::sync::Mutex;

const ICONS_LEN: usize = 20;
const ICONS: [&str; ICONS_LEN] = [
    "✨", "🎉", "🎊", "🎃", "🎀", "🎄", "🎨", "🎪", "🎩", "👑", "🔮", "🥊", "🏹", "🔫", "📞", "📷",
    "🎬", "🔍", "💡", "📐",
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

#[poise::command(guild_only, slash_command)]
async fn new(
    ctx: Context<'_>,
    #[description = "What you want to ask"] question: String,
) -> Result<()> {
    let custom_data = unwrap_custom_data(ctx);
    let mut poll = custom_data.lock().await;

    if (*poll).is_some() {
        let msg = "There's a poll running already. Close it before creating a new one.";
        ctx.send(ephemeral_reply(msg)).await?;
        return Ok(());
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

#[poise::command(guild_only, slash_command)]
async fn close(ctx: Context<'_>) -> Result<()> {
    let custom_data = unwrap_custom_data(ctx);
    let mut poll = custom_data.lock().await;

    let response = match *poll {
        None => "There's no poll to close",
        Some(_) => {
            *poll = None;
            "The poll has been closed."
        }
    };

    ctx.send(ephemeral_reply(response)).await?;

    Ok(())
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
        let msg = ephemeral_reply("There's no poll running, create one with /poll new <question>");
        ctx.send(msg).await?;
        return Ok(());
    };

    let Ok(mut message) = ctx.channel_id().message(ctx, poll.message_id).await else {
        let msg = ephemeral_reply("Couldn't find the poll in this channel");
        ctx.send(msg).await?;
        return Ok(());
    };

    let Some(icon) = poll.available_icons.pop() else {
        let msg = ephemeral_reply("Sorry buddy but there are enough options already.");
        ctx.send(msg).await?;
        return Ok(());
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
        let msg = ephemeral_reply("There's no poll available my guy.");
        ctx.send(msg).await?;
        return Ok(());
    };

    let Ok(mut message) = ctx.channel_id().message(ctx, poll.message_id).await else {
        let msg = ephemeral_reply("Couldn't find the poll in this channel.");
        ctx.send(msg).await?;
        return Ok(());
    };

    let Some(position) = poll.choices.iter().position(|c| c.text == choice) else {
        ctx.send(ephemeral_reply("I couldn't find the choice."))
            .await?;
        return Ok(());
    };

    if poll.choices[position].owner != ctx.author().id.get() {
        ctx.send(ephemeral_reply("That wasn't a choice you submitted."))
            .await?;
        return Ok(());
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
