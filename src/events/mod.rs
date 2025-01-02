mod blob;
mod call_response;

use std::sync::Arc;

use poise::serenity_prelude::{all::Message, Context};

pub fn handle_new_message_event(ctx: &Context, message: &Message) {
    let ctx = Arc::new(ctx.clone());
    let message = Arc::new(message.clone());

    // Spawn each event manually so they will be done in parallel
    tokio::spawn({
        let ctx = Arc::clone(&ctx);
        let message = Arc::clone(&message);
        async move { blob::say_hi(&ctx, &message).await }
    });
    tokio::spawn({
        let ctx = Arc::clone(&ctx);
        let message = Arc::clone(&message);
        async move { call_response::respond(&ctx, &message).await }
    });
}

pub async fn initialize_event_data() {
    call_response::import_call_responses().await.unwrap();
}
