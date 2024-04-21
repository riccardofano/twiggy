-- Add migration script here
CREATE TABLE BestMixu (
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    user_id INTEGER NOT NULL,
    score INTEGER NOT NULL
);