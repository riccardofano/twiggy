use std::collections::HashMap;
use std::fmt::Display;

use poise::serenity_prelude::CreateEmbed;
use rand::SeedableRng;
use rand::{rngs::StdRng, seq::SliceRandom};
use rand_seeder::Seeder;

use crate::commands::rpg::data::{CLASSES, STANDARD_SPECIES};
use crate::common::pick_best_x_dice_rolls;

use super::data::{Class, Specie, Stat, ADJECTIVES, BANANA_SPECIE, NOUNS};

const BANANA_ID: u64 = 1234567;

const HIT_DICE_SIDES: usize = 6;
const HIT_DICE_POOL: usize = 10;
const HIT_DICE: usize = 4;

const DEFAULT_STATS: [(Stat, u8); 6] = [
    (Stat::CHR, 0),
    (Stat::CON, 0),
    (Stat::DEX, 0),
    (Stat::INT, 0),
    (Stat::STR, 0),
    (Stat::WIS, 0),
];

pub struct Character {
    pub hp: isize,
    pub max_hp: isize,
    pub name: String,
    stats: HashMap<Stat, usize>,

    class: &'static Class,
    specie: &'static Specie,
    alignment: String,
    move_choices: Vec<Stat>,
}

impl Character {
    pub fn new(user_id: u64, name: &str, seed: &Option<&str>) -> Self {
        let mut rng = match seed {
            Some(s) => Seeder::from(&s).make_rng(),
            None => StdRng::seed_from_u64(rand::random::<u64>()),
        };
        let class = CLASSES
            .choose(&mut rng)
            .expect("Expected the class array to not be empty");
        let specie = if user_id == BANANA_ID {
            BANANA_SPECIE
        } else {
            STANDARD_SPECIES
                .choose(&mut rng)
                .expect("Expected the specie array to not be empty")
        };
        let adjective = ADJECTIVES
            .choose(&mut rng)
            .expect("Expected the adjective array to not be empty");
        let noun = NOUNS
            .choose(&mut rng)
            .expect("Expected the noun array to not be empty");

        let alignment = format!("{adjective} {noun}");

        let len = class.stat_preferences.len();
        let mut move_choices = Vec::with_capacity(len * (len + 1) / 2);
        for i in 0..class.stat_preferences.len() {
            for _ in 0..(class.stat_preferences.len() - i) {
                move_choices.push(class.stat_preferences[i])
            }
        }

        let stats = HashMap::from(DEFAULT_STATS);
        let mut stats: HashMap<Stat, usize> = stats
            .into_keys()
            .map(|k| (k, pick_best_x_dice_rolls(6, 3, 3, *seed)))
            .collect();
        for stat in specie.stat_bonuses.iter() {
            *stats.get_mut(stat).expect("Expected to have all the stats") += 1;
        }

        let max_hp =
            pick_best_x_dice_rolls(HIT_DICE_SIDES, HIT_DICE_POOL, HIT_DICE, *seed) as isize;

        Self {
            hp: max_hp,
            max_hp,
            name: name.to_string(),
            stats,
            move_choices,
            class,
            specie,
            alignment,
        }
    }

    pub fn random_move_stat(&self) -> Stat {
        let mut rng = rand::thread_rng();
        *self
            .move_choices
            .choose(&mut rng)
            .expect("Expected to have at least 1 move choice")
    }

    pub fn get_modifier(&self, stat: &Stat) -> usize {
        self.stats[stat] / 2 - 5
    }

    pub fn to_embed<'a, 'b>(&'a self, builder: &'b mut CreateEmbed) -> &'b mut CreateEmbed {
        builder
            .color(0x0099333)
            .title(&self.name)
            .description(format!(
                "{info}\n```{stats}```",
                info = self.display_info(),
                stats = self.display_stats()
            ));
        builder
    }

    fn display_info(&self) -> String {
        format!(
            "{specie} {class}\nAlignment: {alignment}\nHP: {hp}",
            specie = self.specie.name,
            class = self.class.name,
            alignment = self.alignment,
            hp = self.max_hp,
        )
    }

    fn display_stats(&self) -> String {
        format!(
            r#"STR | DEX | CON | INT | WIS | CHR
{str: >3} | {dex: >3} | {con: >3} | {int: >3} | {wis: >3} | {chr: >3}"#,
            // `{something: >3}` means pad the variable `something` with spaces so the word is always 3 wide
            str = self.stats[&Stat::STR],
            dex = self.stats[&Stat::DEX],
            con = self.stats[&Stat::CON],
            int = self.stats[&Stat::INT],
            wis = self.stats[&Stat::WIS],
            chr = self.stats[&Stat::CHR],
        )
    }
}

impl Display for Character {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{name}\n{divider}\n{info}\n{stats}",
            name = self.name,
            divider = "-".repeat(self.name.len()),
            info = self.display_info(),
            stats = self.display_stats()
        )
    }
}
