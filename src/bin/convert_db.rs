use std::collections::HashSet;

use sqlx::{Connection, QueryBuilder, Row, Sqlite, SqliteConnection};

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
    created_at: &'a str,
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
        .unwrap();

    sqlx::query("
        INSERT
        OR REPLACE INTO User (id, fav_color, last_random, last_loss)
        SELECT id, favColor, lastRandom, lastLoss
        FROM source_db.User;

        INSERT OR REPLACE INTO DuelStats ( user_id, losses, wins, draws, win_streak, loss_streak, win_streak_max, loss_streak_max )
        SELECT userId, losses, wins, draws, winStreak, lossStreak, winStreakMax, lossStreakMax
        FROM source_db.Duels;

        INSERT OR REPLACE INTO BestMixu (user_id, score, tiles)
        SELECT owner, score, tiles
        FROM source_db.BestMixu;

        INSERT OR REPLACE INTO RPGCharacter ( user_id, losses, wins, draws, last_loss, elo_rank, peak_elo, floor_elo )
        SELECT id, wins, losses, draws, lastLoss, eloRank, peakElo, floorElo
        FROM source_db.RPGCharacter;

        INSERT OR REPLACE INTO DinoUser (id, last_hatch, last_gifting, last_slurp, consecutive_fails)
        SELECT id, lastMint, lastGiftGiven, lastSlurp, consecutiveFails
        FROM source_db.NFDEnjoyer;
        ")
        .execute(&mut transaction)
        .await
        .unwrap();

    let dino_rows = sqlx::query(
        "SELECT id, owner, previousOwners, coveters, shunners, name, filename, mintDate, hotness, code FROM source_db.NFDItem",
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

        let previous_owners: &str = dino_row.get(2);
        let previous_owners: Vec<&str> = previous_owners.split(',').collect();

        let mut giftee = owner;
        for i in (1..previous_owners.len()).rev() {
            let current = previous_owners[i];
            if current.is_empty() { break }

            users.insert(current);
            transactions.push(DinoTransaction { dino_id, user_id: giftee, gifter_id: Some(current), kind: "GIFT" });
            giftee = current;
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
    let mut builder: QueryBuilder<Sqlite> = QueryBuilder::new(
        "INSERT OR REPLACE INTO DinoUser (id) "
    );
    builder.push_values(users.into_iter(), |mut b, u| { b.push_bind(u); });

    // Adding the dinos
    let mut builder: QueryBuilder<Sqlite> = QueryBuilder::new(
        "; INSERT OR REPLACE INTO Dino (id, owner_id, name, filename, created_at, owners, hotness, body, mouth, eyes) "
    );
    builder.push_values(dinos.into_iter(), |mut b, d| {
        b.push_bind(d.id).push_bind(d.owner_id).push_bind(d.name).push_bind(d.filename)
        .push_bind(d.created_at).push_bind(d.owners).push_bind(d.hotness)
        .push_bind(d.body).push_bind(d.mouth).push_bind(d.eyes);
    });

    // Now we can add the transactions.
    builder.push("; INSERT INTO DinoTransactions (dino_id, user_id, gifter_id, type) ");
    builder.push_values(transactions.into_iter(), |mut b, t| {
        b.push_bind(t.dino_id).push_bind(t.user_id).push_bind(t.gifter_id).push_bind(t.kind);
    });

    builder.build()
        .execute(&mut transaction)
        .await
        .expect("Failed to add transactions");

    transaction.commit().await.expect("Failed to commit transaction");
}
