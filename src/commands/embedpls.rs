use serenity::all::Member;

use crate::{common::bail_reply, config::EMBED_ROLE, Context, Result};

/// Toggle the ability of embedding images/videos
#[poise::command(guild_only, slash_command, prefix_command)]
pub async fn embedpls(
    ctx: Context<'_>,
    #[description = "(mod only) User to add/remove role for"] member: Option<Member>,
) -> Result<()> {
    let member = match member {
        Some(user) if is_author_administrator(ctx).await => user,
        Some(_) => return bail_reply(ctx, "You don't have the power to do that.").await,
        None => ctx
            .author_member()
            .await
            .expect("Command should be guild only")
            .into_owned(),
    };

    if member.roles.contains(&EMBED_ROLE) {
        member.remove_role(ctx, EMBED_ROLE).await?;
        bail_reply(ctx, "The embed role has been removed.").await
    } else {
        member.add_role(ctx, EMBED_ROLE).await?;
        bail_reply(ctx, "The embed role has been added.").await
    }
}

async fn is_author_administrator(ctx: Context<'_>) -> bool {
    let Some(member) = ctx.author_member().await else {
        return false;
    };

    let Ok(permissions) = member.permissions(ctx) else {
        return false;
    };

    permissions.administrator()
}
