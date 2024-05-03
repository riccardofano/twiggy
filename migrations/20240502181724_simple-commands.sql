-- Add migration script here
CREATE TABLE SimpleCommands (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    content TEXT NOT NULL,
    guild_id INTEGER NOT NULL
);

CREATE UNIQUE INDEX idx_guild_command ON SimpleCommands(guild_id, name);
