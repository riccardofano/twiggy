use std::borrow::Cow;
use std::time::Duration;

use chrono::{NaiveDateTime, Utc};
use poise::serenity_prelude::{Member, Mention, Mutex, Role};
use rand::Rng;
use sqlx::SqlitePool;

use crate::{common::ephemeral_message, Context, Result, SUB_ROLE};

// These commands were originally made in american english so I'm keeping them
// that way, there won't be any `ou`s in this module.

const DEFAULT_GAMBLE_FAIL_CHANCE: u8 = 15;
const RANDOM_COLOR_COOLDOWN: Duration = Duration::from_secs(60 * 60);

#[poise::command(
    guild_only,
    slash_command,
    prefix_command,
    subcommands("change", "random", "favorite", "lazy", "gamble", "setgamblechance"),
    custom_data = "Mutex::new(DEFAULT_GAMBLE_FAIL_CHANCE)"
)]
pub async fn color(_ctx: Context<'_>) -> Result<()> {
    Ok(())
}

#[poise::command(guild_only, slash_command, prefix_command)]
async fn change(ctx: Context<'_>, hexcode: String) -> Result<()> {
    if let Some(reason) = reject_on_cooldown(ctx).await? {
        ephemeral_message(ctx, reason).await?;
        return Ok(());
    }

    let Some(color) = to_color(&hexcode) else {
        ephemeral_message(ctx, "Please provide a valid hex color code.").await?;
        return Ok(())
    };

    let Some(member) = ctx.author_member().await else {
        ephemeral_message(ctx, "I could not find your roles.").await?;
        return Ok(());
    };

    if let Some(reason) = reject_non_subs(&member).await {
        ephemeral_message(ctx, reason).await?;
        return Ok(());
    }

    let role_name = match change_color(ctx, member, Some(color)).await {
        Ok(name) => name,
        Err(e) => {
            eprintln!("Error while trying to change color: {e}");
            ephemeral_message(
                ctx,
                "Something went wrong while trying to change your color. :(",
            )
            .await?;
            return Ok(());
        }
    };

    ephemeral_message(ctx, format!("The role color {role_name} has been added!")).await?;

    Ok(())
}

#[poise::command(guild_only, slash_command, prefix_command)]
async fn random(ctx: Context<'_>) -> Result<()> {
    if let Some(reason) = reject_on_cooldown(ctx).await? {
        ephemeral_message(ctx, reason).await?;
        return Ok(());
    }

    let Some(member) = ctx.author_member().await else {
        ephemeral_message(ctx, "I could not find your roles.").await?;
        return Ok(())
    };

    if let Some(reason) = reject_non_subs(&member).await {
        ephemeral_message(ctx, reason).await?;
        return Ok(());
    }

    let role_result = change_color(ctx, member, None).await;
    if let Err(e) = role_result {
        eprintln!("Error while trying to change to a random color: {e}");
        ephemeral_message(
            ctx,
            "Something went wrong while trying to change your color. You're spared for now. :(",
        )
        .await?;
        return Ok(());
    }

    update_last_random_cooldown(&ctx.data().database, &ctx.author().id.to_string()).await?;
    ctx.say(format!(
        "Hahaha. Get stuck with {} for an hour.",
        role_result.unwrap()
    ))
    .await?;

    Ok(())
}

#[poise::command(guild_only, slash_command, prefix_command)]
async fn gamble(ctx: Context<'_>) -> Result<()> {
    if let Some(reason) = reject_on_cooldown(ctx).await? {
        ephemeral_message(ctx, reason).await?;
        return Ok(());
    }

    let Some(member) = ctx.author_member().await else {
        ephemeral_message(ctx, "I could not find your roles").await?;
        return Ok(())
    };

    if let Some(reason) = reject_non_subs(&member).await {
        ephemeral_message(ctx, reason).await?;
        return Ok(());
    }

    let custom_data = ctx.parent_commands()[0]
        .custom_data
        .downcast_ref::<Mutex<u8>>()
        .expect("Expected to have passed the gamble chance as custom_data");
    let gamble_chance = custom_data.lock().await;
    let roll: u8 = {
        let mut rng = rand::thread_rng();
        rng.gen_range(0..=100)
    };
    if roll > *gamble_chance {
        ephemeral_message(ctx, "Yay! You get to keep your color!").await?;
        return Ok(());
    }

    let role_result = change_color(ctx, member, None).await;
    if let Err(e) = role_result {
        eprintln!("Error while trying to change to a random color: {e}");
        ephemeral_message(
            ctx,
            "Something went wrong while trying to change your color. You're spared for now. :(",
        )
        .await?;
        return Ok(());
    }

    update_last_random_cooldown(&ctx.data().database, &ctx.author().id.to_string()).await?;
    ctx.say(format!(
        "Hahaha. Get stuck with {} for an hour.",
        role_result.unwrap()
    ))
    .await?;

    Ok(())
}

async fn change_color(
    ctx: Context<'_>,
    mut member: Cow<'_, Member>,
    color: Option<u32>,
) -> Result<String> {
    let Some(guild) = ctx.guild() else {
        return Err(anyhow::anyhow!("Could not find the guild guild where to assign a new color role."));
    };

    let color = color.unwrap_or_else(generate_random_hex_color);
    let role_name = format!("#{color:06X}");
    let role = match guild.role_by_name(&role_name) {
        Some(role) => role.clone(),
        None => {
            guild
                .create_role(ctx, |role| role.name(&role_name).colour(color as u64))
                .await?
        }
    };
    remove_unused_color_roles(ctx, &mut member).await?;
    member.to_mut().add_role(ctx, role.id).await?;

    Ok(role_name)
}

#[poise::command(guild_only, slash_command, prefix_command)]
pub async fn favorite(ctx: Context<'_>, hexcode: String) -> Result<()> {
    let Some(color) = to_color(&hexcode) else {
        ephemeral_message(ctx, "Please provide a valid hex color code.").await?;
        return Ok(())
    };
    let color_code = format!("#{color:06X}");
    let author_id = ctx.author().id.to_string();

    sqlx::query!(
        r#"INSERT OR IGNORE INTO User (id) VALUES (?);
        UPDATE User SET fav_color = ? WHERE id = ?"#,
        author_id,
        color_code,
        author_id
    )
    .execute(&ctx.data().database)
    .await?;

    ephemeral_message(
        ctx,
        format!("{color_code} has been set as your favorite color!"),
    )
    .await?;

    Ok(())
}

#[poise::command(guild_only, slash_command, prefix_command)]
pub async fn lazy(ctx: Context<'_>) -> Result<()> {
    if let Some(reason) = reject_on_cooldown(ctx).await? {
        ephemeral_message(ctx, reason).await?;
        return Ok(());
    }

    let Some(member) = ctx.author_member().await else {
        ephemeral_message(ctx, "Could not find your roles").await?;
        return Ok(());
    };

    if let Some(reason) = reject_non_subs(&member).await {
        ephemeral_message(ctx, reason).await?;
        return Ok(());
    }

    let author_id = ctx.author().id.to_string();

    let row = sqlx::query!(
        r#"INSERT OR IGNORE INTO User (id) VALUES (?);
        SELECT fav_color FROM User WHERE id = ?"#,
        author_id,
        author_id
    )
    .fetch_one(&ctx.data().database)
    .await?;

    let Some(color_code) = row.fav_color else {
        ephemeral_message(
            ctx,
            "You're so lazy you haven't even set a favorite color, \
            set one for next time!"
        ).await?;

        return Ok(());
    };

    let Some(author_member) = ctx.author_member().await else {
        ephemeral_message(ctx, "Are you not in a guild right now?").await?;
        return Ok(())
    };

    let Some(color) = to_color(&color_code) else {
        sqlx::query!("UPDATE User SET fav_color = NULL WHERE id = ?", author_id)
            .execute(&ctx.data().database)
            .await?;

        ephemeral_message(
            ctx,
            "For some reason the color that was saved was invalid, \
            I reset it for you, \
            you should now set a new favorite.",
        )
        .await?;
        return Ok(());
    };

    let color_role = change_color(ctx, author_member, Some(color)).await?;
    ephemeral_message(ctx, format!("Color has been changed to {color_role}")).await?;

    Ok(())
}

#[poise::command(
    guild_only,
    slash_command,
    prefix_command,
    required_permissions = "MODERATE_MEMBERS"
)]
pub async fn uncolor(ctx: Context<'_>, member: Member) -> Result<()> {
    let member_id = member.user.id;
    let roles_were_removed = remove_unused_color_roles(ctx, &mut Cow::Owned(member)).await?;

    if !roles_were_removed {
        ephemeral_message(ctx, "There were no roles to remove.").await?;
    }

    ephemeral_message(
        ctx,
        format!(
            "All color roles have been removed from {}.",
            Mention::from(member_id)
        ),
    )
    .await?;

    Ok(())
}

#[poise::command(
    guild_only,
    slash_command,
    prefix_command,
    required_permissions = "ADMINISTRATOR"
)]
async fn setgamblechance(ctx: Context<'_>, percent: u8) -> Result<()> {
    if !(0..=100).contains(&percent) {
        ephemeral_message(ctx, "Please provide a number between 0 and 100").await?;
        return Ok(());
    }

    let custom_data = ctx.parent_commands()[0]
        .custom_data
        .downcast_ref::<Mutex<u8>>()
        .expect("Expected to have passed the gamble chance as custom_data");

    *custom_data.lock().await = percent;
    ephemeral_message(ctx, format!("Gamble chance has been set to {percent}%")).await?;

    Ok(())
}

fn to_color(hexcode: &str) -> Option<u32> {
    let hexcode = hexcode.strip_prefix('#').unwrap_or(hexcode);

    match u32::from_str_radix(hexcode, 16) {
        Ok(code) if hexcode.len() == 6 && code > 0 => Some(code),
        _ => None,
    }
}

async fn is_role_unused(ctx: Context<'_>, role: &Role) -> Result<bool> {
    let members = role.guild_id.members(ctx, None, None).await?;
    for member in members {
        let Some(roles) = member.roles(ctx) else {
            continue
        };
        if roles.contains(role) {
            return Ok(false);
        }
    }

    Ok(true)
}

async fn remove_unused_color_roles(ctx: Context<'_>, member: &mut Cow<'_, Member>) -> Result<bool> {
    let Some(roles) = member.roles(ctx) else {
        return Ok(false)
    };

    let mut roles_were_removed = false;
    for role in roles.iter() {
        if !role.name.starts_with('#') {
            continue;
        }
        member.to_mut().remove_role(ctx, role.id).await?;
        roles_were_removed = true;

        if is_role_unused(ctx, role).await? {
            role.guild_id.delete_role(ctx, role.id).await?;
        }
    }

    Ok(roles_were_removed)
}

fn generate_random_hex_color() -> u32 {
    let mut rng = rand::thread_rng();
    rng.gen_range(0..0x1000000)
}

async fn update_last_random_cooldown(db: &SqlitePool, user_id: &str) -> Result<()> {
    sqlx::query!(
        r#"INSERT OR IGNORE INTO User (id) VALUES (?);
        UPDATE User SET last_random = datetime('now') WHERE id = ?"#,
        user_id,
        user_id
    )
    .execute(db)
    .await?;

    Ok(())
}

struct UserCooldowns {
    last_random: NaiveDateTime,
    last_loss: NaiveDateTime,
}

async fn reject_on_cooldown(ctx: Context<'_>) -> Result<Option<String>> {
    let user_id = ctx.author().id.to_string();
    let row = sqlx::query_as!(
        UserCooldowns,
        r#"INSERT OR IGNORE INTO User (id) VALUES (?);
        SELECT last_random, last_loss FROM User WHERE id = ?"#,
        user_id,
        user_id
    )
    .fetch_one(&ctx.data().database)
    .await?;

    let now = Utc::now().naive_utc();
    let cooldown_duration = chrono::Duration::from_std(RANDOM_COLOR_COOLDOWN)?;

    let permitted_time_from_random = row.last_random + cooldown_duration;
    let permitted_time_from_loss = row.last_loss + cooldown_duration;

    if permitted_time_from_random > now {
        return Ok(Some(format!(
            "You recently randomed/gambled, you can change your color <t:{}:R>",
            permitted_time_from_random.timestamp()
        )));
    }

    if permitted_time_from_loss > now {
        return Ok(Some(format!(
            "You recently dueled and lost, you can change your color <t:{}:R>",
            permitted_time_from_loss.timestamp()
        )));
    }

    Ok(None)
}

async fn reject_non_subs(member: &Member) -> Option<String> {
    if !member.roles.contains(&SUB_ROLE.into()) {
        return Some("Yay! You get to keep your white color!".to_string());
    }

    None
}
