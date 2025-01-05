use std::sync::atomic::{AtomicI64, Ordering};

use poise::serenity_prelude::{all::Message, Context};

use crate::config::{BLOB_HI_TIMEOUT, BLOB_ID};

static LAST_HELLO: AtomicI64 = AtomicI64::new(0);

pub async fn say_hi(ctx: &Context, message: &Message) {
    let user_id = message.author.id;
    if user_id != BLOB_ID {
        return;
    }

    let last_hello = LAST_HELLO.load(Ordering::Acquire);
    let next_hello = last_hello + BLOB_HI_TIMEOUT.num_milliseconds();
    let message_timestamp = message.timestamp.timestamp();
    if message_timestamp <= next_hello {
        return;
    }

    LAST_HELLO.store(message_timestamp, Ordering::Release);

    if let Err(e) = message.reply(ctx, "Hi Blob!").await {
        eprintln!("Failed to say hi to blob: {e:?}");
    };
}
