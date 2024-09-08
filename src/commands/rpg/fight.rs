use super::character::Character;
use super::data::{Stat, VictoryKind};

use crate::common::{pick_best_x_dice_rolls, Score};

use rand::seq::SliceRandom;
use std::cmp;
use std::fmt::Display;

const OUTPUT_WIDTH: usize = 24;
const MAX_ROUNDS: usize = 10;

pub enum FightResult {
    ChallengerWin,
    AccepterWin,
    Draw,
}

impl FightResult {
    pub fn to_score(&self, is_challenger: bool) -> Score {
        match (self, is_challenger) {
            (FightResult::ChallengerWin, true) => Score::Win,
            (FightResult::ChallengerWin, false) => Score::Loss,
            (FightResult::AccepterWin, true) => Score::Loss,
            (FightResult::AccepterWin, false) => Score::Win,
            _ => Score::Draw,
        }
    }
}

pub struct RPGFight {
    pub challenger: Character,
    pub accepter: Character,
    pub log: String,
    pub summary: String,
}

impl RPGFight {
    pub fn new(challenger: Character, accepter: Character) -> Self {
        Self {
            challenger,
            accepter,
            log: String::new(),
            summary: String::new(),
        }
    }

    pub fn fight(&mut self) -> FightResult {
        let mut rounds = 0;

        while self.challenger.hp > 0 && self.accepter.hp > 0 && rounds < MAX_ROUNDS {
            let challenger_initiative = pick_best_x_dice_rolls(20, 1, 1, None)
                + self.challenger.get_modifier(&Stat::DEX)
                - self.challenger.get_modifier(&Stat::CHR);

            let accepter_initiative = pick_best_x_dice_rolls(20, 1, 1, None)
                + self.accepter.get_modifier(&Stat::DEX)
                - self.accepter.get_modifier(&Stat::CHR);

            let mut is_challenger_first = challenger_initiative > accepter_initiative;
            for _ in 0..2 {
                let is_fight_over = self.play_turn(is_challenger_first);
                if is_fight_over {
                    break;
                }

                is_challenger_first = !is_challenger_first;
            }
            rounds += 1;
        }

        let (result, victor, loser) = if self.accepter.hp == 0 {
            (FightResult::ChallengerWin, &self.challenger, &self.accepter)
        } else if self.challenger.hp == 0 {
            (FightResult::AccepterWin, &self.accepter, &self.challenger)
        } else {
            self.summary = format!("After {MAX_ROUNDS} rounds they decide to call it a draw.");
            return FightResult::Draw;
        };

        self.log += "\n";
        let result_texts = if victor.hp == victor.max_hp {
            VictoryKind::Perfect.get_texts()
        } else if victor.hp < 5 {
            VictoryKind::Close.get_texts()
        } else {
            VictoryKind::Standard.get_texts()
        };

        let mut rng = rand::thread_rng();
        self.summary = result_texts
            .choose(&mut rng)
            .expect("Expected to have at least one result text")
            .replace("VICTOR", &format!("**{}**", victor.name))
            .replace("LOSER", &format!("**{}**", loser.name));

        result
    }

    fn play_turn(&mut self, challenger_is_attacker: bool) -> bool {
        let (attacker, defender) = if challenger_is_attacker {
            (&mut self.challenger, &mut self.accepter)
        } else {
            (&mut self.accepter, &mut self.challenger)
        };

        let attack_stat = attacker.random_move_stat();
        let defence_stat = defender.random_move_stat();

        let attack_reroll = attack_stat.has_advantage(&defence_stat) as usize;
        let defence_reroll = defence_stat.has_advantage(&attack_stat) as usize;

        let attack_roll = pick_best_x_dice_rolls(20, 1 + attack_reroll, 1, None)
            + attacker.get_modifier(&attack_stat);
        let defence_roll = pick_best_x_dice_rolls(20, 1 + defence_reroll, 1, None)
            + defender.get_modifier(&defence_stat);

        let mut turn_log = String::new();

        turn_log += &attack_stat.get_attack_text();

        let damage = if attack_roll >= defence_roll {
            turn_log += &format!(" {}", defence_stat.get_defence_failure_text());

            let damage_modifier = match attack_stat {
                Stat::STR | Stat::DEX | Stat::CON => cmp::max(0, attacker.get_modifier(&Stat::STR)),
                Stat::INT | Stat::CHR | Stat::WIS => cmp::max(0, attacker.get_modifier(&Stat::INT)),
            };

            pick_best_x_dice_rolls(10, 1, 1, None) + damage_modifier
        } else {
            turn_log += &format!(" {}", defence_stat.get_defence_success_text());
            0
        };

        turn_log = turn_log
            .replace("DEF", &format!("**{}**[{}]", defender.name, defender.hp))
            .replace("ATK", &format!("**{}**[{}]", attacker.name, attacker.hp))
            .replace("DMG", &damage.to_string());

        self.log += &format!("{}\n", turn_log);

        defender.hp = cmp::max(0, defender.hp - damage as isize);
        defender.hp == 0
    }

    fn intro(&self) -> String {
        let mut res = String::new();
        let pad = " ".repeat(OUTPUT_WIDTH / 2);
        res += &format!("{pad}+-------+{pad}\n");
        res += &format!("{pad}|  vs.  |{pad}\n");
        res += &format!("{pad}+-------+{pad}\n");

        res
    }

    pub fn summary(&self) -> &str {
        &self.summary
    }
}

impl Display for RPGFight {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "```")?;
        writeln!(f, "{}", self.challenger)?;
        writeln!(f, "{}", self.intro())?;
        writeln!(f, "{}", self.accepter)?;
        writeln!(f, "```")?;
        writeln!(f, "{}", self.log)?;
        writeln!(f, "{}", self.summary)
    }
}
