// How to use:
// - Make sure you have a working version of the DB this app uses named "database.sqlite" (the README.md has instructions for it)
// - Place the database we use in the JS version the root folder (same one as database.sqlite) with the name 'dev.db'
// - Run this binary with 'cargo r --bin import_js_data'
//
// IMPORTANT: This script replaces existing rows with the ones in the JS db, use with caution

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use sqlx::{Connection, QueryBuilder, Row, Sqlite, SqliteConnection, Transaction};

struct DinoTransaction<'a> {
    dino_id: i64,
    user_id: &'a str,
    gifter_id: Option<&'a str>,
    kind: &'static str,
}

struct Dino<'a> {
    id: i64,
    owner_id: &'a str,
    name: &'a str,
    filename: &'a str,
    created_at: DateTime<Utc>,
    hotness: i64,
    owners: i64,
    body: &'a str,
    mouth: &'a str,
    eyes: &'a str,
}

#[rustfmt::skip]
#[tokio::main]
async fn main() {
    let mut db = SqliteConnection::connect("database.sqlite").await.unwrap();
    let mut transaction = db.begin().await.expect("Failed to begin transaction");

    sqlx::query("ATTACH 'dev.db' AS source_db")
        .execute(&mut transaction)
        .await
        .expect("Could not attach JS db file");

    sqlx::query("
        INSERT
        OR REPLACE INTO User (id, fav_color, last_random, last_loss)
        SELECT id, favColor, DATETIME(lastRandom / 1000, 'unixepoch'), DATETIME(lastLoss / 1000, 'unixepoch')
        FROM source_db.User;

        INSERT OR REPLACE INTO DuelStats ( user_id, losses, wins, draws, win_streak, loss_streak, win_streak_max, loss_streak_max )
        SELECT userId, losses, wins, draws, winStreak, lossStreak, winStreakMax, lossStreakMax
        FROM source_db.Duels;

        INSERT OR REPLACE INTO BestMixu (user_id, score, tiles)
        SELECT owner, score, tiles
        FROM source_db.BestMixu;

        INSERT OR REPLACE INTO RPGCharacter ( user_id, losses, wins, draws, last_loss, elo_rank, peak_elo, floor_elo )
        SELECT id, wins, losses, draws, DATETIME(lastLoss / 1000, 'unixepoch'), eloRank, peakElo, floorElo
        FROM source_db.RPGCharacter;

        INSERT OR REPLACE INTO DinoUser (id, last_hatch, last_gifting, last_slurp, consecutive_fails)
        SELECT id, DATETIME(lastMint / 1000, 'unixepoch'), DATETIME(lastGiftGiven / 1000, 'unixepoch'), DATETIME(lastSlurp / 1000, 'unixepoch'), consecutiveFails
        FROM source_db.NFDEnjoyer;
        ")
        .execute(&mut transaction)
        .await
        .unwrap();

    let dino_rows = sqlx::query(
        "SELECT id, owner, previousOwners, coveters, shunners, name, filename, DATETIME(mintDate / 1000, 'unixepoch'), hotness, code FROM source_db.NFDItem",
    )
    .fetch_all(&mut transaction)
    .await
    .unwrap();

    let mut transactions = Vec::with_capacity(1024);
    let mut users = HashSet::with_capacity(1024);
    let mut dinos = Vec::with_capacity(1024);

    for dino_row in &dino_rows {
        let dino_id: i64 = dino_row.get(0);
        let owner: &str = dino_row.get(1);
        users.insert(owner);

        // The first owner is put in the 'previousOwners' list on dino creation
        // so the current owner is the last previousOwner, no need to do
        // anything special, we can just use this list
        let previous_owners: &str = dino_row.get(2);
        let previous_owners: Vec<&str> = previous_owners.split(',').collect();
        for i in 0..(previous_owners.len() - 1) {
            let previous_owner = previous_owners[i];
            if previous_owner.is_empty() { break }

            // The previous owners are formatted like <@1234567890>
            // we don't want the <@ and >
            let previous_owner = &previous_owner[2..previous_owner.len() - 1];

            let Some(&receiver) = previous_owners.get(i + 1) else { continue };
            let receiver = &receiver[2..receiver.len() - 1];
            // The first owner was already added so just adding the one who received the dino next is enough
            users.insert(receiver);

            transactions.push(DinoTransaction { dino_id, user_id: receiver, gifter_id: Some(previous_owner), kind: "GIFT" })
        }

        let coveters: &str = dino_row.get(3);
        for coveter in coveters.split(',') {
            if coveter.is_empty() { break }
            users.insert(coveter);
            transactions.push(DinoTransaction { dino_id, user_id: coveter, gifter_id: None, kind: "COVET" })
        }

        let shunners: &str = dino_row.get(4);
        for shunner in shunners.split(',') {
            if shunner.is_empty() { break }
            users.insert(shunner);
            transactions.push(DinoTransaction { dino_id, user_id: shunner, gifter_id: None, kind: "SHUN" })
        }

        let code: &str = dino_row.get(9);
        let parts: Vec<&str> = code.split(',').collect();
        dinos.push(Dino {
            id: dino_id,
            owner_id: owner,
            name: dino_row.get(5),
            filename: dino_row.get(6),
            created_at: dino_row.get(7),
            hotness: dino_row.get(8),
            owners: previous_owners.len() as i64,
            body: parts[0],
            mouth: parts[1],
            eyes: parts[2]
        })
    }

    // Make sure every possibile dino user is present before creating the transactions,
    // for example in JS-land a covet would just append the user's id to the coveters string
    // in this version, to uphold the database reference, every transaction has
    // to have a real user related to it.
    let mut builder: QueryBuilder<Sqlite> = QueryBuilder::new("INSERT OR REPLACE INTO DinoUser (id) ");
    if !users.is_empty() {
        builder.push("");
        builder.push_values(users.into_iter(), |mut b, u| { b.push_bind(u); });
        builder.push("; ");

        builder.build().execute(&mut transaction).await.expect("Failed to insert users");
    }
    builder.reset();

    // Insert dinos and transactions in chunks because there's a u16::MAX limit
    // to the number of paramenters a query can have and we can easily exceed that
    // Users aren't be too numerous and they only need one argument.
    for chunk in dinos.chunks(1024) {
        insert_dinos(&mut transaction, chunk).await
    }

    for dino_trans in transactions.chunks(1024) {
        insert_transactions(&mut transaction, dino_trans).await;
    }

    sqlx::query(
        "INSERT INTO DinoTransactions (dino_id, user_id, type) SELECT dinoId, enjoyerId, 'FAVOURITE' FROM NFDEnthusiasts"
    )
    .execute(&mut transaction)
    .await
    .expect("Failed to add dino favourites list");

    transaction.commit().await.expect("Failed to commit transaction");
}

async fn insert_transactions<'a>(
    transaction: &mut Transaction<'_, Sqlite>,
    transactions: &'a [DinoTransaction<'a>],
) {
    let mut builder: QueryBuilder<Sqlite> =
        QueryBuilder::new("INSERT INTO DinoTransactions (dino_id, user_id, gifter_id, type) ");
    builder.push_values(transactions, |mut b, t| {
        b.push_bind(t.dino_id)
            .push_bind(t.user_id)
            .push_bind(t.gifter_id)
            .push_bind(t.kind);
    });

    builder.build().execute(transaction).await.unwrap();
}

async fn insert_dinos<'a>(transaction: &mut Transaction<'_, Sqlite>, dinos: &'a [Dino<'a>]) {
    let mut builder: QueryBuilder<Sqlite> =
        QueryBuilder::new(
            "INSERT OR REPLACE INTO Dino (id, owner_id, name, filename, created_at, owners, hotness, body, mouth, eyes) "
        );
    builder.push_values(dinos, |mut b, d| {
        b.push_bind(d.id)
            .push_bind(d.owner_id)
            .push_bind(d.name)
            .push_bind(d.filename)
            .push_bind(d.created_at)
            .push_bind(d.owners)
            .push_bind(d.hotness)
            .push_bind(d.body)
            .push_bind(d.mouth)
            .push_bind(d.eyes);
    });

    builder.build().execute(transaction).await.unwrap();
}
