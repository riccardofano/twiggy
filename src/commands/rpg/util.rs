use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rand_seeder::Seeder;

pub fn pick_best_x_dice_rolls(
    die_sides: usize,
    total_rolls: usize,
    x: usize,
    seed: Option<&str>,
) -> usize {
    let mut rng = match seed {
        Some(s) => Seeder::from(&s).make_rng(),
        None => StdRng::seed_from_u64(rand::random::<u64>()),
    };

    let mut rolls = (0..total_rolls)
        .map(|_| rng.gen_range(1..=die_sides))
        .collect::<Vec<usize>>();
    rolls.sort();

    rolls.iter().rev().take(x).sum()
}
