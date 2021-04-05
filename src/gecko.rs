use std::sync::Arc;

use crate::{
    command::{CoingeckoCommand, Command, DiscordCommand, Manager},
    Config,
};
use coingecko_tokio::{MarketRequest, Order};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{Receiver, Sender};

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
                let req = MarketRequest::new(
                    String::from("gbp"),
                    None,
                    None,
                    Some(Order::MarketCapDesc),
                    Some(250),
                    None,
                    None,
                    None,
                );

                if let Ok(mut state) = client.markets(req.clone()).await {
                    let _ = tx
                        .send(Command::Discord(DiscordCommand::SendCoingeckoBase(state)))
                        .await;

                    loop {
                        if let Ok(_new_state) = client.markets(req.clone()).await {
                            // Compare the states, if any condition to jump is met, send a message to discord
                        }
                        tokio::time::sleep(tokio::time::Duration::from_secs(
                            config_cloned.coingecko.sleep_time_secs,
                        ))
                        .await;
                    }
                } else {
                    log::error!("Couldnt start Coingecko loop, couldnt get base state.")
                }
            }
        });
    }
}
