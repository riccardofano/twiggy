CREATE TABLE Dino (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    owner_id TEXT NOT NULL REFERENCES DinoUser(id),
    name TEXT NOT NULL UNIQUE,
    filename TEXT NOT NULL UNIQUE,
    created_at DATETIME NOT NULL,

    worth INTEGER NOT NULL DEFAULT 1,
    hotness INTEGER NOT NULL DEFAULT 0,

    body TEXT NOT NULL,
    mouth TEXT NOT NULL,
    eyes TEXT NOT NULL,
    UNIQUE (body, mouth, eyes)
);

CREATE TABLE DinoUser (
    id TEXT NOT NULL PRIMARY KEY,
    last_hatch DATETIME NOT NULL DEFAULT 0,
    last_gifting DATETIME NOT NULL DEFAULT 0,
    last_rename DATETIME NOT NULL DEFAULT 0,
    last_slurp DATETIME NOT NULL DEFAULT 0,
    consecutive_fails INTEGER NOT NULL DEFAULT 4
);

CREATE TABLE DinoTransactionType (
    type TEXT NOT NULL PRIMARY KEY
);

CREATE TABLE DinoTransactions (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    dino_id INTEGER NOT NULL REFERENCES Dino(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES DinoUser(id) ON DELETE NO ACTION,
    gifter_id TEXT DEFAULT NULL REFERENCES DinoUser(id) ON DELETE NO ACTION,
    type TEXT NOT NULL REFERENCES DinoTransactionType(type)
);

INSERT INTO DinoTransactionType (type) VALUES ('GIFT');
INSERT INTO DinoTransactionType (type) VALUES ('COVET');
INSERT INTO DinoTransactionType (type) VALUES ('SHUN');
INSERT INTO DinoTransactionType (type) VALUES ('FAVOURITE');