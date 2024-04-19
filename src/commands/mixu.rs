use crate::{Context, Result};

use anyhow::anyhow;
use poise::serenity_prelude::Emoji;
use rand::{seq::SliceRandom, thread_rng};
use tokio::sync::OnceCell;

const MIXU_BANNER: &str =
    ":regional_indicator_m::regional_indicator_i::regional_indicator_x::regional_indicator_u:";
const MIKU_BANNER: &str =
    ":regional_indicator_m::regional_indicator_i::regional_indicator_k::regional_indicator_u:";
const MIXU_POSITIONS: [usize; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
static MIXU_PIECES: OnceCell<Vec<Emoji>> = OnceCell::const_new();

/// Generate a random mixu
#[poise::command(slash_command, prefix_command)]
pub async fn mixu(ctx: Context<'_>) -> Result<()> {
    let mut positions = MIXU_POSITIONS;
    positions.shuffle(&mut thread_rng());

    let pieces = MIXU_PIECES
        .get_or_try_init(|| retrieve_mixu_emojis(ctx))
        .await?;
    let mixu = stringify_mixu(pieces, &positions, MIXU_BANNER);

    ctx.say(mixu).await?;

    Ok(())
}

/// Have Miku stare into your soul
#[poise::command(slash_command, prefix_command)]
pub async fn mikustare(ctx: Context<'_>) -> Result<()> {
    let positions = MIXU_POSITIONS;
    let pieces = MIXU_PIECES
        .get_or_try_init(|| retrieve_mixu_emojis(ctx))
        .await?;
    let miku = stringify_mixu(pieces, &positions, MIKU_BANNER);

    ctx.say(miku).await?;

    Ok(())
}

fn stringify_mixu(pieces: &[Emoji], positions: &[usize], banner: &str) -> String {
    let mut output = String::with_capacity(128);
    output.push_str(banner);

    for i in 0..4 {
        output.push('\n');
        for j in 0..4 {
            let index = i * 4 + j;
            let position = positions[index];
            output.push_str(&pieces[position].to_string());
        }
    }

    output
}

async fn retrieve_mixu_emojis(ctx: Context<'_>) -> Result<Vec<Emoji>> {
    let guild = ctx.guild().ok_or_else(|| anyhow!("Failed to load guild"))?;
    let emojis = guild.emojis(ctx).await?;

    let mut miku_emoji_ids = Vec::with_capacity(16);
    for i in 1..=16 {
        let piece = emojis
            .iter()
            .find(|emoji| emoji.name == format!("miku{i}"))
            .ok_or_else(|| anyhow::anyhow!("Could not find miku piece {i}"))?;
        miku_emoji_ids.push(piece.clone());
    }

    Ok(miku_emoji_ids)
}
