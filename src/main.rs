use std::{
    fs::{File, OpenOptions},
    io::{BufReader, Write},
    sync::Arc,
};

use command::TwitterCommand;
use discord::DiscordConfig;

use serde::{Deserialize, Serialize};

use tokio::sync::mpsc::{self};
use twitter::TwitterConfig;

pub mod command;
pub mod discord;
pub mod twitter;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Config {
    pub twitter: TwitterConfig,
    pub discord: DiscordConfig,
}
impl Config {
    fn persist(&self) -> Result<(), anyhow::Error> {
        let mut file = OpenOptions::new()
            .append(false)
            .write(true)
            .create(true)
            .open("config.json")?;
        file.write_all(serde_json::to_string_pretty(self)?.as_bytes())?;

        Ok(())
    }
    fn read() -> Result<Arc<Config>, anyhow::Error> {
        let config = File::open("config.json")?;
        let reader = BufReader::new(config);
        Ok(Arc::new(serde_json::from_reader(reader)?))
    }
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    pretty_env_logger::init();
    let config = Config::read()?;

    let (cmd_tx, cmd_rx) = mpsc::channel(64);
    let (discord_tx, discord_rx) = mpsc::channel(64);

    // discord, twitter, coingecko threads all sending to big command rx select with tx's back to dtc
    // TODO coingecko checks

    twitter::start_manager(Arc::clone(&config), cmd_rx, discord_tx);

    let _ = cmd_tx
        .send(TwitterCommand::AddTwitterSubscription(String::from(
            "Polkadot",
        )))
        .await;

    discord::start_manager(Arc::clone(&config), discord_rx, cmd_tx);

    loop {}
    Ok(())
}
