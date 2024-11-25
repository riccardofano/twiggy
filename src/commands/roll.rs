use rpg_dice_roller::{Dice, DiceKind};

use crate::{Context, Result};

#[poise::command(slash_command, prefix_command, subcommands("dice", "cursed"))]
pub async fn roll(ctx: Context<'_>) -> Result<()> {
    Ok(())
}

#[poise::command(slash_command, prefix_command)]
pub async fn dice(ctx: Context<'_>) -> Result<()> {
    todo!()
}

#[poise::command(slash_command, prefix_command)]
pub async fn cursed(ctx: Context<'_>) -> Result<()> {
    let dice = Dice::new(999, DiceKind::Standard(444), &[]);
    let rolled = dice.roll_all(&mut rand::thread_rng());

    ctx.say(format!("999d444: {}", rolled.value())).await?;

    Ok(())
}
