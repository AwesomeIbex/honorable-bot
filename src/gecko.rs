use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{Receiver, Sender};

use crate::{
    command::{CoingeckoCommand, Command, Manager},
    Config,
};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct CoingeckoConfig {
    pub sleep_time_secs: u64,
}

impl Manager<CoingeckoCommand> for CoingeckoConfig {
    fn start_manager(
        &self,
        config_cloned: Arc<Config>,
        mut rx: Receiver<CoingeckoCommand>,
        tx: Sender<Command>,
    ) {
        log::info!("Starting coingecko manager");
        let _ = tokio::spawn(async move {
            let client = coingecko::Client::new(reqwest::Client::new());

            // loop and call list
            // delay for config length

            if let Ok(state) = client.coins_list().await {
                loop {
                    if let Ok(new_state) = client.coins_list().await {
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
