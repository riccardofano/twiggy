use std::{env::temp_dir, fs, path::PathBuf};

use image::{imageops::overlay, io::Reader};
use poise::serenity_prelude::AttachmentType;
use rand::{seq::SliceRandom, thread_rng};
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
const MAX_GENERATION_ATTEMPTS: usize = 20;

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
    subcommands("hatch"),
    custom_data = "setup_dinos()"
)]
pub async fn dino(_ctx: Context<'_>) -> Result<()> {
    Ok(())
}

#[poise::command(slash_command, guild_only)]
async fn hatch(ctx: Context<'_>) -> Result<()> {
    let hatch_roll = pick_best_x_dice_rolls(4, 1, 1, None);
    // TODO: give twitch subs a reroll

    let custom_data_lock = ctx.parent_commands()[0]
        .custom_data
        .downcast_ref::<RwLock<Fragments>>()
        .expect("Expected to have passed a ChallengeData struct as custom_data");

    let fragments = custom_data_lock.read().await;
    let parts = generate_dino(fragments).await;

    if parts.is_none() {
        ephemeral_message(
            ctx,
            "I tried really hard but i wasn't able to make a unique dino for you. Sorry... :'(",
        )
        .await?;
    }

    let parts = parts.unwrap();
    let image_path = generate_dino_image(&parts)?;
    let image_file_name = image_path.file_name().unwrap().to_str().unwrap();

    let name = name(ctx.author(), &ctx).await;
    if let Err(e) = ctx
        .send(|m| {
            m.attachment(AttachmentType::Path(&image_path)).embed(|e| {
                e.colour(0x66ff99)
                    .author(|a| a.name(name))
                    .title(parts.name)
                    .attachment(image_file_name)
            })
        })
        .await
    {
        eprintln!("{:?}", e)
    };

    Ok(())
}

async fn generate_dino(fragments: RwLockReadGuard<'_, Fragments>) -> Option<DinoParts> {
    let mut tries = 0;

    loop {
        let generated = choose_parts(&fragments);

        // TODO: check if it's a duplicate
        if true {
            break Some(generated);
        }

        tries += 1;
        if tries > MAX_GENERATION_ATTEMPTS {
            return None;
        }
    }
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

    let name = format!(
        "{}_{}_{}",
        body.file_stem().unwrap().to_str().unwrap(),
        mouth.file_stem().unwrap().to_str().unwrap(),
        eyes.file_stem().unwrap().to_str().unwrap()
    );

    DinoParts {
        body,
        mouth,
        eyes,
        name,
    }
}

fn generate_dino_image(parts: &DinoParts) -> Result<PathBuf> {
    let mut body = Reader::open(&parts.body)
        .expect("Could not open file")
        .decode()
        .expect("Could not decode file");
    let mouth = Reader::open(&parts.mouth)?.decode()?;
    let eyes = Reader::open(&parts.eyes)?.decode()?;

    overlay(&mut body, &mouth, 0, 0);
    overlay(&mut body, &eyes, 0, 0);

    let path = temp_dir().join(&parts.name).with_extension("png");
    dbg!(&path);
    body.save(&path)?;

    Ok(path)
}
