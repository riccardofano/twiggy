mod blob;
mod call_response;
mod streaming;

use std::sync::Arc;

use poise::serenity_prelude::{
    all::{Message, Presence},
    Context,
};

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

pub fn handle_presence_update(ctx: &Context, new_data: &Presence) {
    let ctx = ctx.clone();
    let new_data = new_data.clone();

    // TODO: When more tasks need to be added ctx/new_data should become Arcs like in `handle_new_message_event`
    tokio::spawn(async move {
        if let Err(e) = streaming::update_streaming_role_status(&ctx, &new_data).await {
            eprintln!("Error updating streaming role: {e:?}")
        }
    });
}

pub async fn initialize_event_data() {
    call_response::import_call_responses().await.unwrap();
}
