use crate::{Context, Result};

use anyhow::anyhow;
use poise::serenity_prelude::Emoji;
use rand::{seq::SliceRandom, thread_rng};
use tokio::sync::OnceCell;

const MIXU_BANNER: &str =
    ":regional_indicator_m::regional_indicator_i::regional_indicator_x::regional_indicator_u:";
const MIKU_BANNER: &str =
    ":regional_indicator_m::regional_indicator_i::regional_indicator_k::regional_indicator_u:";
const MIKU_POSITIONS: [usize; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
static MIXU_PIECES: OnceCell<Vec<Emoji>> = OnceCell::const_new();

/// Generate a random mixu
#[poise::command(slash_command, prefix_command)]
pub async fn mixu(ctx: Context<'_>) -> Result<()> {
    let mut positions = MIKU_POSITIONS;
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
    let pieces = MIXU_PIECES
        .get_or_try_init(|| retrieve_mixu_emojis(ctx))
        .await?;

    let miku = stringify_mixu(pieces, &MIKU_POSITIONS, MIKU_BANNER);
    ctx.say(miku).await?;

    Ok(())
}

fn stringify_mixu(pieces: &[Emoji], positions: &[usize; 16], banner: &str) -> String {
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

fn count_points(tiles: &[usize; 16]) -> i64 {
    let mut count = 0;
    for row in 0..4 {
        for col in 0..4 {
            let index = row * 4 + col;
            let tile = tiles[index];

            if tile == index {
                count += 1;
            }

            // Not at the 4th col
            // There is no neighbor to the right of the tile in the 4th col
            // Next tile is its neighbor
            if col != 3 && tile % 4 != 3 && tiles[index + 1] == tile + 1 {
                count += 1;
            }

            // Not at last row
            // Tile under is its neighbor
            if row != 3 && tiles[index + 4] == tile + 4 {
                count += 1;
            }
        }
    }

    count as i64
}


#[cfg(test)]
mod tests {
    use super::*;

    const NO_POINT_POSITION: &[usize; 16] = &[10, 0, 14, 9, 7, 3, 12, 5, 2, 8, 6, 13, 1, 4, 15, 11];

    #[test]
    fn perfect_mixu_score() {
        let positions = MIKU_POSITIONS;
        let points = count_points(&positions);
        assert_eq!(points, 40);
    }

    #[test]
    fn awful_mixu_score() {
        let points = count_points(NO_POINT_POSITION);
        assert_eq!(points, 0);
    }

    #[test]
    fn one_row_and_one_col_neighbor() {
        let positions = [9, 15, 16, 2, 11, 10, 12, 1, 8, 4, 14, 5, 6, 3, 7, 13];
        let points = count_points(&positions);
        assert_eq!(points, 2);
    }

    #[test]
    fn no_points_for_wrapping_row_neighbors() {
        let positions = [10, 0, 14, 9, 7, 3, 4, 8, 2, 5, 6, 13, 1, 12, 15, 11];
        let points = count_points(&positions);
        assert_eq!(points, 1);
    }

    #[test]
    fn connection_bottom_right_corner() {
        let positions = [5, 12, 0, 9, 10, 8, 6, 4, 7, 1, 3, 11, 2, 13, 14, 15];
        let points = count_points(&positions);
        assert_eq!(points, 8);
    }
}
