use rpg_dice_roller::{Dice, DiceKind};

use crate::{common::ephemeral_reply, Context, Result};

#[poise::command(slash_command, prefix_command, subcommands("dice", "cursed"))]
pub async fn roll(_ctx: Context<'_>) -> Result<()> {
    Ok(())
}

#[poise::command(slash_command, prefix_command)]
pub async fn dice(
    ctx: Context<'_>,
    #[description = "The expression that will be parsed and rolled"] expression: String,
) -> Result<()> {
    match rpg_dice_roller::roll(&expression) {
        Err(msg) => {
            let full_message = format!("Failed to parse expression:\n{msg}");
            ctx.send(ephemeral_reply(full_message)).await?;
        }
        Ok(rolled) => {
            let rolls = rolled.to_string();
            let message = if rolls.len() < 200 {
                format!("'{expression}': {} = {}", rolls, rolled.value())
            } else {
                format!("'{expression}' = {}", rolled.value())
            };
            ctx.say(message).await?;
        }
    }

    Ok(())
}

#[poise::command(slash_command, prefix_command)]
pub async fn cursed(ctx: Context<'_>) -> Result<()> {
    let dice = Dice::new(999, DiceKind::Standard(444), &[]);
    let rolled = dice.roll_all(&mut rand::thread_rng());

    ctx.say(format!("'999d444' = {}", rolled.value())).await?;

    Ok(())
}
