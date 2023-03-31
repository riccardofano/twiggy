# Rust port of [TwiggyBot](https://github.com/Brexbot/TwiggyBot) just for fun

## How to run

Prerequisites:

- Rust (https://rustup.rs/)

In order to compile the project a database needs to be setup. For that your need to install the SQLx CLI.

```
cargo install sqlx-cli
```

Then create the database and run the migrations with:

```
sqlx create database
sqlx migrate run
```

Now you just need to provide a valid Discord Token.

```bash
# Linux and MacOS
export DISCORD_TOKEN=<token>
# Powershell
$Env:DISCORD_TOKEN="<token>"
```

And then run the bot.

```bash
cargo run
# or in the release mode if you're a try-hard
cargo run --release
```
