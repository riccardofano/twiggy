use crate::common::Score;

pub const RANK_CHANGE_FACTOR: f64 = 56.;

pub struct LadderRank {
    upper_bound: i64,
    icon: &'static str,
    _name: &'static str,
}

pub const RANKS: &[LadderRank] = &[
    LadderRank {
        upper_bound: 700,
        icon: "🪵",
        _name: "Wood",
    },
    LadderRank {
        upper_bound: 800,
        icon: "🥉",
        _name: "Bronze",
    },
    LadderRank {
        upper_bound: 900,
        icon: "🥈",
        _name: "Silver",
    },
    LadderRank {
        upper_bound: 1100,
        icon: "🥇",
        _name: "Gold",
    },
    LadderRank {
        upper_bound: 1200,
        icon: "💎",
        _name: "Diamond",
    },
    LadderRank {
        upper_bound: 1300,
        icon: "🎀",
        _name: "Master",
    },
    LadderRank {
        upper_bound: i64::MAX,
        icon: "🏆",
        _name: "Grand Master",
    },
];

pub fn calculate_lp_difference(old_elo: i64, new_elo: i64) -> String {
    let elo_difference = new_elo - old_elo;

    let mut i = 0;
    let icon = loop {
        if RANKS[i].upper_bound > new_elo {
            break RANKS[i].icon;
        }
        i += 1;
    };

    if elo_difference > 0 {
        format!("{icon} gained {}LP", elo_difference)
    } else {
        format!("{icon} lost {}LP", -elo_difference)
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
