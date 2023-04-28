use std::{
    env::temp_dir,
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    time::Duration,
};

use chrono::{NaiveDateTime, Utc};
use image::{
    imageops::overlay, io::Reader, DynamicImage, ImageBuffer, ImageOutputFormat, RgbaImage,
};
use poise::serenity_prelude::{AttachmentType, ButtonStyle, CreateActionRow};
use rand::{seq::SliceRandom, thread_rng};
use sqlx::{Acquire, SqliteConnection, SqlitePool};
use tokio::sync::{RwLock, RwLockReadGuard};

use crate::{
    common::{ephemeral_message, name, pick_best_x_dice_rolls},
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

pub const COVET_BUTTON: &str = "dino-covet";
pub const SHUN_BUTTON: &str = "dino-shun";
pub const FAVOURITE_BUTTON: &str = "dino-favourite";

const HATCH_COOLDOWN: Duration = Duration::from_secs(10);

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
    subcommands("hatch", "collection"),
    custom_data = "setup_dinos()"
)]
pub async fn dino(_ctx: Context<'_>) -> Result<()> {
    Ok(())
}

#[poise::command(slash_command, guild_only)]
async fn hatch(ctx: Context<'_>) -> Result<()> {
    let now = Utc::now().naive_utc();
    let hatch_cooldown_duration = chrono::Duration::from_std(HATCH_COOLDOWN)?;

    let hatcher_record =
        get_user_record(&ctx.data().database, &ctx.author().id.to_string()).await?;
    if hatcher_record.last_hatch + hatch_cooldown_duration > now {
        // TODO: better message
        ephemeral_message(ctx, "Can't hatch yet").await?;
        return Ok(());
    }

    let hatch_roll = pick_best_x_dice_rolls(4, 1, 1, None) as i64;
    // TODO: give twitch subs a reroll

    if hatch_roll <= (MAX_FAILED_HATCHES - hatcher_record.consecutive_fails) {
        update_failed_hatches(&ctx.data().database, ctx.author().id.to_string()).await?;

        let midnight_utc = (now + chrono::Duration::days(1))
            .date()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        ctx.say(format!(
            "You failed to hatch the egg ({} attempt), \
            better luck next time. You can try again <t:{}:R>",
            HATCH_FAILS_TEXT[hatcher_record.consecutive_fails as usize],
            midnight_utc.timestamp()
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
    let image_file_name = image_path.file_name().unwrap().to_str().unwrap();

    let mut conn = ctx.data().database.acquire().await?;
    let mut transaction = conn.begin().await?;

    let dino_id = insert_dino(
        &mut transaction,
        &ctx.author().id.to_string(),
        &parts,
        image_file_name,
    )
    .await?;

    let author_name = name(ctx.author(), &ctx).await;
    let now = Utc::now().timestamp();

    let mut row = CreateActionRow::default();
    row.create_button(|b| {
        b.custom_id(format!("{COVET_BUTTON}:{dino_id}"))
            .emoji('👍')
            .label("Covet".to_string())
            .style(ButtonStyle::Success)
    });
    row.create_button(|b| {
        b.custom_id(format!("{SHUN_BUTTON}:{dino_id}"))
            .emoji('👎')
            .label("Shun".to_string())
            .style(ButtonStyle::Danger)
    });
    row.create_button(|b| {
        b.custom_id(format!("{FAVOURITE_BUTTON}:{dino_id}"))
            .emoji('♥')
            .label("Favourite".to_string())
            .style(ButtonStyle::Primary)
    });

    ctx.send(|message| {
        message
            .components(|c| c.add_action_row(row))
            .attachment(AttachmentType::Path(&image_path))
            .embed(|embed| {
                embed
                    .colour(0x66ff99)
                    .author(|author| author.name(author_name))
                    .title(&parts.name)
                    .description(format!("**Created:** <t:{now}>"))
                    .footer(|f| {
                        f.text(format!(
                            "{} is worth 0 Dino Bucks!\nHotness Rating: 0.00",
                            &parts.name
                        ))
                    })
                    .attachment(image_file_name)
            })
    })
    .await?;

    transaction.commit().await?;

    Ok(())
}

#[poise::command(slash_command, guild_only)]
async fn collection(ctx: Context<'_>, silent: Option<bool>) -> Result<()> {
    let silent = silent.unwrap_or(true);

    let db = &ctx.data().database;
    let dinos = fetch_collection(db, &ctx.author().id.to_string()).await?;
    let image = generate_dino_collection_image(&dinos)?;
    let filename = format!("{}_collection.png", ctx.author().name);

    ctx.send(|message| {
        message
            .attachment(AttachmentType::Bytes {
                data: image.into(),
                filename: filename.clone(),
            })
            .embed(|embed| embed.colour(0x66ff99).attachment(&filename))
            .ephemeral(silent)
    })
    .await?;

    Ok(())
}

async fn update_failed_hatches(db: &SqlitePool, user_id: String) -> Result<()> {
    sqlx::query!(
        "UPDATE DinoUser SET consecutive_fails = consecutive_fails + 1 WHERE id = ?",
        user_id
    )
    .execute(db)
    .await?;

    Ok(())
}

async fn generate_dino(
    db: &SqlitePool,
    fragments: RwLockReadGuard<'_, Fragments>,
) -> Result<Option<DinoParts>> {
    let mut tries = 0;

    loop {
        let generated = choose_parts(&fragments);
        let is_duplicate = are_parts_duplicate(db, &generated).await?;

        if !is_duplicate {
            return Ok(Some(generated));
        }

        tries += 1;
        if tries > MAX_GENERATION_ATTEMPTS {
            return Ok(None);
        }
    }
}

async fn are_parts_duplicate(db: &SqlitePool, parts: &DinoParts) -> Result<bool> {
    let body = get_file_stem(&parts.body);
    let mouth = get_file_stem(&parts.mouth);
    let eyes = get_file_stem(&parts.eyes);
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

    // TODO: add random characters at the end like in the original

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

    // TODO: remember to delete the image when the dino gets deleted
    let mut image: RgbaImage = ImageBuffer::new(width, height);
    for (i, dino) in collection.iter().enumerate() {
        let x = (i as u32 % columns) * (COLUMN_MARGIN + DINO_IMAGE_SIZE);
        let y = (i as f32 / columns as f32).floor() as u32 * (ROW_MARGIN + DINO_IMAGE_SIZE);

        let dino_image = Reader::open(output_path.join(&dino.filename))?.decode()?;
        overlay(&mut image, &dino_image, x.into(), y.into());
    }

    let mut bytes: Vec<u8> = Vec::new();
    image.write_to(&mut Cursor::new(&mut bytes), ImageOutputFormat::Png)?;

    Ok(bytes)
}

struct UserRecord {
    last_hatch: NaiveDateTime,
    consecutive_fails: i64,
}

async fn get_user_record(db: &SqlitePool, user_id: &str) -> Result<UserRecord> {
    let row = sqlx::query_as!(
        UserRecord,
        r#"INSERT OR IGNORE INTO DinoUser (id) VALUES (?);
        SELECT last_hatch, consecutive_fails FROM DinoUser WHERE id = ?"#,
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
    file_name: &str,
) -> Result<i64> {
    let body = get_file_name(&parts.body);
    let mouth = get_file_name(&parts.mouth);
    let eyes = get_file_name(&parts.eyes);

    let row = sqlx::query!(
        r#"INSERT INTO Dino (owner_id, name, filename, created_at, body, mouth, eyes)
        VALUES (?, ?, ?, datetime('now'), ?, ?, ?)
        RETURNING id"#,
        user_id,
        parts.name,
        file_name,
        body,
        mouth,
        eyes
    )
    .fetch_one(&mut *conn)
    .await?;

    sqlx::query!(
        r#"INSERT INTO DinoTransactions (dino_id, user_id, type) VALUES (?, ?, 'HATCH');
        UPDATE DinoUser SET last_hatch = datetime('now'), consecutive_fails = 0 WHERE id = ?"#,
        row.id,
        user_id,
        user_id
    )
    .execute(&mut *conn)
    .await?;

    Ok(row.id)
}

struct DinoRecord {
    filename: String,
}

async fn fetch_collection(db: &SqlitePool, user_id: &str) -> Result<Vec<DinoRecord>> {
    let rows = sqlx::query_as!(
        DinoRecord,
        r#"INSERT OR IGNORE INTO DinoUser (id) VALUES (?);
        SELECT filename FROM Dino WHERE owner_id = ? LIMIT 25"#,
        user_id,
        user_id
    )
    .fetch_all(db)
    .await?;

    Ok(rows)
}
