use rand::seq::SliceRandom;

#[allow(clippy::upper_case_acronyms)]
#[derive(PartialEq, Eq, Hash, Clone, Copy)]
pub enum Stat {
    STR,
    DEX,
    CON,
    INT,
    WIS,
    CHR,
}

impl Stat {
    pub fn has_advantage(&self, other: &Stat) -> bool {
        use self::Stat::*;
        matches!(
            (self, other),
            (STR, DEX)
                | (STR, CON)
                | (DEX, CON)
                | (DEX, INT)
                | (CON, INT)
                | (CON, WIS)
                | (INT, WIS)
                | (INT, CHR)
                | (WIS, CHR)
                | (WIS, STR)
                | (CHR, STR)
                | (CHR, DEX)
        )
    }

    pub fn get_attack_text(&self) -> String {
        let mut rng = rand::thread_rng();
        ATTACK_TEXTS[*self as usize]
            .choose(&mut rng)
            .unwrap()
            .to_string()
    }
    pub fn get_defence_success_text(&self) -> String {
        let mut rng = rand::thread_rng();
        DEFENCE_SUCCESS_TEXTS[*self as usize]
            .choose(&mut rng)
            .unwrap()
            .to_string()
    }
    pub fn get_defence_failure_text(&self) -> String {
        let mut rng = rand::thread_rng();
        DEFENCE_FAILURE_TEXTS[*self as usize]
            .choose(&mut rng)
            .unwrap()
            .to_string()
    }
}

pub struct Class {
    pub name: &'static str,
    pub stat_preferences: &'static [Stat; 6],
}

pub struct Specie {
    pub name: &'static str,
    pub stat_bonuses: &'static [Stat],
}

pub const CLASSES: &[Class] = &[
    Class {
        name: "artificer",
        stat_preferences: &[
            Stat::INT,
            Stat::WIS,
            Stat::CON,
            Stat::STR,
            Stat::DEX,
            Stat::CHR,
        ],
    },
    Class {
        name: "barbarian",
        stat_preferences: &[
            Stat::STR,
            Stat::CON,
            Stat::CHR,
            Stat::WIS,
            Stat::DEX,
            Stat::INT,
        ],
    },
    Class {
        name: "bard",
        stat_preferences: &[
            Stat::CHR,
            Stat::WIS,
            Stat::INT,
            Stat::DEX,
            Stat::CON,
            Stat::STR,
        ],
    },
    Class {
        name: "cleric",
        stat_preferences: &[
            Stat::WIS,
            Stat::CON,
            Stat::CHR,
            Stat::STR,
            Stat::DEX,
            Stat::INT,
        ],
    },
    Class {
        name: "druid",
        stat_preferences: &[
            Stat::WIS,
            Stat::INT,
            Stat::STR,
            Stat::CON,
            Stat::DEX,
            Stat::CHR,
        ],
    },
    Class {
        name: "fighter",
        stat_preferences: &[
            Stat::STR,
            Stat::CON,
            Stat::DEX,
            Stat::CHR,
            Stat::INT,
            Stat::WIS,
        ],
    },
    Class {
        name: "monk",
        stat_preferences: &[
            Stat::DEX,
            Stat::CHR,
            Stat::STR,
            Stat::WIS,
            Stat::CON,
            Stat::INT,
        ],
    },
    Class {
        name: "paladin",
        stat_preferences: &[
            Stat::CHR,
            Stat::STR,
            Stat::INT,
            Stat::CON,
            Stat::WIS,
            Stat::DEX,
        ],
    },
    Class {
        name: "ranger",
        stat_preferences: &[
            Stat::WIS,
            Stat::DEX,
            Stat::CON,
            Stat::INT,
            Stat::CHR,
            Stat::STR,
        ],
    },
    Class {
        name: "rogue",
        stat_preferences: &[
            Stat::DEX,
            Stat::CHR,
            Stat::STR,
            Stat::CON,
            Stat::WIS,
            Stat::INT,
        ],
    },
    Class {
        name: "sorcerer",
        stat_preferences: &[
            Stat::INT,
            Stat::CHR,
            Stat::WIS,
            Stat::DEX,
            Stat::CON,
            Stat::STR,
        ],
    },
    Class {
        name: "warlock",
        stat_preferences: &[
            Stat::INT,
            Stat::WIS,
            Stat::CHR,
            Stat::CON,
            Stat::DEX,
            Stat::STR,
        ],
    },
    Class {
        name: "wizard",
        stat_preferences: &[
            Stat::INT,
            Stat::WIS,
            Stat::CHR,
            Stat::CON,
            Stat::DEX,
            Stat::STR,
        ],
    },
    Class {
        name: "warrior",
        stat_preferences: &[
            Stat::STR,
            Stat::CON,
            Stat::DEX,
            Stat::CHR,
            Stat::WIS,
            Stat::INT,
        ],
    },
    Class {
        name: "thief",
        stat_preferences: &[
            Stat::DEX,
            Stat::WIS,
            Stat::INT,
            Stat::STR,
            Stat::CHR,
            Stat::CON,
        ],
    },
    Class {
        name: "motorcycle knight",
        stat_preferences: &[
            Stat::DEX,
            Stat::STR,
            Stat::CON,
            Stat::CHR,
            Stat::INT,
            Stat::WIS,
        ],
    },
    Class {
        name: "bardbarian",
        stat_preferences: &[
            Stat::STR,
            Stat::CHR,
            Stat::CON,
            Stat::DEX,
            Stat::WIS,
            Stat::INT,
        ],
    },
    Class {
        name: "person-at-Arms",
        stat_preferences: &[
            Stat::CON,
            Stat::STR,
            Stat::DEX,
            Stat::CHR,
            Stat::INT,
            Stat::WIS,
        ],
    },
    Class {
        name: "librarian",
        stat_preferences: &[
            Stat::INT,
            Stat::CHR,
            Stat::WIS,
            Stat::CON,
            Stat::STR,
            Stat::DEX,
        ],
    },
    Class {
        name: "jedi",
        stat_preferences: &[
            Stat::WIS,
            Stat::CHR,
            Stat::DEX,
            Stat::CON,
            Stat::STR,
            Stat::INT,
        ],
    },
    Class {
        name: "strangler",
        stat_preferences: &[
            Stat::STR,
            Stat::INT,
            Stat::WIS,
            Stat::DEX,
            Stat::CON,
            Stat::CHR,
        ],
    },
    Class {
        name: "battle felon",
        stat_preferences: &[
            Stat::CHR,
            Stat::STR,
            Stat::CON,
            Stat::DEX,
            Stat::INT,
            Stat::WIS,
        ],
    },
    Class {
        name: "pugalist",
        stat_preferences: &[
            Stat::STR,
            Stat::DEX,
            Stat::CON,
            Stat::CHR,
            Stat::WIS,
            Stat::INT,
        ],
    },
    Class {
        name: "documancer",
        stat_preferences: &[
            Stat::INT,
            Stat::WIS,
            Stat::DEX,
            Stat::CON,
            Stat::STR,
            Stat::CHR,
        ],
    },
    Class {
        name: "mathemagician",
        stat_preferences: &[
            Stat::WIS,
            Stat::INT,
            Stat::DEX,
            Stat::CON,
            Stat::STR,
            Stat::CHR,
        ],
    },
    Class {
        name: "tourist",
        stat_preferences: &[
            Stat::CON,
            Stat::DEX,
            Stat::CHR,
            Stat::WIS,
            Stat::INT,
            Stat::STR,
        ],
    },
    Class {
        name: "valkyrie",
        stat_preferences: &[
            Stat::STR,
            Stat::CON,
            Stat::DEX,
            Stat::WIS,
            Stat::INT,
            Stat::CHR,
        ],
    },
    Class {
        name: "juggler",
        stat_preferences: &[
            Stat::DEX,
            Stat::CHR,
            Stat::INT,
            Stat::STR,
            Stat::CON,
            Stat::WIS,
        ],
    },
    Class {
        name: "CEO",
        stat_preferences: &[
            Stat::CHR,
            Stat::INT,
            Stat::STR,
            Stat::CON,
            Stat::DEX,
            Stat::WIS,
        ],
    },
    Class {
        name: "drunken master",
        stat_preferences: &[
            Stat::DEX,
            Stat::CON,
            Stat::WIS,
            Stat::CHR,
            Stat::INT,
            Stat::STR,
        ],
    },
    Class {
        name: "chaotician",
        stat_preferences: &[
            Stat::CON,
            Stat::WIS,
            Stat::INT,
            Stat::CHR,
            Stat::DEX,
            Stat::STR,
        ],
    },
    Class {
        name: "prankster",
        stat_preferences: &[
            Stat::CHR,
            Stat::DEX,
            Stat::WIS,
            Stat::INT,
            Stat::STR,
            Stat::CON,
        ],
    },
    Class {
        name: "anarchist",
        stat_preferences: &[
            Stat::INT,
            Stat::STR,
            Stat::CON,
            Stat::CHR,
            Stat::DEX,
            Stat::WIS,
        ],
    },
    Class {
        name: "pacifist",
        stat_preferences: &[
            Stat::CON,
            Stat::WIS,
            Stat::CHR,
            Stat::INT,
            Stat::STR,
            Stat::DEX,
        ],
    },
    Class {
        name: "tactician",
        stat_preferences: &[
            Stat::INT,
            Stat::STR,
            Stat::CHR,
            Stat::WIS,
            Stat::CON,
            Stat::DEX,
        ],
    },
    Class {
        name: "bureaucrat",
        stat_preferences: &[
            Stat::INT,
            Stat::STR,
            Stat::CON,
            Stat::DEX,
            Stat::WIS,
            Stat::CHR,
        ],
    },
    Class {
        name: "mecha-pilot",
        stat_preferences: &[
            Stat::DEX,
            Stat::WIS,
            Stat::INT,
            Stat::CON,
            Stat::CHR,
            Stat::STR,
        ],
    },
    Class {
        name: "disarmorer",
        stat_preferences: &[
            Stat::WIS,
            Stat::INT,
            Stat::CHR,
            Stat::DEX,
            Stat::STR,
            Stat::CON,
        ],
    },
    Class {
        name: "potwash",
        stat_preferences: &[
            Stat::CON,
            Stat::WIS,
            Stat::INT,
            Stat::DEX,
            Stat::CHR,
            Stat::STR,
        ],
    },
    Class {
        name: "waifu",
        stat_preferences: &[
            Stat::CHR,
            Stat::DEX,
            Stat::STR,
            Stat::CON,
            Stat::WIS,
            Stat::INT,
        ],
    },
    Class {
        name: "street samurai",
        stat_preferences: &[
            Stat::DEX,
            Stat::WIS,
            Stat::STR,
            Stat::CHR,
            Stat::CON,
            Stat::INT,
        ],
    },
];

pub const STANDARD_SPECIES: &[Specie] = &[
    Specie {
        name: "Dwarf",
        stat_bonuses: &[Stat::CON, Stat::CON, Stat::STR],
    },
    Specie {
        name: "Elf",
        stat_bonuses: &[Stat::INT, Stat::INT, Stat::DEX],
    },
    Specie {
        name: "Halfling",
        stat_bonuses: &[Stat::DEX, Stat::CHR, Stat::CHR],
    },
    Specie {
        name: "Human",
        stat_bonuses: &[Stat::CON, Stat::DEX, Stat::STR],
    },
    Specie {
        name: "Dragonborn",
        stat_bonuses: &[Stat::CON, Stat::STR, Stat::INT],
    },
    Specie {
        name: "Gnome",
        stat_bonuses: &[Stat::DEX, Stat::DEX, Stat::DEX],
    },
    Specie {
        name: "Half-Elf",
        stat_bonuses: &[Stat::INT, Stat::CON, Stat::CHR],
    },
    Specie {
        name: "Half-Orc",
        stat_bonuses: &[Stat::STR, Stat::STR, Stat::CON],
    },
    Specie {
        name: "Tiefling",
        stat_bonuses: &[Stat::INT, Stat::INT, Stat::DEX],
    },
    Specie {
        name: "Dire-Manatee",
        stat_bonuses: &[Stat::INT, Stat::INT, Stat::INT],
    },
    Specie {
        name: "Half-Goat",
        stat_bonuses: &[Stat::DEX, Stat::DEX, Stat::STR],
    },
    Specie {
        name: "Reverse-Mermaid",
        stat_bonuses: &[Stat::CHR, Stat::CHR, Stat::DEX],
    },
    Specie {
        name: "Reverse-Centaur",
        stat_bonuses: &[Stat::STR, Stat::DEX, Stat::DEX],
    },
    Specie {
        name: "Satyr",
        stat_bonuses: &[Stat::INT, Stat::INT, Stat::DEX],
    },
    Specie {
        name: "Double-Hobbit",
        stat_bonuses: &[Stat::CHR, Stat::CHR, Stat::CHR],
    },
    Specie {
        name: "Long-Goblin",
        stat_bonuses: &[Stat::DEX, Stat::CON, Stat::CON],
    },
    Specie {
        name: "Double half-orc",
        stat_bonuses: &[Stat::STR, Stat::STR, Stat::STR],
    },
    Specie {
        name: "Gingerbrute-Person",
        stat_bonuses: &[Stat::CHR, Stat::INT, Stat::INT],
    },
    Specie {
        name: "Sock Demon",
        stat_bonuses: &[Stat::INT, Stat::CHR, Stat::CHR],
    },
    Specie {
        name: "Metalhead",
        stat_bonuses: &[Stat::CHR, Stat::INT, Stat::STR],
    },
    Specie {
        name: "Beer Elemental",
        stat_bonuses: &[Stat::CON, Stat::CON, Stat::CHR, Stat::CHR],
    },
    Specie {
        name: "Slime-Person",
        stat_bonuses: &[Stat::DEX, Stat::DEX, Stat::CHR],
    },
];

pub const BANANA_SPECIE: &Specie = &Specie {
    name: "Bananasaurus",
    stat_bonuses: &[
        Stat::STR,
        Stat::DEX,
        Stat::CON,
        Stat::INT,
        Stat::WIS,
        Stat::CHR,
    ],
};

pub const ADJECTIVES: &[&str] = &[
    "Chaotic",
    "Neutral",
    "Lawful",
    "Ordered",
    "Inspired",
    "Performative",
    "Angry",
    "Hard",
    "Soft",
    "Low-key",
    "Based",
    "Woke",
    "Projected",
    "Mailicious",
    "Directed",
    "Memetic",
    "Bureaucratic",
    "Loving",
    "Organised",
    "Frustrated",
    "Enlightend",
    "Absurd",
    "Frustrated",
    "Indifferent",
    "Apathetic",
    "Contented",
    "Cynical",
    "Riteous",
    "Indulgent",
    "Pragmatic",
    "Postmodern",
    "Educated",
    "Ignorant",
];

pub const NOUNS: &[&str] = &[
    "evil",
    "neutral",
    "good",
    "stupid",
    "clever",
    "zen",
    "angry",
    "coffee",
    "food",
    "heroic",
    "meme",
    "inactive",
    "pretty",
    "ugly",
    "wahoo",
    "horny",
    "righteous",
    "sin",
];

const ATTACK_TEXTS: &[&[&str]] = &[
    &[
        "ATK swings a wild haymaker at DEF,",
        "ATK throws a punch at DEF,",
        "ATK goes in for the bear hug,",
        "ATK tries to crush DEF like a bug,",
        "ATK hurls a boulder at DEF,",
        "ATK advances menacingly,",
        "ATK does a shoryuken,",
        "ATK tries to bonk DEF on the noggin,",
        "ATK yeets DEF,",
    ],
    &[
        "ATK lunges at DEF,",
        "ATK darts in with an attack,",
        "ATK throws a rock,",
        "ATK unleashes a flurry of blows",
        "ATK sneaks up on DEF,",
        "ATK shoots an arrow at DEF,",
        "ATK begins the 5 point exploding heart technique,",
        "ATK pulls off a special move,",
        "ATK starts throwing hands,",
    ],
    &[
        "ATK flexes at DEF,",
        "ATK bull-charges DEF,",
        "ATK challenges DEF to a drinking contest,",
        "ATK body slams DEF,",
        "ATK shows off their hot bod,",
        "ATK winks at DEF,",
        "ATK starts throwing shapes,",
    ],
    &[
        "ATK throws a fireball at DEF,",
        "ATK unleashes a psychic assault,",
        "ATK plays a face-down card and ends their turn,",
        "ATK outsmarts DEF,",
        "ATK points their finger of death at DEF,",
        "ATK reads the dictionary at DEF,",
        "ATK throws a spirit bomb at DEF,",
    ],
    &[
        "ATK calls on a higher power to smite DEF,",
        "ATK orders their animal companion to attack,",
        "ATK believes in themself,",
        "ATK springs an ambush,",
        "ATK enacts a cunning plan,",
        "ATK appeals to DEF's better nature,",
        "ATK casts turn undead,",
        "ATK stands in contemplation,",
    ],
    &[
        "ATK says mean things about DEF,",
        "ATK cancels DEF on Twitter,",
        "ATK bombards DEF with discord pings,",
        "ATK starts the crowd chanting,",
        "ATK drops a truth bomb on DEF,",
        "ATK taunts DEF,",
        "ATK reads DEF their rights,",
        "ATK uses \"good\" as an adverb,",
    ],
];

// If the defence is successful, this set of strings is chosen from
// according to the stat the defender defended with
// "ATK" gets replaced with the attacker name,
// "DEF" gets replaced with the defender name
const DEFENCE_SUCCESS_TEXTS: &[&[&str]] = &[
    &[
        "but DEF pushes them over.",
        "but DEF simply flexes.",
        "but it glances off DEF's washboard abs.",
        "but DEF is a force of nature.",
        "but DEF is having none of it.",
        "but DEF is too strong.",
        "but DEF is too stacked.",
        "but DEF is built like a brick shithouse.",
    ],
    &[
        "but DEF dodges the attack.",
        "but DEF is nowhere to be seen!",
        "but DEF is somewhere else.",
        "DEF parries!",
        "DEF counters with pocket sand!",
        "but DEF narrowly avoids it.",
        "but DEF sidesteps.",
    ],
    &[
        "but DEF stands impervious.",
        "but DEF hardly notices.",
        "but DEF ignores it.",
        "but DEF isn't affected.",
        "but DEF is built of sterner stuff.",
        "it's not very effective.",
        "DEF takes it on the chin.",
        "DEF just blinks.",
        "but DEF goes super saiyan!",
    ],
    &[
        "but DEF reads them like a book.",
        "but DEF uses their brain wrinkles to counter.",
        "but DEF teleports away.",
        "but DEF casts stoneskin for extra armor.",
        "but DEF knows better.",
        "but DEF shouts COUNTERSPELL!",
        "but DEF outsmarts them.",
        "but DEF is one step ahead.",
    ],
    &[
        "but DEF is protected by divine light.",
        "but DEF is saved by their animal companion.",
        "but DEF doesn't believe in damage.",
        "but DEF has other ideas.",
        "but DEF already prepared for that.",
        "but DEF has other plans.",
        "but DEF is destined for greater things.",
        "but DEF just turns the other cheek.",
        "DEF meditates through the attack.",
    ],
    &[
        "but DEF just laughs, unnerving ATK.",
        "but DEF convinces them it's a bad idea.",
        "but DEF talks them out of it.",
        "but DEF distracts them.",
        "but DEF just cracks wise.",
        "but DEF just shouts them down.",
        "but DEF talks their way out of it.",
        "but DEF is too pretty.",
        "but DEF gets the crowd on their side.",
    ],
];

// If the defence fails, then text is selected from this set.
// "ATK" gets replaced with the attacker name,
// "DEF" gets replaced with the defender name
// "DMG" gets replaced with the damage value.
const DEFENCE_FAILURE_TEXTS: &[&[&str]] = &[
    &[
        "and DEF's strength fails, taking DMG damage.",
        "and DEF can't resist the DMG damage.",
        "and DEF is too weak to prevent the DMG damage.",
        "overpowering DEF's defence inflicting DMG damage.",
        "and DEF can't quite get the upper hand. DMG damage.",
        "and DEF can't push through. DMG damage.",
        "DEF's muscles aren't big enough to avoid the DMG damage.",
    ],
    &[
        "and DEF is too slow to get out the way, eating DMG damage.",
        "DEF fails to dodge. DMG damage done.",
        "DEF didn't react in time and takes DMG damage.",
        "DEF stumbles and takes the full DMG damage.",
        "and DEF gets the parry timing wrong, taking DMG damage.",
        "DEF takes DMG damage and blames lag.",
        "DEF walks right into the DMG damage.",
        "DEF's fancy footwork isn't enough. DMG damage.",
    ],
    &[
        "and DEF takes the full DMG damage.",
        "and DEF blocks it with their face taking DMG damage.",
        "and DEF can't resist the DMG damage.",
        "DEF is left with DMG fewer hit points.",
        "and DEF isn't tough enough to resist the DMG damage.",
        "and DEF isn't tough enough to ignore DMG damage.",
        "DEF's is less healthy after the DMG damage.",
    ],
    &[
        "and DEF reacts poorly suffering DMG damage.",
        "and DEF has a smooth brain moment resulting in DMG damage.",
        "and DEF didn't see the DMG damage coming.",
        "and DEF's counterspell fizzles, taking DMG damage.",
        "DEF forgot the words to their spell and takes DMG damage.",
        "DEF doesn't know what hit them. DMG damage.",
        "and DEF can't think of a solution to the DMG damage.",
        "DEF hurt themself in confusion for DMG damage.",
    ],
    &[
        "and DEF's power abandons them, taking DMG damage.",
        "and DEF wasn't prepared for that, taking DMG damage.",
        "and DEF didn't expect it. DMG damage done.",
        "and DEF's faith falters suffering DMG damage.",
        "DEF turns the other cheek. It gets hit for DMG damage.",
        "DEF is caught off guard, suffering DMG damage.",
        "and DEF didn't try hard enough. DMG damage.",
        "and DEF can't come to accept it. DMG damage.",
    ],
    &[
        "and DEF's laughter is not the best medicine. DMG damage.",
        "and DEF's talking doesn't stop the DMG damage.",
        "cutting DEF off mid sentence and inflicting DMG damage.",
        "interrupting DEF's monologue and inflicting DMG damage.",
        "and DEF is left speechless. DMG damage.",
        "and DEF has no reply. DMG damage.",
        "and DEF is tongue-tied. DMG damage.",
        "and the DMG damage makes DEF cry.",
    ],
];

#[repr(usize)]
#[derive(Clone, Copy)]
pub enum VictoryKind {
    Standard,
    Perfect,
    Close,
}

impl VictoryKind {
    pub fn get_texts(&self) -> &'static [&'static str] {
        VICTORY_TEXTS[*self as usize]
    }
}

// Finally, it selects a random concluding message.
// VICTOR is replaced with the winner's name
// LOSER is replaced with the loser's name
const VICTORY_TEXTS: &[&[&str]] = &[
    &[
        "LOSER falls and VICTOR wins!",
        "LOSER is smashed like a bowl of eggs. VICTOR wins!",
        "LOSER taps out. VICTOR wins!",
        "Sucks to suck LOSER, VICTOR wins!",
        "LOSER can't go on, VICTOR wins!",
        "VICTOR stands victorious, LOSER is left to lick their wounds.",
        "VICTOR wins! GG go next.",
        "VICTOR wins! GG no re.",
        "LOSER faints. VICTOR jumps for joy!",
        "LOSER can't take it any more, VICTOR wins!",
        "LOSER is outplayed, VICTOR is the winner!",
        "Winner winner chicken dinner for VICTOR. LOSER starves.",
        "VICTOR wins! LOSER thinks the game is rigged!",
    ],
    &[
        "VICTOR scores a perfect victory! LOSER is shamed!",
        "VICTOR is untouchable! LOSER never got a hit in.",
        "VICTOR must be hacking because LOSER couldn't land a hit.",
        "FRAUD ALERT! VICTOR scores a perfect victory over LOSER.",
        "VICTOR wins without breaking a sweat. Was LOSER even trying?",
    ],
    &[
        "VICTOR stands bloodied but victorious. LOSER gave as good as they got!",
        "VICTOR scrapes by, narrowly defeating LOSER.",
        "VICTOR wins over LOSER by a hair.",
        "VICTOR and LOSER are evenly matched, but VICTOR comes out ahead.",
        "A close one, but VICTOR wins.",
    ],
];
