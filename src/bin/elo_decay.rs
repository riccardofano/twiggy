use sqlx::{Connection, Row, Sqlite, SqliteConnection, Transaction};

const ELO_DECAY_FACTOR: f64 = 0.01;

#[tokio::main]
async fn main() {
    let mut db = SqliteConnection::connect("./database.sqlite")
        .await
        .expect("Failed to connect to database");
    let mut transaction = db.begin().await.expect("Failed to begin transaction");

    let initial_tally: i64 = sqlx::query("SELECT SUM(elo_rank) FROM RpgCharacter")
        .fetch_one(&mut transaction)
        .await
        .expect("Failed to get initial elo_rank tally")
        .get_unchecked(0);

    sqlx::query!(
        "UPDATE RpgCharacter SET
        elo_rank = CAST(ROUND((1 - $1) * elo_rank + $1 * 1000 + elo_rank) - elo_rank AS INTEGER)",
        ELO_DECAY_FACTOR
    )
    .execute(&mut transaction)
    .await
    .expect("Failed to update the RPG Character's elo");

    let decayed_tally: i64 = sqlx::query("SELECT SUM(elo_rank) FROM RpgCharacter")
        .fetch_one(&mut transaction)
        .await
        .expect("Failed to get elo_rank tally after decay")
        .get_unchecked(0);

    balance_economy(&mut transaction, initial_tally, decayed_tally).await;

    transaction
        .commit()
        .await
        .expect("Failed to commit transaction");
}

// Original comment by background_nose:
// Due to rounding, points can get lost or created during the adjustment
// Over time this would cause the "average skill" to move away from 1000
// So we have tallied up this "Elo drift" to make a pool of missing points
// Sort it by Elo rank, allowing us to take points from the rich, and give them
// to the poor. As required to keep the status quo.
async fn balance_economy(
    transaction: &mut Transaction<'_, Sqlite>,
    initial_tally: i64,
    decayed_tally: i64,
) {
    let drift = initial_tally - decayed_tally;
    println!("Initial tally: {initial_tally:>6}");
    println!("Decayed tally: {decayed_tally:>6}");
    println!("Elo drift:     {drift:>6}");

    let drift_signum = drift.signum();
    let order_query = match drift_signum {
        -1 => "SELECT user_id, elo_rank FROM RPGCharacter ORDER BY elo_rank DESC LIMIT ?",
        1 => "SELECT user_id, elo_rank FROM RPGCharacter ORDER BY elo_rank ASC LIMIT ?",
        _ => return,
    };
    let rows = sqlx::query(order_query)
        .bind(drift.abs())
        .fetch_all(&mut *transaction)
        .await
        .expect("Failed to select the users that should get rebalanced");

    for row in rows {
        let user_id: &str = row.get_unchecked(0);
        let old_elo: i64 = row.get_unchecked(1);
        println!(
            "Rebalancing {user_id}'s elo, from {old_elo} to {}...",
            old_elo + drift_signum
        );
        sqlx::query("UPDATE RPGCharacter SET elo_rank = elo_rank + ? WHERE user_id = ?")
            .bind(drift_signum)
            .bind(user_id)
            .execute(&mut *transaction)
            .await
            .expect("Failed to update elo_rank for <@{user_id}>");
    }

    let average: f64 = sqlx::query("SELECT AVG(elo_rank) FROM RPGCharacter")
        .fetch_one(transaction)
        .await
        .expect("Failed to fetch new average elo_rank")
        .get_unchecked(0);
    println!("Average elo now: {}", average)
}
