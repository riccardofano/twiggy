use std::fmt::Display;

use rand::seq::SliceRandom;

use crate::common::Score;

pub const RANK_CHANGE_FACTOR: f64 = 56.;

pub struct LadderRank {
    pub upper_bound: i64,
    pub icon: &'static str,
    pub name: &'static str,
}

#[derive(Copy, Clone)]
pub enum LadderPosition {
    Top,
    Tail,
    Wins,
    Losses,
}

impl LadderPosition {
    pub fn suffix(&self) -> String {
        match self {
            Self::Top => "LP",
            Self::Tail => "LP",
            Self::Wins => "wins",
            Self::Losses => "losses",
        }
        .to_string()
    }

    pub fn random_text(&self) -> &'static str {
        let mut rng = rand::thread_rng();
        let texts = LADDER_TEXTS[*self as usize];
        texts
            .choose(&mut rng)
            .expect("Expected to have at least one ladder text for each option")
    }
}

impl From<LadderPosition> for String {
    fn from(val: LadderPosition) -> Self {
        val.to_string()
    }
}

impl Display for LadderPosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Top => "Top",
                Self::Tail => "Tail",
                Self::Wins => "Wins",
                Self::Losses => "Losses",
            }
        )
    }
}

pub const RANKS: &[LadderRank] = &[
    LadderRank {
        upper_bound: 700,
        icon: "🪵",
        name: "Wood",
    },
    LadderRank {
        upper_bound: 800,
        icon: "🥉",
        name: "Bronze",
    },
    LadderRank {
        upper_bound: 900,
        icon: "🥈",
        name: "Silver",
    },
    LadderRank {
        upper_bound: 1100,
        icon: "🥇",
        name: "Gold",
    },
    LadderRank {
        upper_bound: 1200,
        icon: "💎",
        name: "Diamond",
    },
    LadderRank {
        upper_bound: 1300,
        icon: "🎀",
        name: "Master",
    },
    LadderRank {
        upper_bound: i64::MAX,
        icon: "🏆",
        name: "Grand Master",
    },
];

const LADDER_TEXTS: &[&[&str]] = &[
    &[
        "is the champion",
        "is the big cheese",
        "is top banana",
        "is supreme ruler",
        "is the coolest chatter",
        "is the gout gamer",
        "is based and RPG pilled",
        "probably cheated",
        "is the raid boss",
        "is on top",
    ],
    &[
        "is everyone's best friend",
        "had their lunch money taken",
        "has the best personality",
        "is making the room brighter",
        "can't seem to catch a break",
        "is a sweet summer child",
        "gave peace a chance",
    ],
    &[
        "has the most bedpost notches",
        "has the biggest tally",
        "sits on a throne of skulls",
        "has been winning a lot",
    ],
    &[
        "has the worst luck",
        "can't catch a break",
        "needs to work on their technique",
        "has found inner peace",
        "will turn it around soon",
        "is a victim of variance",
        "has taken the most Ls",
    ],
];

pub fn find_ladder_rank(elo: i64) -> &'static LadderRank {
    let mut i = 0;
    loop {
        if RANKS[i].upper_bound > elo {
            return &RANKS[i];
        }
        i += 1;
    }
}

pub fn calculate_lp_difference(old_elo: i64, new_elo: i64) -> String {
    let elo_difference = new_elo - old_elo;
    let rank = find_ladder_rank(new_elo);

    if elo_difference > 0 {
        format!("{} gained {}LP", rank.icon, elo_difference)
    } else {
        format!("{} lost {}LP", rank.icon, -elo_difference)
    }
}

pub fn calculate_new_elo(player_rank: i64, opponent_rank: i64, outcome: &Score) -> i64 {
    let base: f64 = 10.;
    let exponent = 1. / 400.;
    let expected = 1. / (1. + base.powf(exponent * (opponent_rank - player_rank) as f64));

    let score = match outcome {
        Score::Win => 1.,
        Score::Loss => 0.,
        Score::Draw => 0.5,
    };

    player_rank + (RANK_CHANGE_FACTOR * (score - expected)).round() as i64
}
