use crate::Context;

use anyhow::Result;
use rand::seq::SliceRandom;

const REPLIES: &[&str] = &[
    "It is certain.",
    "It is decidedly so.",
    "Without a doubt.",
    "Yes definitely.",
    "You may rely on it.",
    "As I see it, yes.",
    "Most likely.",
    "Outlook good.",
    "Yes.",
    "Signs point to yes.",
    "Reply hazy, try again.",
    "Ask again later.",
    "Better not tell you now.",
    "Cannot predict now.",
    "Concentrate and ask again.",
    "Don't count on it.",
    "My reply is no.",
    "My sources say no.",
    "Outlook not so good.",
    "Very doubtful. ",
];

/// Magic 8 Ball in Rust
#[poise::command(slash_command, prefix_command)]
pub async fn eightball(
    ctx: Context<'_>,
    #[description = "The question you want to ask the 8 Ball"] message: Option<String>,
) -> Result<()> {
    let fortune = {
        // make sure rng goes out of scope before you call await
        let mut rng = rand::thread_rng();
        REPLIES
            .choose(&mut rng)
            .expect("Expected to have at least 1 choice")
    };
    let reply = match message {
        Some(message) => format!("{message} - {fortune}"),
        None => fortune.to_string(),
    };
    ctx.say(reply).await?;

    Ok(())
}
