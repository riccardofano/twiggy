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
    let rolled = match rpg_dice_roller::roll(&expression) {
        Ok(rolled) => rolled,
        Err(msg) => {
            let full_message = format!("```\nFailed to parse expression:\n{msg}\n```");
            ctx.send(ephemeral_reply(full_message)).await?;
            return Ok(());
        }
    };

    let rolls = rolled.to_string();
    let message = if rolls.len() < 200 {
        format!("`{expression}`: {} = {}", rolls, rolled.value())
    } else {
        format!("`{expression}` = {}", rolled.value())
    };

    ctx.say(message).await?;

    Ok(())
}

#[poise::command(slash_command, prefix_command)]
pub async fn cursed(ctx: Context<'_>) -> Result<()> {
    let dice = Dice::new(999, DiceKind::Standard(444), &[]);
    let rolled = dice.roll_all();

    ctx.say(format!("'999d444' = {}", rolled.value())).await?;

    Ok(())
}
