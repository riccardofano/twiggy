use std::borrow::Cow;
use std::time::Duration;

use anyhow::{bail, Context as Ctx};
use chrono::{NaiveDateTime, Utc};
use poise::serenity_prelude::{EditRole, Member, Mention, Role};
use rand::Rng;
use serenity::all::RoleId;
use sqlx::SqlitePool;
use tokio::sync::Mutex;

use crate::{
    common::{bail_reply, ephemeral_reply},
    Context, Result, SUB_ROLE_ID,
};

// These commands were originally made in american english so I'm keeping them
// that way, there won't be any `ou`s in this module.

const DEFAULT_GAMBLE_FAIL_CHANCE: u8 = 15;
const RANDOM_COLOR_COOLDOWN: Duration = Duration::from_secs(60 * 60);

const ANCHOR_ROLE_ID: RoleId = SUB_ROLE_ID;

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

/// Change your display color
#[poise::command(guild_only, slash_command, prefix_command)]
async fn change(
    ctx: Context<'_>,
    #[description = "The 6 digit hex color to change to"] hexcode: String,
) -> Result<()> {
    if let Err(reason) = reject_on_cooldown(ctx).await {
        return bail_reply(ctx, reason.to_string()).await;
    }

    let Some(color) = to_color(&hexcode) else {
        return bail_reply(ctx, "Please provide a valid hex color code.").await;
    };

    let Some(member) = ctx.author_member().await else {
        return bail_reply(ctx, "I could not find your roles.").await;
    };

    if let Err(reason) = reject_non_subs(&member).await {
        return bail_reply(ctx, reason.to_string()).await;
    }

    let role_name = match change_color(ctx, member, Some(color)).await {
        Ok(name) => name,
        Err(e) => {
            eprintln!("Error while trying to change color: {e}");
            let msg = "Something went wrong while trying to change your color. :(";
            return bail_reply(ctx, msg).await;
        }
    };

    bail_reply(ctx, format!("The role color {role_name} has been added!")).await
}

/// Randomize your display color
#[poise::command(guild_only, slash_command, prefix_command)]
async fn random(ctx: Context<'_>) -> Result<()> {
    if let Err(reason) = reject_on_cooldown(ctx).await {
        return bail_reply(ctx, reason.to_string()).await;
    }

    let Some(member) = ctx.author_member().await else {
        return bail_reply(ctx, "I could not find your roles.").await;
    };

    if let Err(reason) = reject_non_subs(&member).await {
        return bail_reply(ctx, reason.to_string()).await;
    }

    let role_result = change_color(ctx, member, None).await;
    if let Err(e) = role_result {
        eprintln!("Error while trying to change to a random color: {e}");
        let msg = "Something went wrong while trying to change your color :(";
        return bail_reply(ctx, msg).await;
    }

    update_last_random_cooldown(&ctx.data().database, &ctx.author().id.to_string()).await?;
    ctx.say(format!(
        "Hahaha. Get stuck with {} for an hour.",
        role_result.unwrap()
    ))
    .await?;

    Ok(())
}

/// Tempt the Wheel of Fate for a new color... or not!
#[poise::command(guild_only, slash_command, prefix_command)]
async fn gamble(ctx: Context<'_>) -> Result<()> {
    if let Err(reason) = reject_on_cooldown(ctx).await {
        return bail_reply(ctx, reason.to_string()).await;
    }

    let Some(member) = ctx.author_member().await else {
        return bail_reply(ctx, "I could not find your roles").await;
    };

    if let Err(reason) = reject_non_subs(&member).await {
        return bail_reply(ctx, reason.to_string()).await;
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
        return bail_reply(ctx, "Yay! You get to keep your color!").await;
    }

    let role_result = change_color(ctx, member, None).await;
    if let Err(e) = role_result {
        eprintln!("Error while trying to change to a random color: {e}");
        let msg = "Something went wrong while trying to change your color :(";
        return bail_reply(ctx, msg).await;
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
    let guild_id = ctx
        .guild_id()
        .expect("Expected colors commands to be guild only.");
    let Ok(guild) = guild_id.to_partial_guild(ctx).await else {
        bail!("Could not get guild from guild_id");
    };

    let color = color.unwrap_or_else(generate_random_hex_color);
    let role_name = format!("#{color:06X}");
    let role = match guild.role_by_name(&role_name) {
        Some(role) => role.clone(),
        None => {
            let Some(anchor_role) = guild.roles.get(&ANCHOR_ROLE_ID) else {
                bail!(
                    "The anchor role was not found, \
                unable to create a role with at the correct position."
                );
            };

            guild
                .create_role(
                    ctx,
                    EditRole::new()
                        .name(&role_name)
                        .colour(color as u64)
                        .position(anchor_role.position + 1),
                )
                .await?
        }
    };
    remove_unused_color_roles(ctx, &mut member).await?;
    member.to_mut().add_role(ctx, role.id).await?;

    Ok(role_name)
}

/// Change your favourite display color
#[poise::command(guild_only, slash_command, prefix_command)]
pub async fn favorite(
    ctx: Context<'_>,
    #[description = "The 6 digit hex color to change to"] hexcode: String,
) -> Result<()> {
    let Some(color) = to_color(&hexcode) else {
        return bail_reply(ctx, "Please provide a valid hex color code.").await;
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

    ctx.send(ephemeral_reply(format!(
        "{color_code} has been set as your favorite color!"
    )))
    .await?;

    Ok(())
}

/// Revert your color your favorite one
#[poise::command(guild_only, slash_command, prefix_command)]
pub async fn lazy(ctx: Context<'_>) -> Result<()> {
    if let Err(reason) = reject_on_cooldown(ctx).await {
        return bail_reply(ctx, reason.to_string()).await;
    }

    let Some(member) = ctx.author_member().await else {
        return bail_reply(ctx, "Could not find your roles").await;
    };

    if let Err(reason) = reject_non_subs(&member).await {
        return bail_reply(ctx, reason.to_string()).await;
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
        let msg = "You're so lazy you haven't even set a favorite color, set one for next time!";
        return bail_reply(ctx, msg).await;
    };

    let Some(author_member) = ctx.author_member().await else {
        return bail_reply(ctx, "Are you not in a guild right now?").await;
    };

    let Some(color) = to_color(&color_code) else {
        sqlx::query!("UPDATE User SET fav_color = NULL WHERE id = ?", author_id)
            .execute(&ctx.data().database)
            .await?;

        return bail_reply(
            ctx,
            "For some reason the color that was saved was invalid, \
            I reset it for you, \
            you should now set a new favorite.",
        )
        .await;
    };

    let color_role = change_color(ctx, author_member, Some(color)).await?;
    ctx.send(ephemeral_reply(format!(
        "Color has been changed to {color_role}"
    )))
    .await?;

    Ok(())
}

/// Remove a member's display color
#[poise::command(
    guild_only,
    slash_command,
    prefix_command,
    required_permissions = "MODERATE_MEMBERS"
)]
pub async fn uncolor(
    ctx: Context<'_>,
    #[description = "The member whose color you want to remove"] member: Member,
) -> Result<()> {
    let member_id = member.user.id;
    let roles_were_removed = remove_unused_color_roles(ctx, &mut Cow::Owned(member)).await?;

    if !roles_were_removed {
        ctx.send(ephemeral_reply("There were no roles to remove."))
            .await?;
    }

    ctx.send(ephemeral_reply(format!(
        "All color roles have been removed from {}.",
        Mention::from(member_id)
    )))
    .await?;

    Ok(())
}

/// Update the gamble percentage chance
#[poise::command(
    guild_only,
    slash_command,
    prefix_command,
    required_permissions = "ADMINISTRATOR"
)]
async fn setgamblechance(
    ctx: Context<'_>,
    #[description = "Gamble chance percentage"]
    #[min = 0]
    #[max = 100]
    percent: u8,
) -> Result<()> {
    if !(0..=100).contains(&percent) {
        return bail_reply(ctx, "Please provide a number between 0 and 100").await;
    }

    let custom_data = ctx.parent_commands()[0]
        .custom_data
        .downcast_ref::<Mutex<u8>>()
        .expect("Expected to have passed the gamble chance as custom_data");

    *custom_data.lock().await = percent;
    ctx.send(ephemeral_reply(format!(
        "Gamble chance has been set to {percent}%"
    )))
    .await?;

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
            continue;
        };
        if roles.contains(role) {
            return Ok(false);
        }
    }

    Ok(true)
}

async fn remove_unused_color_roles(ctx: Context<'_>, member: &mut Cow<'_, Member>) -> Result<bool> {
    let Some(roles) = member.roles(ctx) else {
        return Ok(false);
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

async fn reject_on_cooldown(ctx: Context<'_>) -> Result<()> {
    let user_id = ctx.author().id.to_string();
    let row = sqlx::query_as!(
        UserCooldowns,
        r#"INSERT OR IGNORE INTO User (id) VALUES (?);
        SELECT last_random, last_loss FROM User WHERE id = ?"#,
        user_id,
        user_id
    )
    .fetch_one(&ctx.data().database)
    .await
    .context("Something went wrong while trying to fetch your cooldowns")?;

    let now = Utc::now().naive_utc();
    let cooldown_duration = chrono::Duration::from_std(RANDOM_COLOR_COOLDOWN)?;

    let permitted_time_from_random = row.last_random + cooldown_duration;
    let permitted_time_from_loss = row.last_loss + cooldown_duration;

    if permitted_time_from_random > now {
        bail!(
            "You recently randomed/gambled, you can change your color <t:{}:R>",
            permitted_time_from_random.and_utc().timestamp()
        );
    }

    if permitted_time_from_loss > now {
        bail!(
            "You recently dueled and lost, you can change your color <t:{}:R>",
            permitted_time_from_loss.and_utc().timestamp()
        );
    }

    Ok(())
}

async fn reject_non_subs(member: &Member) -> Result<()> {
    if !member.roles.contains(&SUB_ROLE_ID) {
        bail!("Yay! You get to keep your white color!");
    }

    Ok(())
}
