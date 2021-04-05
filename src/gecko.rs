use std::sync::Arc;

use crate::{
    command::{CoingeckoCommand, Command, DiscordCommand, Manager},
    Config,
};
use coingecko_tokio::{Market, MarketRequest, Order};
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

            let req = MarketRequest::new(
                String::from("gbp"),
                None,
                None,
                Some(Order::MarketCapDesc),
                Some(250),
                None,
                None,
            );

            match client.markets(req.clone()).await {
                Ok(mut state) => {
                    let _ = tx
                        .send(Command::Discord(DiscordCommand::SendCoingeckoBase(
                            state.clone(),
                        )))
                        .await;

                    loop {
                        if let Ok(new_state) = client.markets(req.clone()).await {
                            compare_state(tx.clone(), &state, new_state.clone()).await;
                            state = new_state;
                        }
                        tokio::time::sleep(tokio::time::Duration::from_secs(
                            config_cloned.coingecko.sleep_time_secs,
                        ))
                        .await;
                    }
                }
                Err(e) => log::error!("Couldnt get base state for coingecko {}", e),
            }
        });
    }
}

async fn compare_state(tx: Sender<Command>, initial_state: &[Market], new_state: Vec<Market>) {
    for market in new_state {
        let market_initial = initial_state.iter().find(|m| m.id == market.id);
        if let Some(market_initial) = market_initial {
            let has_risen_price =
                ((market.current_price / market_initial.current_price) * 100_f64) >= 120_f64;
            let raised_ranking = (market_initial.market_cap_rank - market.market_cap_rank) >= 5;

            if has_risen_price {
                tx.send(Command::Discord(
                    DiscordCommand::SendCoingeckoPriceIncrease(market.clone()),
                ))
                .await;
            }
            if raised_ranking {
                tx.send(Command::Discord(DiscordCommand::SendCoingeckoRankIncrease(
                    market,
                    market_initial.market_cap_rank
                )))
                .await;
            }
        }
    }
    // If any of the new states coins has risen by X percent
    // If any of the new states coins has raised rankings by 2, who did it overtake?
}
