use crate::common::Score;

pub const RANK_CHANGE_FACTOR: f64 = 56.;

pub struct LadderRank {
    pub upper_bound: i64,
    pub icon: &'static str,
    pub name: &'static str,
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
