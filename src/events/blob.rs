use std::sync::atomic::{AtomicI64, Ordering};

use serenity::all::UserId;

const BLOB_ID: UserId = UserId::new(1234); // TODO: proper id
const TIMEOUT: i64 = 1000 * 60 * 60 * 10;

static LAST_HELLO: AtomicI64 = AtomicI64::new(0);

pub fn try_saying_hi(user_id: UserId, message_timestamp: i64) -> Option<String> {
    if user_id != BLOB_ID {
        return None;
    }

    let last_hello = LAST_HELLO.load(Ordering::Acquire);
    let next_hello = last_hello + TIMEOUT;
    if message_timestamp <= next_hello {
        return None;
    }

    LAST_HELLO.store(message_timestamp, Ordering::Release);
    Some("Hi Blob!".to_string())
}
