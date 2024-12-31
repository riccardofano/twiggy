use chrono::{TimeDelta, Utc};
use rand::Rng;
use serenity::all::{ButtonStyle, CreateActionRow, CreateButton, CreateMessage, Member, Message};

use crate::common::{bail_reply, name, text_message, update_response};
use crate::{Context, Result};

/// Commit sudoku
#[poise::command(guild_only, slash_command, prefix_command)]
pub async fn sudoku(
    ctx: Context<'_>,
    #[description = "Your final words"] message: Option<String>,
) -> Result<()> {
    let guild = ctx
        .partial_guild()
        .await
        .expect("Expected /sudoku to be guild only.");

    if ctx.author().id == guild.owner_id {
        return bail_reply(ctx, "Sadly I cannot time out the owner of the server.").await;
    }

    let Some(mut member) = ctx.author_member().await else {
        return bail_reply(ctx, "Could not get your member details.").await;
    };
    let Some(permissions) = member.permissions else {
        return bail_reply(ctx, "I was not able to find your permissions").await;
    };

    let random_timeout = {
        let mut rng = rand::thread_rng();
        rng.gen_range(420..=690)
    };

    let timeout_until = Utc::now()
        .checked_add_signed(TimeDelta::seconds(random_timeout))
        .unwrap();
    member
        .to_mut()
        .disable_communication_until_datetime(ctx, timeout_until.into())
        .await?;

    let goodbye_message = match message {
        Some(message) => format!("\n> {message}"),
        None => String::new(),
    };
    let author_name = name(&ctx, ctx.author()).await;

    ctx.say(format!(
        "{author_name} has been timed out for {random_timeout} seconds, or until <t:{}:T>.{goodbye_message}",
        timeout_until.timestamp()
    ))
    .await?;

    // Provide an escape hatch for mods
    if permissions.moderate_members() {
        let message = jailbreak_message();
        let handle = member.user.direct_message(ctx, message).await?;

        listen_to_jailbreak_message(ctx, &handle, member.into_owned()).await;
    }

    Ok(())
}

/// Time out a user
#[poise::command(
    guild_only,
    slash_command,
    required_permissions = "MODERATE_MEMBERS",
    default_member_permissions = "MODERATE_MEMBERS"
)]
pub async fn timeout(
    ctx: Context<'_>,
    #[description = "The chatter you want to time out"] mut chatter: Member,
    #[min = 1]
    #[max = 2419200] // 28 days in seconds
    #[description = "The timeout duration in seconds"]
    duration: i64,
) -> Result<()> {
    // There should be no need to check the author's permissions inside the
    // command because if they able to see it that should already mean they are
    // mods or have the permission specified by the server owner.

    let Ok(permissions) = chatter.permissions(ctx) else {
        return bail_reply(ctx, "I was not able to check the chatter's permissions").await;
    };

    let guild = ctx
        .partial_guild()
        .await
        .expect("Expected /timeout to be guild only.");

    if guild.owner_id == chatter.user.id {
        return bail_reply(ctx, "I can't time out the owner of the guild, sadly :(").await;
    }

    // Timeout user
    let until = Utc::now()
        .checked_add_signed(TimeDelta::seconds(duration))
        .unwrap();
    chatter
        .disable_communication_until_datetime(ctx, until.into())
        .await?;
    ctx.reply(format!(
        "{chatter} has been timed out. We'll see them again <t:{}:R>",
        until.timestamp()
    ))
    .await?;

    // Provide an escape hatch for mods
    if permissions.moderate_members() {
        let message = jailbreak_message();
        let handle = chatter.user.direct_message(ctx, message).await?;

        listen_to_jailbreak_message(ctx, &handle, chatter).await;
    }

    Ok(())
}

fn jailbreak_message() -> CreateMessage {
    let btn = CreateButton::new("pardon-btn")
        .emoji('⛏')
        .label("Break out of jail")
        .style(ButtonStyle::Danger);

    let row = CreateActionRow::Buttons(vec![btn]);

    CreateMessage::new()
        .content("It's dangerous to go alone! Take this.")
        .components(vec![row])
}

async fn listen_to_jailbreak_message(ctx: Context<'_>, handle: &Message, mut chatter: Member) {
    while let Some(interaction) = handle
        .await_component_interaction(ctx)
        .filter(|f| f.data.custom_id == "pardon-btn")
        .author_id(chatter.user.id)
        .await
    {
        if let Err(e) = chatter.enable_communication(ctx).await {
            eprintln!("Failed to remove timeout for {}: {e:?}", chatter.user.name);
            continue;
        };

        if let Err(e) = interaction
            .create_response(
                ctx,
                update_response(text_message("You were pardoned, my friend.").components(vec![])),
            )
            .await
        {
            eprintln!("Failed to remove timeout for {}: {e:?}", chatter.user.name);
        }

        break;
    }
}
