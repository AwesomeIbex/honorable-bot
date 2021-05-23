use std::sync::Arc;

use crate::{
    command::{CoingeckoCommand, Command, DiscordCommand, Manager},
    Config,
};
use coingecko_tokio::PriceChangePercentage::OneHour;
use coingecko_tokio::{Market, MarketRequest, Order};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{Receiver, Sender};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct CoingeckoConfig {
    pub sleep_time_secs: u64,
    pub rules: Vec<Rule>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub enum Rule {
    PositivePercent(f32),
    NegativePercent(f32),
    PositiveRank(i16),
    NegativeRank(i16),
}

pub enum RuleResult {
    Percent(bool, Market, f32),
    Rank(bool, Market, i16),
}

impl Manager<CoingeckoCommand> for CoingeckoConfig {
    fn start_manager(
        &self,
        config: Arc<Config>,
        _rx: Receiver<CoingeckoCommand>,
        tx: Sender<Command>,
    ) {
        log::info!("Starting coingecko manager");
        let _ = tokio::spawn(async move {
            let client = coingecko_tokio::Client::new(reqwest::Client::new());

            let req = MarketRequest::new(
                String::from("usd"),
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
                            compare_state(tx.clone(), &state, &new_state, &config).await;
                            state = new_state;
                        }
                        tokio::time::sleep(tokio::time::Duration::from_secs(
                            config.coingecko.sleep_time_secs,
                        ))
                        .await;
                    }
                }
                Err(e) => log::error!("Couldnt get base state for coingecko {:?}", e),
            }
        });
    }
}

fn get_price_diff_pct(initial: &f64, current: &f64) -> f32 {
    (((current / initial) * 100_f64) - 100_f64) as f32
}

async fn compare_state(
    tx: Sender<Command>,
    initial_state: &[Market],
    new_state: &[Market],
    config: &Config,
) {
    for market in new_state {
        let market_initial = initial_state.iter().find(|m| m.id == market.id);
        if let Some(market_initial) = market_initial {
            for res in apply_rules(&config, &market_initial, &market) {
                tx.send(Command::Discord(DiscordCommand::SendCoingeckoRuleResult(
                    res,
                )))
                .await;
            }
        }
    }
}

fn calculate_market_cap_rank_diff(initial: &Market, current: &Market) -> i16 {
    (initial.market_cap_rank - current.market_cap_rank) as i16
}

fn apply_rules(config: &Config, initial: &Market, current: &Market) -> Vec<RuleResult> {
    let mut rule_results: Vec<RuleResult> = vec![];

    config.coingecko.rules.iter().for_each(|rule| match rule {
        Rule::PositivePercent(max) => {
            let price_percentage =
                get_price_diff_pct(&initial.current_price, &current.current_price);
            if price_percentage.is_sign_positive() && price_percentage >= *max {
                rule_results.push(RuleResult::Percent(true, current.clone(), price_percentage))
            }
        }
        Rule::NegativePercent(max) => {
            let price_percentage =
                get_price_diff_pct(&initial.current_price, &current.current_price);
            if price_percentage.is_sign_negative() && price_percentage <= *max {
                rule_results.push(RuleResult::Percent(
                    false,
                    current.clone(),
                    price_percentage,
                ))
            }
        }
        Rule::PositiveRank(max) => {
            let rank_diff = calculate_market_cap_rank_diff(&initial, &current);
            if rank_diff.is_positive() && rank_diff >= *max {
                rule_results.push(RuleResult::Rank(true, current.clone(), rank_diff))
            }
        }
        Rule::NegativeRank(max) => {
            let rank_diff = calculate_market_cap_rank_diff(&initial, &current);
            if rank_diff.is_negative() && rank_diff <= *max {
                rule_results.push(RuleResult::Rank(false, current.clone(), rank_diff))
            }
        }
    });

    rule_results
}
