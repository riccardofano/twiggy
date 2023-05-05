use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
};

use chrono::{NaiveDateTime, Utc};
use image::{imageops::overlay, io::Reader, ImageBuffer, ImageOutputFormat, RgbaImage};
use poise::serenity_prelude::{AttachmentType, ButtonStyle, CreateActionRow, User, UserId};
use rand::{seq::SliceRandom, thread_rng};
use sqlx::error::DatabaseError;
use sqlx::sqlite::SqliteError;
use sqlx::{FromRow, QueryBuilder, Row, Sqlite, SqliteConnection, SqlitePool};
use tokio::sync::{RwLock, RwLockReadGuard};

use crate::{
    common::{avatar_url, ephemeral_message, name as get_name, pick_best_x_dice_rolls},
    Context, Result,
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

const GIFTING_COOLDOWN: Duration = Duration::from_secs(60 * 60);
const SLURP_COOLDOWN: Duration = Duration::from_secs(60 * 60);

pub const COVET_BUTTON: &str = "dino-covet";
pub const SHUN_BUTTON: &str = "dino-shun";
pub const FAVOURITE_BUTTON: &str = "dino-favourite";

fn setup_dinos() -> RwLock<Fragments> {
    let fragments_dir = fs::read_dir(FRAGMENT_PATH).expect("Could not read fragment path");

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
    subcommands("hatch", "collection", "rename", "view", "gift", "slurp"),
    custom_data = "setup_dinos()"
)]
pub async fn dino(_ctx: Context<'_>) -> Result<()> {
    Ok(())
}

/// Attempt to hatch a new dino.
#[poise::command(slash_command, guild_only)]
async fn hatch(ctx: Context<'_>) -> Result<()> {
    let now = Utc::now().naive_utc();
    let midnight_utc = now.date().and_hms_opt(0, 0, 0).unwrap();
    let midnight_utc_tomorrow = midnight_utc + chrono::Duration::days(1);

    let author_id = ctx.author().id.to_string();
    let hatcher_record = get_user_record(&ctx.data().database, &author_id).await?;

    if hatcher_record.last_hatch > midnight_utc {
        ephemeral_message(
            ctx,
            format!(
                "Dont be greedy! You can hatch again <t:{}:R>.",
                midnight_utc_tomorrow.timestamp()
            ),
        )
        .await?;
        return Ok(());
    }

    let hatch_roll = pick_best_x_dice_rolls(4, 1, 1, None) as i64;
    // TODO: give twitch subs a reroll

    let mut conn = ctx.data().database.acquire().await?;
    if hatch_roll <= (MAX_FAILED_HATCHES - hatcher_record.consecutive_fails) {
        update_last_user_action(
            &mut conn,
            &author_id,
            UserAction::Hatch(hatcher_record.consecutive_fails + 1),
        )
        .await?;

        ctx.say(format!(
            "You failed to hatch the egg ({} attempt), \
            better luck next time. You can try again <t:{}:R>",
            HATCH_FAILS_TEXT[hatcher_record.consecutive_fails as usize],
            midnight_utc_tomorrow.timestamp()
        ))
        .await?;

        return Ok(());
    }

    let custom_data_lock = ctx.parent_commands()[0]
        .custom_data
        .downcast_ref::<RwLock<Fragments>>()
        .expect("Expected to have passed a Fragments struct as custom_data");

    let fragments = custom_data_lock.read().await;
    let parts = generate_dino(&ctx.data().database, fragments).await?;

    if parts.is_none() {
        ephemeral_message(
            ctx,
            "I tried really hard but i wasn't able to make a unique dino for you. Sorry... :'(",
        )
        .await?;
        return Ok(());
    }

    let parts = parts.unwrap();
    let image_path = generate_dino_image(&parts)?;

    let mut transaction = ctx.data().database.begin().await?;

    let dino = insert_dino(&mut transaction, &author_id, &parts, &image_path).await?;
    update_last_user_action(&mut transaction, &author_id, UserAction::Hatch(0)).await?;

    let author_name = get_name(ctx.author(), &ctx).await;
    let message = send_dino_embed(
        ctx,
        &dino,
        &author_name,
        &avatar_url(ctx.author()),
        &image_path,
        now,
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
    let user = user.unwrap_or_else(|| ctx.author().clone());

    let db = &ctx.data().database;
    let dino_collection = fetch_collection(db, &user.id.to_string(), kind).await?;

    if dino_collection.dinos.is_empty() {
        let content = match user_is_author {
            true => "You don't have any dinos :'(".to_string(),
            false => format!("{} doesn't have any dinos :'(", get_name(&user, &ctx).await),
        };
        ephemeral_message(ctx, content).await?;
        return Ok(());
    }

    let image = generate_dino_collection_image(&dino_collection.dinos)?;
    let filename = format!("{}_collection.png", user.name);

    let others_count = dino_collection.dino_count - dino_collection.dinos.len() as i64;
    let dino_names = dino_collection
        .dinos
        .iter()
        .map(|d| d.name.as_ref())
        .collect::<Vec<&str>>()
        .join(", ");

    let description = if others_count == 1 {
        format!("{} and one more!", &dino_names)
    } else if others_count > 0 {
        format!("{} and {} others!", &dino_names, &others_count)
    } else {
        format!("{dino_names}!")
    };
    let dino_count = if dino_collection.dino_count == 1 {
        "1 Dino".to_string()
    } else {
        format!("{} Dinos", dino_collection.dino_count)
    };

    let author_name = get_name(&user, &ctx).await;

    ctx.send(|message| {
        message
            .embed(|embed| {
                embed
                    .colour(0xffbf00)
                    .author(|author| author.name(&author_name).icon_url(avatar_url(&user)))
                    .title(format!("{}'s collection", &author_name))
                    .description(description)
                    .footer(|f| {
                        f.text(format!(
                            "{}. They are worth: {} Bucks",
                            dino_count, dino_collection.transaction_count
                        ))
                    })
                    .attachment(&filename)
            })
            .attachment(AttachmentType::Bytes {
                data: image.into(),
                filename,
            })
            .ephemeral(silent)
    })
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
        ephemeral_message(ctx, "The name of the dino you specified was not found.").await?;
        return Ok(());
    };

    if dino.owner_id != ctx.author().id.to_string().as_ref() {
        ephemeral_message(ctx, "You don't own this dino, you can't rename it.").await?;
        return Ok(());
    }

    if let Err(e) = update_dino_name(&ctx.data().database, dino.id, &replacement).await {
        if let Some(sqlite_error) = e.downcast_ref::<SqliteError>() {
            // NOTE: 2067 is the code for a UNIQUE constraint error in Sqlite
            // https://www.sqlite.org/rescode.html#constraint_unique
            if sqlite_error.code() == Some("2067".into()) {
                ephemeral_message(ctx, "This name is already taken!").await?;
                return Ok(());
            }
        };
        return Err(e);
    }

    ephemeral_message(
        ctx,
        format!(
            "**{}** name has been update to **{}**!",
            dino.name, replacement
        ),
    )
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
        ephemeral_message(ctx, "The name of the dino you specified was not found.").await?;
        return Ok(());
    };

    let owner_user_id = UserId::from_str(&dino.owner_id)?;
    let (user_name, user_avatar) = match owner_user_id.to_user(&ctx).await {
        Ok(user) => (get_name(&user, &ctx).await, avatar_url(&user)),
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
    #[description = "The person who will recieve the dino"] recipient: User,
) -> Result<()> {
    let user_record = get_user_record(&ctx.data().database, &ctx.author().id.to_string()).await?;

    let now = Utc::now().naive_utc();
    let gifting_cooldown_duration = chrono::Duration::from_std(GIFTING_COOLDOWN)?;
    let time_until_next_gift = user_record.last_gifting + gifting_cooldown_duration;

    if time_until_next_gift > now {
        ephemeral_message(
            ctx,
            format!(
                "You're too kind, you're gifting too often. You can gift again <t:{}:R>",
                time_until_next_gift.timestamp()
            ),
        )
        .await?;
        return Ok(());
    }

    let Some(dino_record) = get_dino_record(&ctx.data().database, &dino).await? else {
        ephemeral_message(ctx, format!( "Could not find a dino named {dino}.")).await?;
        return Ok(());
    };

    if dino_record.owner_id != ctx.author().id.to_string().as_ref() {
        ephemeral_message(ctx, "You cannot gift a dino you don't own.").await?;
        return Ok(());
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

    let sender_name = get_name(ctx.author(), &ctx).await;
    let receiver_name = get_name(&recipient, &ctx).await;
    let dino_name = if dino_record.hatch_message.is_empty() {
        dino
    } else {
        format!("[{}]({})", dino, dino_record.hatch_message)
    };

    ctx.send(|message| {
        message.embed(|embed| {
            embed.colour(0x990933).description(&format!(
                "**{sender_name}** gifted {dino_name} to **{receiver_name}**! How kind!",
            ))
        })
    })
    .await?;

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
        ephemeral_message(ctx, "You can't slurp the same dino twice, you cheater!").await?;
        return Ok(());
    }

    let user_record = get_user_record(&ctx.data().database, &ctx.author().id.to_string()).await?;

    let now = Utc::now().naive_utc();
    let slurp_cooldown_duration = chrono::Duration::from_std(SLURP_COOLDOWN)?;
    let time_until_next_slurp = user_record.last_slurp + slurp_cooldown_duration;

    if time_until_next_slurp > now {
        ephemeral_message(
            ctx,
            format!(
                "Don't be greedy! You can slurp again <t:{}:R>",
                time_until_next_slurp.timestamp()
            ),
        )
        .await?;
        return Ok(());
    }

    let Some(first_dino) = get_dino_record(&ctx.data().database, &first).await? else {
        ephemeral_message(ctx, &format!("Could not find a dino named {first}.")).await?;
        return Ok(());
    };

    let author_id = ctx.author().id.to_string();

    if first_dino.owner_id != author_id {
        ephemeral_message(
            ctx,
            &format!("Doesn't seem you own {first}, are you trying to pull a fast one on me?!"),
        )
        .await?;
        return Ok(());
    }

    let Some(second_dino) = get_dino_record(&ctx.data().database, &second).await? else {
        ephemeral_message(ctx, &format!("Could not find a dino named {second}.")).await?;
        return Ok(());
    };

    if second_dino.owner_id != author_id {
        ephemeral_message(
            ctx,
            &format!("Doesn't seem you own {second}, are you trying to pull a fast one on me?!"),
        )
        .await?;
        return Ok(());
    }
    let custom_data_lock = ctx.parent_commands()[0]
        .custom_data
        .downcast_ref::<RwLock<Fragments>>()
        .expect("Expected to have passed a Fragments struct as custom_data");

    let fragments = custom_data_lock.read().await;
    let parts = generate_dino(&ctx.data().database, fragments).await?;

    if parts.is_none() {
        ephemeral_message(
            ctx,
            "I tried really hard but i wasn't able to make a unique dino for you. Sorry... :'(",
        )
        .await?;
        return Ok(());
    }

    let mut transaction = ctx.data().database.begin().await?;
    delete_dino(&mut transaction, first_dino.id).await?;
    delete_dino(&mut transaction, second_dino.id).await?;

    let parts = parts.unwrap();
    let image_path = generate_dino_image(&parts)?;

    let dino = insert_dino(&mut transaction, &author_id, &parts, &image_path).await?;
    update_last_user_action(&mut transaction, &author_id, UserAction::Slurp).await?;

    let author_name = get_name(ctx.author(), &ctx).await;
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

enum UserAction {
    Hatch(i64),
    Slurp,
    Gift,
}

impl UserAction {
    fn to_update_query(&self) -> String {
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
    conn: &mut SqliteConnection,
    user_id: &str,
    action: UserAction,
) -> Result<()> {
    let mut query = QueryBuilder::new(format!(
        "UPDATE DinoUser SET {} WHERE id = ",
        action.to_update_query()
    ));
    query.push_bind(user_id);

    query.build().execute(conn).await?;

    Ok(())
}

async fn generate_dino(
    db: &SqlitePool,
    fragments: RwLockReadGuard<'_, Fragments>,
) -> Result<Option<DinoParts>> {
    let mut tries = 0;

    loop {
        let mut generated = choose_parts(&fragments);
        let duplicate_parts = are_parts_duplicate(db, &generated).await?;

        if !duplicate_parts {
            loop {
                let duplicate_name = is_name_duplicate(db, &generated).await?;
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

async fn are_parts_duplicate(db: &SqlitePool, parts: &DinoParts) -> Result<bool> {
    let body = get_file_name(&parts.body);
    let mouth = get_file_name(&parts.mouth);
    let eyes = get_file_name(&parts.eyes);
    let row = sqlx::query!(
        "SELECT id FROM Dino WHERE body = ? AND mouth = ? AND eyes = ?",
        body,
        mouth,
        eyes
    )
    .fetch_optional(db)
    .await?;

    Ok(row.is_some())
}

async fn is_name_duplicate(db: &SqlitePool, parts: &DinoParts) -> Result<bool> {
    let row = sqlx::query!("SELECT id FROM Dino WHERE name = ?", parts.name)
        .fetch_optional(db)
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

struct UserRecord {
    last_hatch: NaiveDateTime,
    last_slurp: NaiveDateTime,
    last_gifting: NaiveDateTime,
    consecutive_fails: i64,
}

async fn get_user_record(db: &SqlitePool, user_id: &str) -> Result<UserRecord> {
    let row = sqlx::query_as!(
        UserRecord,
        r#"INSERT OR IGNORE INTO DinoUser (id) VALUES (?);
        SELECT last_hatch, last_slurp, last_gifting, consecutive_fails FROM DinoUser WHERE id = ?"#,
        user_id,
        user_id,
    )
    .fetch_one(db)
    .await?;

    Ok(row)
}

async fn insert_dino(
    conn: &mut SqliteConnection,
    user_id: &str,
    parts: &DinoParts,
    file_path: &Path,
) -> Result<DinoRecord> {
    let body = get_file_name(&parts.body);
    let mouth = get_file_name(&parts.mouth);
    let eyes = get_file_name(&parts.eyes);
    let file_name = get_file_name(file_path);

    // NOTE: `query_as!` mistakenly interprets all string type fields as
    // nullable strings (when every field is marked NOT NULL), using
    // `query_as_unchecked!` until that gets fixed.
    let row = sqlx::query_as_unchecked!(
        DinoRecord,
        r#"INSERT INTO Dino
        (owner_id, name, filename, created_at, body, mouth, eyes)
        VALUES (?, ?, ?, datetime('now'), ?, ?, ?)
        RETURNING *"#,
        user_id,
        parts.name,
        file_name,
        body,
        mouth,
        eyes,
    )
    .fetch_one(conn)
    .await?;

    Ok(row)
}

async fn update_hatch_message(
    conn: &mut SqliteConnection,
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
    .execute(conn)
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
    db: &SqlitePool,
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

    let dinos: Vec<DinoRecord> = query.build_query_as().fetch_all(db).await?;
    query.reset();

    // FIXME: there's probably a better way to get this but this will do for now
    query.push("SELECT COUNT(*), TOTAL(worth) FROM Dino ");
    kind.push_to_query(&mut query, user_id);

    let row = query.build().fetch_one(db).await?;
    let dino_count = row.get(0);
    let transaction_count: f64 = row.get(1);

    Ok(DinoCollection {
        dino_count,
        transaction_count: transaction_count as i64,
        dinos,
    })
}

async fn get_dino_record(db: &SqlitePool, dino_name: &str) -> Result<Option<DinoRecord>> {
    let row = sqlx::query_as!(DinoRecord, "SELECT * FROM Dino WHERE name = ?", dino_name)
        .fetch_optional(db)
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
    let mut row = CreateActionRow::default();
    row.create_button(|b| {
        b.custom_id(format!("{COVET_BUTTON}:{}", dino.id))
            .emoji('👍')
            .label("Covet".to_string())
            .style(ButtonStyle::Success)
    });
    row.create_button(|b| {
        b.custom_id(format!("{SHUN_BUTTON}:{}", dino.id))
            .emoji('👎')
            .label("Shun".to_string())
            .style(ButtonStyle::Danger)
    });
    row.create_button(|b| {
        b.custom_id(format!("{FAVOURITE_BUTTON}:{}", dino.id))
            .emoji('🫶') // heart hands emoji
            .label("Favourite".to_string())
            .style(ButtonStyle::Secondary)
    });

    let image_name = get_file_name(image_path);

    let reply_handle = ctx
        .send(|message| {
            message
                .components(|c| c.add_action_row(row))
                .attachment(AttachmentType::Path(image_path))
                .embed(|embed| {
                    embed
                        .colour(0x66ff99)
                        .author(|author| author.name(owner_name).icon_url(owner_avatar))
                        .title(&dino.name)
                        .description(format!("**Created:** <t:{}>", created_at.timestamp()))
                        .footer(|f| {
                            f.text(format!(
                                "{} is worth {} Dino Bucks!\nHotness Rating: {}",
                                &dino.name, dino.worth, dino.hotness
                            ))
                        })
                        .attachment(image_name)
                })
        })
        .await?;

    let message_link = reply_handle.message().await?.link();

    Ok(message_link)
}

async fn gift_dino(
    conn: &mut SqliteConnection,
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
    .execute(conn)
    .await?;

    Ok(())
}

async fn delete_dino(conn: &mut SqliteConnection, dino_id: i64) -> Result<()> {
    let row = sqlx::query!("DELETE FROM Dino WHERE id = ? RETURNING filename", dino_id)
        .fetch_one(conn)
        .await?;

    let file_path = Path::new(OUTPUT_PATH).join(row.filename);
    if file_path.exists() {
        fs::remove_file(file_path)?;
    }

    Ok(())
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
