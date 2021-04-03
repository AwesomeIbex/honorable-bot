use std::{collections::HashMap, sync::Arc};

use crate::{
    command::{CoingeckoCommand, Command, DiscordCommand, Manager},
    Config,
};
use coingecko::SimplePriceReq;
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
            let client = coingecko::Client::new(reqwest::Client::new());

            // loop and call list
            // delay for config length

            if let Ok(state) = client.coins_list().await {
                println!("Found {} coins in list", state.len());
                let coin_ids = state
                    .iter()
                    .skip(6595) // There is 6607, really me might wanna take the top 500 and then do a chunked iterator and flatten them for our state
                    .map(|coin| coin.id.to_string())
                    .collect::<Vec<String>>()
                    .join(",");
                let req = SimplePriceReq::new(coin_ids.clone(), "usd".into()) //TODO problem with req not being cloned
                    .include_market_cap()
                    .include_24hr_vol()
                    .include_24hr_change()
                    .include_last_updated_at();
                // TODO the response type of this is shit, it should be something better than this.
                let state = client.simple_price(req).await.unwrap(); //TODO remove me

                let _ = tx
                    .send(Command::Discord(DiscordCommand::SendCoingeckoBase(state)))
                    .await;

                loop {
                    //TODO clone is implemented here but weird caching issue
                    let req = SimplePriceReq::new(coin_ids.clone(), "usd".into())
                        .include_market_cap()
                        .include_24hr_vol()
                        .include_24hr_change()
                        .include_last_updated_at();
                    if let Ok(new_state) = client.simple_price(req).await {
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
