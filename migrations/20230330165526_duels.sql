-- Add migration script here
CREATE TABLE User (
    id TEXT NOT NULL PRIMARY KEY,
    fav_color TEXT,
    last_random DATETIME NOT NULL DEFAULT 0,
    last_loss DATETIME NOT NULL DEFAULT 0
);

CREATE TABLE DuelStats (
    user_id TEXT NOT NULL PRIMARY KEY REFERENCES User(id),
    losses INTEGER NOT NULL DEFAULT 0,
    wins INTEGER NOT NULL DEFAULT 0,
    draws INTEGER NOT NULL DEFAULT 0,
    win_streak INTEGER NOT NULL DEFAULT 0,
    loss_streak INTEGER NOT NULL DEFAULT 0,
    win_streak_max INTEGER NOT NULL DEFAULT 0,
    loss_streak_max INTEGER NOT NULL DEFAULT 0
);