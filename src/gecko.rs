use std::sync::Arc;

use crate::{
    command::{CoingeckoCommand, Command, DiscordCommand, Manager},
    Config,
};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{Receiver, Sender};
use coingecko_tokio::MarketRequest;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct CoingeckoConfig {
    pub sleep_time_secs: u64,
}

impl Manager<CoingeckoCommand> for CoingeckoConfig {
    fn start_manager(
        &self,
        config_cloned: Arc<Config>,
        _rx: Receiver<CoingeckoCommand>,
        tx: Sender<Command>,
    ) {
        log::info!("Starting coingecko manager");
        let _ = tokio::spawn(async move {
            let client = coingecko_tokio::Client::new(reqwest::Client::new());

            // loop and call list
            // delay for config length

            if let Ok(state) = client.coins_list().await {
                println!("Found {} coins in list", state.len());
                let req = MarketRequest::new();
                // TODO the response type of this is shit, it should be something better than this.
                let state = client.simple_price(req).await.unwrap(); //TODO remove me

                let _ = tx
                    .send(Command::Discord(DiscordCommand::SendCoingeckoBase(state)))
                    .await;

                loop {
                    if let Ok(_new_state) = client.simple_price(req).await {
                        // Compare the states, if any condition to jump is met, send a message to discord
                    }
                    tokio::time::sleep(tokio::time::Duration::from_secs(
                        config_cloned.coingecko.sleep_time_secs,
                    ))
                    .await;
                }
            }
        });
    }
}
