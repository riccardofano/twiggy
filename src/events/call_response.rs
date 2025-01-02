use std::{io::BufReader, path::Path, sync::OnceLock};

use chrono::{DateTime, TimeDelta, Utc};
use poise::serenity_prelude::{all::Message, Context};
use regex::Regex;
use serde::Deserialize;
use serenity::all::{CreateAttachment, CreateMessage};
use tokio::{fs::File, io::AsyncReadExt, sync::Mutex};

use crate::Result;

#[derive(Debug, Clone, Deserialize)]
struct Response {
    expression: String,
    cooldown: i64,
    text: Option<String>,
    attachment: Option<String>,
}

const ASSETS_PATH: &str = "./assets/call_responses";
static RESPONSES: OnceLock<Vec<Response>> = OnceLock::new();
static LAST_USES: Mutex<Vec<DateTime<Utc>>> = Mutex::const_new(Vec::new());

pub async fn import_call_responses() -> Result<()> {
    let file = std::fs::File::open(Path::new(ASSETS_PATH).join("data.json"))?;
    let reader = BufReader::new(file);
    let responses: Vec<Response> = serde_json::from_reader(reader)?;

    let mut last_uses = LAST_USES.lock().await;
    *last_uses = vec![DateTime::<Utc>::MIN_UTC; responses.len()];

    RESPONSES.set(responses).unwrap();
    Ok(())
}

pub async fn respond(ctx: &Context, message: &Message) {
    if message.author.bot {
        return;
    }

    for (i, response) in RESPONSES
        .get()
        .expect("responses to be initialized")
        .iter()
        .enumerate()
    {
        if is_on_cooldown(i, response.cooldown).await {
            continue;
        }

        let expression = &response.expression;
        let regex =
            Regex::new(expression).unwrap_or_else(|_| panic!("`{expression}` regex to be valid"));

        if !regex.is_match(&message.content) {
            continue;
        }

        match create_message_reply(response).await {
            None => continue,
            Some(msg) => {
                let mut last_uses = LAST_USES.lock().await;
                last_uses[i] = Utc::now();

                if let Err(e) = message.channel_id.send_message(ctx, msg).await {
                    eprintln!("Failed to send call response message {e:?}");
                };
            }
        };

        break;
    }
}

async fn create_message_reply(response: &Response) -> Option<CreateMessage> {
    let msg = match (&response.text, &response.attachment) {
        (None, None) => {
            eprintln!("Found a call response without anything to reply with {response:?}.");
            return None;
        }
        (Some(text), Some(filename)) => match get_attachment(filename).await {
            None => return None,
            Some(attachment) => CreateMessage::new().content(text).add_file(attachment),
        },
        (None, Some(filename)) => match get_attachment(filename).await {
            None => return None,
            Some(attachment) => CreateMessage::new().add_file(attachment),
        },
        (Some(text), None) => CreateMessage::new().content(text),
    };

    Some(msg)
}

async fn get_attachment(name: &str) -> Option<CreateAttachment> {
    let path = Path::new(ASSETS_PATH).join(name);
    let Ok(mut file) = File::open(&path).await else {
        eprintln!("Failed to open {path:?}");
        return None;
    };

    let mut buf = Vec::new();
    if let Err(e) = file.read_to_end(&mut buf).await {
        eprintln!("Failed to read {path:?} into bytes: {e:?}");
        return None;
    };

    Some(CreateAttachment::bytes(buf, name))
}

async fn is_on_cooldown(i: usize, cooldown_seconds: i64) -> bool {
    let last_uses = LAST_USES.lock().await;

    last_uses[i]
        .checked_add_signed(TimeDelta::seconds(cooldown_seconds))
        .unwrap()
        > Utc::now()
}
