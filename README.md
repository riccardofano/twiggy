# Rust port of [TwiggyBot](https://github.com/Brexbot/TwiggyBot) just for fun

I was curious to see what the bot would look like in Rust, after someone mentioned the "rewrite it in rust" meme for the bot.
I'm pretty surprised by the result, I honestly expected it to be a lot more verbose.

## How to run

Prerequisites:

- Rust (https://rustup.rs/)

In order to compile the project a database needs to be setup. For that your need to install the SQLx CLI.

```
cargo install sqlx-cli
```

Then create the database and run the migrations with:

```
sqlx database create
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

## Extra commands

To use the /ask command you need to set `WOLFRAM_APP_ID` to a valid Wolfram Alpha APP ID in the environment variables.

You can get one here: https://developer.wolframalpha.com/
