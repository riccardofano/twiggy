use chrono::TimeDelta;
use serenity::all::{ChannelId, RoleId, UserId};

// ===================
//    Special users
// ===================

// Special users IDs
pub const BANANA_ID: UserId = UserId::new(1234567);
pub const GOZ_ID: UserId = UserId::new(1234567);
pub const BLOB_ID: UserId = UserId::new(104908485266817024);

// ===================
//    Special roles
// ===================

pub const SUB_ROLE: RoleId = RoleId::new(930791790490030100);
pub const COLOR_ANCHOR_ROLE: RoleId = SUB_ROLE;
pub const EMBED_ROLE: RoleId = RoleId::new(1325149128044445797);

// ===================
//       Channels
// ===================

pub const MIXU_CHANNEL: ChannelId = ChannelId::new(1232394759658541118);

// ===================
//      Cooldowns
// ===================

// /dino
pub const DINO_GIFTING_COOLDOWN: TimeDelta = TimeDelta::hours(1);
pub const DINO_SLURP_COOLDOWN: TimeDelta = TimeDelta::hours(1);

// /rpg
pub const RPG_DEAD_DUEL_COOLDOWN: TimeDelta = TimeDelta::minutes(5);
pub const RPG_LOSS_COOLDOWN: TimeDelta = TimeDelta::minutes(10);

// /ask
pub const ASK_COOLDOWN: TimeDelta = TimeDelta::seconds(10);

// /color
pub const DEFAULT_GAMBLE_FAIL_CHANCE: u8 = 15;
pub const RANDOM_COLOR_COOLDOWN: TimeDelta = TimeDelta::hours(1);

// /duel
pub const DUEL_LOSS_COOLDOWN: TimeDelta = TimeDelta::minutes(10);
pub const DEAD_DUEL_COOLDOWN: TimeDelta = TimeDelta::minutes(5);
pub const DRAW_TIMEOUT_DURATION: TimeDelta = TimeDelta::minutes(10);

// /rps
pub const RPS_ACCEPT_TIMEOUT: TimeDelta = TimeDelta::minutes(10);
pub const RPS_CHOICE_TIMEOUT: TimeDelta = TimeDelta::minutes(5);

// hi blob event
pub const BLOB_HI_TIMEOUT: TimeDelta = TimeDelta::hours(10);
