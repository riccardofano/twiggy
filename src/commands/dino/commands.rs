use anyhow::bail;
use chrono::{NaiveDateTime, Utc};
use image::{imageops::overlay, io::Reader, ImageBuffer, ImageOutputFormat, RgbaImage};
use poise::serenity_prelude::{
    ButtonStyle, CreateActionRow, CreateAttachment, CreateButton, CreateEmbed, CreateEmbedAuthor,
    CreateEmbedFooter, User, UserId,
};
use poise::CreateReply;
use rand::{seq::SliceRandom, thread_rng};
use sqlx::error::DatabaseError;
use sqlx::sqlite::SqliteError;
use sqlx::{FromRow, QueryBuilder, Row, Sqlite, SqliteExecutor, SqlitePool};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tokio::sync::{RwLock, RwLockReadGuard};

use crate::common::{bail_reply, embed_message, ephemeral_text_message, response};
use crate::{
    common::{avatar_url, ephemeral_reply, name as get_name, pick_best_x_dice_rolls},
    Context, Result, SUB_ROLE_ID,
};

#[derive(Default)]
struct Fragments {
    bodies: Vec<PathBuf>,
    mouths: Vec<PathBuf>,
    eyes: Vec<PathBuf>,
}

#[derive(Debug)]
struct DinoParts {
    body: PathBuf,
    mouth: PathBuf,
    eyes: PathBuf,
    name: String,
}

const FRAGMENT_PATH: &str = "./assets/dino/fragments";
const OUTPUT_PATH: &str = "./assets/dino/complete";

const DINO_IMAGE_SIZE: u32 = 112;
const COLUMN_MARGIN: u32 = 2;
const ROW_MARGIN: u32 = 2;

const MAX_GENERATION_ATTEMPTS: usize = 20;
const MAX_FAILED_HATCHES: i64 = 3;
const HATCH_FAILS_TEXT: &[&str; 3] = &["1st", "2nd", "3rd"];

// const GIFTING_COOLDOWN: Duration = Duration::from_secs(60 * 60);
// const SLURP_COOLDOWN: Duration = Duration::from_secs(60 * 60);

pub const COVET_BUTTON: &str = "dino-covet";
pub const SHUN_BUTTON: &str = "dino-shun";
pub const FAVOURITE_BUTTON: &str = "dino-favourite";

fn setup_dinos() -> RwLock<Fragments> {
    let fragments_dir = std::fs::read_dir(FRAGMENT_PATH).expect("Could not read fragment path");

    let mut fragments = Fragments::default();

    for entry in fragments_dir {
        let entry = entry.expect("Could not read entry");
        if !entry.metadata().expect("Could not read metadata").is_file() {
            continue;
        }

        if let Some(file_stem) = entry.path().file_stem() {
            match file_stem.to_str() {
                Some(s) if s.ends_with("_b") => fragments.bodies.push(entry.path()),
                Some(s) if s.ends_with("_m") => fragments.mouths.push(entry.path()),
                Some(s) if s.ends_with("_e") => fragments.eyes.push(entry.path()),
                _ => {}
            }
        }
    }

    RwLock::new(fragments)
}

#[poise::command(
    slash_command,
    guild_only,
    subcommands("hatch", "collection", "rename", "view", "gift", "slurp", "slurpening"),
    custom_data = "setup_dinos()"
)]
pub async fn dino(_ctx: Context<'_>) -> Result<()> {
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct Timings {
    attempt: NaiveDateTime,
    reset: NaiveDateTime,
    next_try: i64,
    kind: UserAction,
}

impl Timings {
    fn hatch(user_record: &UserRecord) -> Self {
        let attempt = Utc::now().naive_utc();
        let last_hatch_date = user_record.last_hatch.date();
        // At midnight the day after the last hatch
        let reset_time = last_hatch_date.and_hms_opt(0, 0, 0).unwrap() + chrono::Duration::days(1);
        // At midnight the day after this most recent attempt
        let next_try = (attempt.date().and_hms_opt(0, 0, 0).unwrap() + chrono::Duration::days(1))
            .and_utc()
            .timestamp();

        Timings {
            attempt,
            reset: reset_time,
            next_try,
            kind: UserAction::Hatch(0),
        }
    }

    fn slurp(user_record: &UserRecord) -> Self {
        let attempt = Utc::now().naive_utc();
        let slurp_cooldown_duration = chrono::Duration::minutes(60);
        let time_until_next_slurp = user_record.last_slurp + slurp_cooldown_duration;

        Timings {
            attempt,
            reset: time_until_next_slurp,
            next_try: time_until_next_slurp.and_utc().timestamp(),
            kind: UserAction::Slurp,
        }
    }

    fn gift(user_record: &UserRecord) -> Self {
        let attempt = Utc::now().naive_utc();
        let gifting_cooldown_duration = chrono::Duration::minutes(60);
        let time_until_next_gift = user_record.last_gifting + gifting_cooldown_duration;

        Timings {
            attempt,
            reset: time_until_next_gift,
            next_try: time_until_next_gift.and_utc().timestamp(),
            kind: UserAction::Gift,
        }
    }

    fn ensure_outside_cooldown(&self) -> Result<()> {
        if self.attempt < self.reset {
            match self.kind {
                UserAction::Hatch(_) => bail!(
                    "Dont be greedy! You can hatch again <t:{}:R>.",
                    self.next_try
                ),
                UserAction::Slurp => bail!(
                    "Don't be greedy! You can slurp again <t:{}:R>",
                    self.next_try
                ),
                UserAction::Gift => bail!(
                    "You're too kind, you're gifting too often. You can gift again <t:{}:R>",
                    self.next_try
                ),
            }
        };

        Ok(())
    }
}

struct DinoUser {
    id: String,
    record: UserRecord,
    timings: Timings,
}

impl DinoUser {
    fn new(id: String, timings: Timings, record: UserRecord) -> Self {
        Self {
            id,
            record,
            timings,
        }
    }
}

/// Attempt to hatch a new dino.
#[poise::command(slash_command, guild_only)]
async fn hatch(ctx: Context<'_>) -> Result<()> {
    let author = ctx.author();
    let author_id = author.id.to_string();

    let db = &ctx.data().database;
    let hatcher = get_user_record(&ctx.data().database, &author_id).await?;

    let user = DinoUser::new(author_id, Timings::hatch(&hatcher), hatcher);

    if let Err(e) = user.timings.ensure_outside_cooldown() {
        return bail_reply(ctx, e.to_string()).await;
    }

    if let Err(e) = try_hatching(db, ctx, &user).await {
        ctx.say(e.to_string()).await?;
        return Ok(());
    }

    let fragments = unwrap_fragments(ctx).await.read().await;
    let Some(parts) = generate_dino(db, &fragments).await? else {
        let msg =
            "I tried really hard but i wasn't able to make a unique dino for you. Sorry... :'(";
        return bail_reply(ctx, msg).await;
    };

    let image_path = generate_dino_image(&parts)?;

    let mut transaction = ctx.data().database.begin().await?;

    let dino = insert_dino(&mut transaction, &user.id, &parts, &image_path, None).await?;
    update_last_user_action(&mut transaction, &user.id, UserAction::Hatch(0)).await?;

    let author_name = get_name(&ctx, author).await;
    let message = send_dino_embed(
        ctx,
        &dino,
        &author_name,
        &avatar_url(author),
        &image_path,
        user.timings.attempt,
    )
    .await?;

    update_hatch_message(&mut transaction, dino.id, &message).await?;

    transaction.commit().await?;

    Ok(())
}

/// View your dino collection.
#[poise::command(slash_command, guild_only)]
async fn collection(
    ctx: Context<'_>,
    #[description = "The user's whose collection you want to view"] user: Option<User>,
    #[description = "The type of collection you want to view"] kind: Option<CollectionKind>,
    #[description = "Whether the message will be shown to everyone or not"] silent: Option<bool>,
) -> Result<()> {
    let silent = silent.unwrap_or(true);
    let kind = kind.unwrap_or(CollectionKind::All);

    let user_is_author = user.is_none();
    let user = user.as_ref().unwrap_or_else(|| ctx.author());

    let db = &ctx.data().database;
    let dino_collection = fetch_collection(db, &user.id.to_string(), kind).await?;

    if dino_collection.dinos.is_empty() {
        let content = match user_is_author {
            true => "You don't have any dinos :'(".to_string(),
            false => format!("{} doesn't have any dinos :'(", get_name(&ctx, user).await),
        };
        return bail_reply(ctx, content).await;
    }

    let image = generate_dino_collection_image(&dino_collection.dinos)?;
    let author_name = get_name(&ctx, user).await;
    let filename = format!("{}_collection.png", user.name);

    let embed = CreateEmbed::default()
        .colour(0xffbf00)
        .author(CreateEmbedAuthor::new(&author_name).icon_url(avatar_url(user)))
        .title(format!("{}'s collection", &author_name))
        .description(dino_collection.description())
        .footer(CreateEmbedFooter::new(format!(
            "{}. They are worth: {} Bucks",
            dino_collection.count_as_string(),
            dino_collection.transaction_count
        )))
        .attachment(&filename);

    ctx.send(
        CreateReply::default()
            .embed(embed)
            .attachment(CreateAttachment::bytes(image, filename))
            .ephemeral(silent),
    )
    .await?;

    Ok(())
}

/// Give your dino a better name.
#[poise::command(slash_command, guild_only, prefix_command)]
async fn rename(
    ctx: Context<'_>,
    #[description = "The existing name of your dino"]
    #[autocomplete = "autocomplete_owned_dinos"]
    name: String,
    #[description = "The new name for your dino"] replacement: String,
) -> Result<()> {
    let Some(dino) = get_dino_record(&ctx.data().database, &name).await? else {
        let msg = "The name of the dino you specified was not found.";
        return bail_reply(ctx, msg).await;
    };

    if dino.owner_id != ctx.author().id.to_string().as_ref() {
        return bail_reply(ctx, "You don't own this dino, you can't rename it.").await;
    }

    if let Err(e) = update_dino_name(&ctx.data().database, dino.id, &replacement).await {
        if let Some(sqlite_error) = e.downcast_ref::<SqliteError>() {
            // NOTE: 2067 is the code for a UNIQUE constraint error in Sqlite
            // https://www.sqlite.org/rescode.html#constraint_unique
            if sqlite_error.code() == Some("2067".into()) {
                return bail_reply(ctx, "This name is already taken!").await;
            }
        };
        return Err(e);
    }

    ctx.send(ephemeral_reply(format!(
        "**{}** name has been update to **{}**!",
        dino.name, replacement
    )))
    .await?;

    Ok(())
}

/// View an existing dino.
#[poise::command(slash_command, guild_only, prefix_command)]
async fn view(
    ctx: Context<'_>,
    #[description = "The name of the dino"]
    #[autocomplete = "autocomplete_all_dinos"]
    name: String,
) -> Result<()> {
    let Some(dino) = get_dino_record(&ctx.data().database, &name).await? else {
        let msg = "The name of the dino you specified was not found.";
        return bail_reply(ctx, msg).await;
    };

    let owner_user_id = UserId::from_str(&dino.owner_id)?;
    let (user_name, user_avatar) = match owner_user_id.to_user(&ctx).await {
        Ok(user) => (get_name(&ctx, &user).await, avatar_url(&user)),
        Err(_) => {
            eprintln!("Could not find user with id: {owner_user_id}. Using a default owner name for this dino.");
            (
                "unknown user".to_string(),
                "https://cdn.discordapp.com/embed/avatars/0.png".to_string(),
            )
        }
    };
    let image_path = Path::new(OUTPUT_PATH).join(&dino.filename);

    send_dino_embed(
        ctx,
        &dino,
        &user_name,
        &user_avatar,
        &image_path,
        dino.created_at,
    )
    .await?;

    Ok(())
}

/// Gift your dino to another chatter. How kind.
#[poise::command(guild_only, slash_command, prefix_command)]
async fn gift(
    ctx: Context<'_>,
    #[description = "The name of the dino you want to give away"]
    #[autocomplete = "autocomplete_owned_dinos"]
    dino: String,
    #[description = "The person who will receive the dino"] recipient: User,
) -> Result<()> {
    let user_record = get_user_record(&ctx.data().database, &ctx.author().id.to_string()).await?;
    let timings = Timings::gift(&user_record);

    if let Err(e) = timings.ensure_outside_cooldown() {
        return bail_reply(ctx, e.to_string()).await;
    }

    let Some(dino_record) = get_dino_record(&ctx.data().database, &dino).await? else {
        return bail_reply(ctx, "Could not find a dino named {dino}.").await;
    };

    if dino_record.owner_id != ctx.author().id.to_string().as_ref() {
        return bail_reply(ctx, "You cannot gift a dino you don't own.").await;
    }

    let mut transaction = ctx.data().database.begin().await?;
    let author_id = ctx.author().id.to_string();

    gift_dino(
        &mut transaction,
        dino_record.id,
        &author_id,
        &recipient.id.to_string(),
    )
    .await?;
    update_last_user_action(&mut transaction, &author_id, UserAction::Gift).await?;

    let sender_name = get_name(&ctx, ctx.author()).await;
    let receiver_name = get_name(&ctx, &recipient).await;
    let dino_name = if dino_record.hatch_message.is_empty() {
        dino
    } else {
        format!("[{}]({})", dino, dino_record.hatch_message)
    };

    let embed = CreateEmbed::default().colour(0x990933).description(format!(
        "**{sender_name}** gifted {dino_name} to **{receiver_name}**! How kind!",
    ));

    ctx.send(CreateReply::default().embed(embed)).await?;

    transaction.commit().await?;

    Ok(())
}

/// Sacrifice two dinos to create a new one
#[poise::command(guild_only, slash_command, prefix_command)]
async fn slurp(
    ctx: Context<'_>,
    #[description = "The first dino to be slurped"]
    #[autocomplete = "autocomplete_owned_dinos"]
    first: String,
    #[description = "The second dino to be slurped"]
    #[autocomplete = "autocomplete_owned_dinos"]
    second: String,
) -> Result<()> {
    if first.trim() == second.trim() {
        return bail_reply(ctx, "You can't slurp the same dino twice, you cheater!").await;
    }

    let user_record = get_user_record(&ctx.data().database, &ctx.author().id.to_string()).await?;
    let timings = Timings::slurp(&user_record);

    if let Err(e) = timings.ensure_outside_cooldown() {
        return bail_reply(ctx, e.to_string()).await;
    }

    let Some(first_dino) = get_dino_record(&ctx.data().database, &first).await? else {
        return bail_reply(ctx, format!("Could not find a dino named {first}.")).await;
    };

    let author_id = ctx.author().id.to_string();

    if first_dino.owner_id != author_id {
        let msg =
            format!("Doesn't seem you own {first}, are you trying to pull a fast one on me?!");
        return bail_reply(ctx, msg).await;
    }

    let Some(second_dino) = get_dino_record(&ctx.data().database, &second).await? else {
        return bail_reply(ctx, format!("Could not find a dino named {second}.")).await;
    };

    if second_dino.owner_id != author_id {
        let msg =
            format!("Doesn't seem you own {second}, are you trying to pull a fast one on me?!");
        return bail_reply(ctx, msg).await;
    }
    let fragments = unwrap_fragments(ctx).await.read().await;
    let parts = generate_dino(&ctx.data().database, &fragments).await?;

    if parts.is_none() {
        let msg =
            "I tried really hard but i wasn't able to make a unique dino for you. Sorry... :'(";
        return bail_reply(ctx, msg).await;
    }

    let mut transaction = ctx.data().database.begin().await?;
    delete_dino(&mut transaction, first_dino.id).await?;
    delete_dino(&mut transaction, second_dino.id).await?;

    let parts = parts.unwrap();
    let image_path = generate_dino_image(&parts)?;

    let dino = insert_dino(&mut transaction, &author_id, &parts, &image_path, None).await?;
    update_last_user_action(&mut transaction, &author_id, UserAction::Slurp).await?;

    let author_name = get_name(&ctx, ctx.author()).await;
    let message = send_dino_embed(
        ctx,
        &dino,
        &author_name,
        &avatar_url(ctx.author()),
        &image_path,
        Utc::now().naive_utc(),
    )
    .await?;

    update_hatch_message(&mut transaction, dino.id, &message).await?;

    transaction.commit().await?;

    Ok(())
}

/// Sacrifice all your non favourite dinos to create new ones (2 -> 1)
#[poise::command(guild_only, slash_command, prefix_command)]
async fn slurpening(ctx: Context<'_>) -> Result<()> {
    let user_id = ctx.author().id.to_string();
    let user_record = get_user_record(&ctx.data().database, &user_id).await?;
    let timings = Timings::slurp(&user_record);

    if let Err(e) = timings.ensure_outside_cooldown() {
        return bail_reply(ctx, e.to_string()).await;
    }

    let mut sacrifices = get_non_favourites(&ctx.data().database, &user_id).await?;

    if sacrifices.len() < 2 {
        let msg = "You don't have enough trash dinos to slurp, the minimum is 2.";
        return bail_reply(ctx, msg).await;
    }

    if sacrifices.len() % 2 == 1 {
        sacrifices.pop();
    }

    let num_to_sacrifice = sacrifices.len();
    let num_to_create = num_to_sacrifice / 2;
    let dinos_at_risk: String = sacrifices
        .iter()
        .map(|d| d.name.as_ref())
        .collect::<Vec<_>>()
        .join(", ");

    let content = format!(
        "You're about to sacrifice {} dinos to create {} new ones.\n\
        Remember their names: {}.\n\
        Are you SURE you want to do this?",
        num_to_sacrifice, num_to_create, dinos_at_risk
    );

    let confirm_button = CreateButton::new("slurpening-confirm")
        .emoji('🔪')
        .label("I AM 100% SURE".to_string())
        .style(ButtonStyle::Danger);

    let reply_handle = ctx
        .send(
            CreateReply::default()
                .components(vec![CreateActionRow::Buttons(vec![confirm_button])])
                .content(content)
                .ephemeral(true),
        )
        .await?;

    while let Some(interaction) = reply_handle
        .message()
        .await?
        .await_component_interaction(ctx)
        .timeout(std::time::Duration::from_secs(10))
        .await
    {
        if interaction.data.custom_id != "slurpening-confirm" {
            continue;
        }

        let fragments = unwrap_fragments(ctx).await.read().await;
        let mut transaction = ctx.data().database.begin().await?;

        for dino in sacrifices.iter() {
            delete_dino(&mut transaction, dino.id).await?;
        }

        let message_link = interaction.message.link();

        let mut created_dinos = Vec::with_capacity(num_to_create);
        for _ in 0..num_to_create {
            let Some(parts) = generate_dino(&ctx.data().database, &fragments).await? else {
                interaction
                    .create_response(
                        ctx,
                        response(ephemeral_text_message(
                            "Sorry but I couldn't generate a dino. Aborting slurpening.",
                        )),
                    )
                    .await?;
                return Ok(());
            };

            let file_path = generate_dino_image(&parts)?;
            let inserted_dino = insert_dino(
                &mut transaction,
                &user_id,
                &parts,
                &file_path,
                Some(&message_link),
            )
            .await?;
            created_dinos.push(inserted_dino);
        }

        let image = generate_dino_collection_image(&created_dinos)?;
        let filename = format!("{}_collection.png", user_id);

        let author_name = get_name(&ctx, ctx.author()).await;
        let author_avatar = avatar_url(ctx.author());

        let new_dino_names = created_dinos
            .iter()
            .map(|d| d.name.as_ref())
            .collect::<Vec<_>>()
            .join(", ");
        let embed = CreateEmbed::default()
            .colour(0xffbf00)
            .author(CreateEmbedAuthor::new(&author_name).icon_url(author_avatar))
            .title(format!("{}'s babies", &author_name))
            .description(format!("{num_to_create} dinos came out: {new_dino_names}."))
            .attachment(&filename);

        interaction
            .create_response(
                ctx,
                response(embed_message(embed).add_file(CreateAttachment::bytes(image, filename))),
            )
            .await?;

        update_last_user_action(&mut transaction, &user_id, UserAction::Slurp).await?;
        transaction.commit().await?;

        return Ok(());
    }

    reply_handle
        .edit(
            ctx,
            CreateReply::default()
                .components(Vec::new())
                .content("Looks like you decided to hold off for now."),
        )
        .await?;
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum UserAction {
    Hatch(i64),
    Slurp,
    Gift,
}

impl UserAction {
    fn to_update_query(self) -> String {
        match self {
            UserAction::Hatch(consecutive_fails) => {
                format!("last_hatch = datetime('now'), consecutive_fails = {consecutive_fails}")
            }
            UserAction::Slurp => "last_slurp = datetime('now')".to_string(),
            UserAction::Gift => "last_gifting = datetime('now')".to_string(),
        }
    }
}

async fn update_last_user_action(
    executor: impl SqliteExecutor<'_>,
    user_id: &str,
    action: UserAction,
) -> Result<()> {
    let mut query = QueryBuilder::new(format!(
        "UPDATE DinoUser SET {} WHERE id = ",
        action.to_update_query()
    ));
    query.push_bind(user_id);

    query.build().execute(executor).await?;

    Ok(())
}

async fn generate_dino(
    executor: impl SqliteExecutor<'_> + Copy,
    fragments: &RwLockReadGuard<'_, Fragments>,
) -> Result<Option<DinoParts>> {
    let mut tries = 0;

    loop {
        let mut generated = choose_parts(fragments);
        let duplicate_parts = are_parts_duplicate(executor, &generated).await?;

        if !duplicate_parts {
            loop {
                let duplicate_name = is_name_duplicate(executor, &generated).await?;
                if !duplicate_name {
                    break;
                }
                // NOTE: The Pedr method of getting a username, add an underscore
                // until it's not duplicated anymore
                generated.name.push('_');
            }
            return Ok(Some(generated));
        }

        tries += 1;
        if tries > MAX_GENERATION_ATTEMPTS {
            return Ok(None);
        }
    }
}

async fn are_parts_duplicate(executor: impl SqliteExecutor<'_>, parts: &DinoParts) -> Result<bool> {
    let body = get_file_name(&parts.body);
    let mouth = get_file_name(&parts.mouth);
    let eyes = get_file_name(&parts.eyes);
    let row = sqlx::query!(
        "SELECT id FROM Dino WHERE body = ? AND mouth = ? AND eyes = ?",
        body,
        mouth,
        eyes
    )
    .fetch_optional(executor)
    .await?;

    Ok(row.is_some())
}

async fn is_name_duplicate(executor: impl SqliteExecutor<'_>, parts: &DinoParts) -> Result<bool> {
    let row = sqlx::query!("SELECT id FROM Dino WHERE name = ?", parts.name)
        .fetch_optional(executor)
        .await?;

    Ok(row.is_some())
}

fn choose_parts(fragments: &Fragments) -> DinoParts {
    let mut rng = thread_rng();
    let body = fragments
        .bodies
        .choose(&mut rng)
        .expect("Expected to have at least one body")
        .to_path_buf();
    let mouth = fragments
        .mouths
        .choose(&mut rng)
        .expect("Expected to have at least one mouth")
        .to_path_buf();
    let eyes = fragments
        .eyes
        .choose(&mut rng)
        .expect("Expected to have at least one set of eyes")
        .to_path_buf();

    let mut parts = DinoParts {
        body,
        mouth,
        eyes,
        name: String::new(),
    };

    parts.name = generate_dino_name(&parts);
    parts
}

fn get_file_name(path: &Path) -> &str {
    path.file_name().unwrap().to_str().unwrap()
}

fn get_file_stem(path: &Path) -> &str {
    path.file_stem().unwrap().to_str().unwrap()
}

fn generate_dino_name(parts: &DinoParts) -> String {
    let body = get_file_stem(&parts.body).replace("_b", "");
    let mouth = get_file_stem(&parts.mouth).replace("_m", "");
    let eyes = get_file_stem(&parts.eyes).replace("_e", "");

    let body_end = 3.min(body.len());
    let mouth_start = 3.min(mouth.len() - 3);
    let eyes_start = 6.min(eyes.len() - 3);

    format!(
        "{}{}{}",
        &body[..body_end],
        &mouth[mouth_start..],
        &eyes[eyes_start..]
    )
}

fn generate_dino_image(parts: &DinoParts) -> Result<PathBuf> {
    let mut body = Reader::open(&parts.body)?.decode()?;
    let mouth = Reader::open(&parts.mouth)?.decode()?;
    let eyes = Reader::open(&parts.eyes)?.decode()?;

    overlay(&mut body, &mouth, 0, 0);
    overlay(&mut body, &eyes, 0, 0);

    let output_path = Path::new(OUTPUT_PATH);
    let path = output_path.join(&parts.name).with_extension("png");
    body.save_with_format(&path, image::ImageFormat::Png)?;

    Ok(path)
}

fn generate_dino_collection_image(collection: &[DinoRecord]) -> Result<Vec<u8>> {
    let columns = (collection.len() as f32).sqrt().ceil() as u32;
    let rows = (collection.len() as f32 / columns as f32).ceil() as u32;

    let width: u32 = columns * DINO_IMAGE_SIZE + (columns - 1) * COLUMN_MARGIN;
    let height: u32 = rows * DINO_IMAGE_SIZE + (rows - 1) * ROW_MARGIN;

    let output_path = Path::new(OUTPUT_PATH);

    let mut image: RgbaImage = ImageBuffer::new(width, height);
    for (i, dino) in collection.iter().enumerate() {
        let x = (i as u32 % columns) * (COLUMN_MARGIN + DINO_IMAGE_SIZE);
        let y = (i as f32 / columns as f32).floor() as u32 * (ROW_MARGIN + DINO_IMAGE_SIZE);

        let dino_image_path = output_path.join(&dino.filename);

        if !dino_image_path.exists() {
            let fragment_path = Path::new(FRAGMENT_PATH);
            generate_dino_image(&DinoParts {
                body: fragment_path.join(&dino.body),
                mouth: fragment_path.join(&dino.mouth),
                eyes: fragment_path.join(&dino.eyes),
                name: dino.name.clone(),
            })?;
        }

        let dino_image = Reader::open(&dino_image_path)?.decode()?;
        overlay(&mut image, &dino_image, x.into(), y.into());
    }

    let mut bytes: Vec<u8> = Vec::new();
    image.write_to(&mut Cursor::new(&mut bytes), ImageOutputFormat::Png)?;

    Ok(bytes)
}

#[derive(Debug)]
struct UserRecord {
    last_hatch: NaiveDateTime,
    last_slurp: NaiveDateTime,
    last_gifting: NaiveDateTime,
    consecutive_fails: i64,
}

async fn get_user_record(executor: impl SqliteExecutor<'_>, user_id: &str) -> Result<UserRecord> {
    let row = sqlx::query_as!(
        UserRecord,
        r#"INSERT OR IGNORE INTO DinoUser (id) VALUES (?);
        SELECT last_hatch, last_slurp, last_gifting, consecutive_fails FROM DinoUser WHERE id = ?"#,
        user_id,
        user_id,
    )
    .fetch_one(executor)
    .await?;

    Ok(row)
}

async fn insert_dino(
    executor: impl SqliteExecutor<'_>,
    user_id: &str,
    parts: &DinoParts,
    file_path: &Path,
    message_link: Option<&str>,
) -> Result<DinoRecord> {
    let body = get_file_name(&parts.body);
    let mouth = get_file_name(&parts.mouth);
    let eyes = get_file_name(&parts.eyes);
    let file_name = get_file_name(file_path);
    let message_link = message_link.unwrap_or_default();

    // NOTE: `query_as!` mistakenly interprets all string type fields as
    // nullable strings (when every field is marked NOT NULL), using
    // `query_as_unchecked!` until that gets fixed.
    let row = sqlx::query_as_unchecked!(
        DinoRecord,
        r#"INSERT INTO Dino
        (owner_id, name, filename, created_at, body, mouth, eyes, hatch_message)
        VALUES (?, ?, ?, datetime('now'), ?, ?, ?, ?)
        RETURNING *"#,
        user_id,
        parts.name,
        file_name,
        body,
        mouth,
        eyes,
        message_link
    )
    .fetch_one(executor)
    .await?;

    Ok(row)
}

async fn update_hatch_message(
    executor: impl SqliteExecutor<'_>,
    dino_id: i64,
    message_link: &str,
) -> Result<()> {
    // NOTE: sqlx has some issues handling hatch_message being NULL
    // so I just made it default to an empty string
    sqlx::query!(
        "UPDATE Dino SET hatch_message = ? WHERE id = ?",
        message_link,
        dino_id
    )
    .execute(executor)
    .await?;

    Ok(())
}

#[derive(FromRow)]
struct DinoRecord {
    id: i64,
    owner_id: String,
    name: String,
    hatch_message: String,
    created_at: NaiveDateTime,
    worth: i64,
    hotness: i64,

    filename: String,
    body: String,
    mouth: String,
    eyes: String,
}

struct DinoCollection {
    dino_count: i64,
    transaction_count: i64,
    dinos: Vec<DinoRecord>,
}

impl DinoCollection {
    fn join_names(&self) -> String {
        self.dinos
            .iter()
            .map(|d| d.name.as_ref())
            .collect::<Vec<&str>>()
            .join(", ")
    }

    fn description(&self) -> String {
        let others_count = self.dino_count - self.dinos.len() as i64;
        let dino_names = self.join_names();

        if others_count == 1 {
            format!("{} and one more!", &dino_names)
        } else if others_count > 0 {
            format!("{} and {} others!", &dino_names, &others_count)
        } else {
            format!("{dino_names}!")
        }
    }

    fn count_as_string(&self) -> String {
        if self.dino_count == 1 {
            "1 Dino".to_string()
        } else {
            format!("{} Dinos", self.dino_count)
        }
    }
}

#[derive(poise::ChoiceParameter)]
enum CollectionKind {
    All,
    Favourite,
    Trash,
}

impl CollectionKind {
    fn push_to_query<'a>(&self, query: &mut QueryBuilder<'a, Sqlite>, user_id: &'a str) {
        match self {
            CollectionKind::All => {
                query.push("WHERE owner_id = ");
                query.push_bind(user_id);
            }
            CollectionKind::Favourite => {
                query.push("INNER JOIN DinoTransactions t WHERE owner_id = ");
                query.push_bind(user_id);
                query.push("AND Dino.id = t.dino_id AND t.type = 'FAVOURITE'");
            }
            CollectionKind::Trash => {
                query.push("WHERE owner_id = ");
                query.push_bind(user_id);
                query.push(
                    "AND id NOT IN (SELECT dino_id FROM DinoTransactions WHERE type = 'FAVOURITE')",
                );
            }
        };
    }
}

async fn fetch_collection(
    executor: impl SqliteExecutor<'_> + Copy,
    user_id: &str,
    kind: CollectionKind,
) -> Result<DinoCollection> {
    // NOTE: query gets reset to whatever was passed into new so I initialized
    // it to an empty string
    let mut query = QueryBuilder::new("");
    query.push("INSERT OR IGNORE INTO DinoUser (id) VALUES (");
    query.push_bind(user_id);
    query.push(");");

    query.push("SELECT * FROM Dino ");
    kind.push_to_query(&mut query, user_id);
    query.push("LIMIT 25");

    let dinos: Vec<DinoRecord> = query.build_query_as().fetch_all(executor).await?;
    query.reset();

    // FIXME: there's probably a better way to get this but this will do for now
    query.push("SELECT COUNT(*), TOTAL(worth) FROM Dino ");
    kind.push_to_query(&mut query, user_id);

    let row = query.build().fetch_one(executor).await?;
    let dino_count = row.get(0);
    let transaction_count: f64 = row.get(1);

    Ok(DinoCollection {
        dino_count,
        transaction_count: transaction_count as i64,
        dinos,
    })
}

async fn get_dino_record(
    executor: impl SqliteExecutor<'_>,
    dino_name: &str,
) -> Result<Option<DinoRecord>> {
    let row = sqlx::query_as!(DinoRecord, "SELECT * FROM Dino WHERE name = ?", dino_name)
        .fetch_optional(executor)
        .await?;

    Ok(row)
}

async fn update_dino_name(db: &SqlitePool, dino_id: i64, new_name: &str) -> Result<()> {
    sqlx::query!(
        "UPDATE OR ABORT Dino SET name = ? WHERE id = ?",
        new_name,
        dino_id
    )
    .execute(db)
    .await?;

    Ok(())
}

async fn send_dino_embed(
    ctx: Context<'_>,
    dino: &DinoRecord,
    owner_name: &str,
    owner_avatar: &str,
    image_path: &Path,
    created_at: NaiveDateTime,
) -> Result<String> {
    // let mut row = CreateActionRow::default();
    let covet = CreateButton::new(format!("{COVET_BUTTON}:{}", dino.id))
        .emoji('👍')
        .label("Covet".to_string())
        .style(ButtonStyle::Success);
    let shun = CreateButton::new(format!("{SHUN_BUTTON}:{}", dino.id))
        .emoji('👎')
        .label("Shun".to_string())
        .style(ButtonStyle::Danger);
    let favourite = CreateButton::new(format!("{FAVOURITE_BUTTON}:{}", dino.id))
        .emoji('🫶') // heart hands emoji
        .label("Favourite".to_string())
        .style(ButtonStyle::Secondary);

    let image_name = get_file_name(image_path);
    let embed = CreateEmbed::default()
        .colour(0x66ff99)
        .author(CreateEmbedAuthor::new(owner_name).icon_url(owner_avatar))
        .title(&dino.name)
        .description(format!(
            "**Created:** <t:{}>",
            created_at.and_utc().timestamp()
        ))
        .footer(CreateEmbedFooter::new(format!(
            "{} is worth {} Dino Bucks!\nHotness Rating: {}",
            &dino.name,
            dino.worth,
            quirkify_hotness(dino.hotness)
        )))
        .attachment(image_name);

    let reply_handle = ctx
        .send(
            CreateReply::default()
                .components(vec![CreateActionRow::Buttons(vec![covet, shun, favourite])])
                .attachment(CreateAttachment::path(image_path).await?)
                .embed(embed),
        )
        .await?;

    let message_link = reply_handle.message().await?.link();

    Ok(message_link)
}

async fn gift_dino(
    executor: impl SqliteExecutor<'_>,
    dino_id: i64,
    gifter_id: &str,
    recipient_id: &str,
) -> Result<()> {
    sqlx::query!(
        r#"INSERT OR IGNORE INTO DinoUser (id) VALUES (?);
        INSERT INTO DinoTransactions (dino_id, user_id, gifter_id, type)
        VALUES (?, ?, ?, 'GIFT');
        UPDATE Dino SET owner_id = ? WHERE id = ?"#,
        recipient_id,
        dino_id,
        recipient_id,
        gifter_id,
        recipient_id,
        dino_id,
    )
    .execute(executor)
    .await?;

    Ok(())
}

async fn delete_dino(executor: impl SqliteExecutor<'_>, dino_id: i64) -> Result<()> {
    let row = sqlx::query!("DELETE FROM Dino WHERE id = ? RETURNING filename", dino_id)
        .fetch_one(executor)
        .await?;

    let file_path = Path::new(OUTPUT_PATH).join(row.filename);
    if file_path.exists() {
        std::fs::remove_file(file_path)?;
    }

    Ok(())
}

async fn get_non_favourites(
    executor: impl SqliteExecutor<'_>,
    user_id: &str,
) -> Result<Vec<DinoRecord>> {
    let rows = sqlx::query_as!(
        DinoRecord,
        r#"INSERT OR IGNORE INTO DinoUser (id) VALUES (?);
        SELECT * FROM Dino
        WHERE owner_id = ?
        AND Dino.id NOT IN
        (SELECT dino_id FROM DinoTransactions WHERE type = 'FAVOURITE') LIMIT 50"#,
        user_id,
        user_id
    )
    .fetch_all(executor)
    .await?;

    Ok(rows)
}

async fn autocomplete_owned_dinos<'a>(
    ctx: Context<'a>,
    partial: &'a str,
) -> impl Iterator<Item = String> + 'a {
    let owner_id = ctx.author().id.to_string();
    let partial = format!("%{partial}%");

    let suggestions = sqlx::query!(
        "SELECT name FROM Dino WHERE owner_id = ? AND name LIKE ? LIMIT 5",
        owner_id,
        partial
    )
    .fetch_all(&ctx.data().database)
    .await
    .unwrap_or_else(|e| {
        eprintln!("Error while trying to suggest autocomplete for '{partial}': {e}");
        vec![]
    });

    suggestions.into_iter().map(|r| r.name)
}

async fn autocomplete_all_dinos<'a>(
    ctx: Context<'a>,
    partial: &'a str,
) -> impl Iterator<Item = String> + 'a {
    let partial = format!("%{partial}%");

    let suggestions = sqlx::query!("SELECT name FROM Dino WHERE name LIKE ? LIMIT 5", partial)
        .fetch_all(&ctx.data().database)
        .await
        .unwrap_or_else(|e| {
            eprintln!("Error while trying to suggest autocomplete for '{partial}': {e}");
            vec![]
        });

    suggestions.into_iter().map(|r| r.name)
}

async fn roll_to_hatch(ctx: Context<'_>) -> Result<i64> {
    let mut hatch_roll = pick_best_x_dice_rolls(4, 1, 1, None) as i64;

    if let Some(guild_id) = ctx.guild_id() {
        if ctx.author().has_role(ctx, guild_id, SUB_ROLE_ID).await? {
            hatch_roll = hatch_roll.max(pick_best_x_dice_rolls(4, 1, 1, None) as i64);
        }
    }

    Ok(hatch_roll)
}

async fn try_hatching(
    executor: impl SqliteExecutor<'_>,
    ctx: Context<'_>,
    user: &DinoUser,
) -> Result<()> {
    let hatch_roll = roll_to_hatch(ctx).await?;

    if hatch_roll <= (MAX_FAILED_HATCHES - user.record.consecutive_fails) {
        update_last_user_action(
            executor,
            &ctx.author().id.to_string(),
            UserAction::Hatch(user.record.consecutive_fails + 1),
        )
        .await?;

        bail!(
            "You failed to hatch the egg ({} attempt), \
            better luck next time. You can try again <t:{}:R>",
            HATCH_FAILS_TEXT[user.record.consecutive_fails as usize],
            user.timings.next_try
        )
    }

    Ok(())
}

async fn unwrap_fragments(ctx: Context<'_>) -> &RwLock<Fragments> {
    ctx.parent_commands()[0]
        .custom_data
        .downcast_ref::<RwLock<Fragments>>()
        .expect("Expected to have passed a Fragments struct as custom_data")
}

pub fn quirkify_hotness(hotness: i64) -> String {
    format!("{:.3}", f64::tanh(hotness as f64 * 0.1))
}
