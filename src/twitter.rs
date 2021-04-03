use std::sync::Arc;

use egg_mode::{stream::StreamMessage, KeyPair};
use futures::prelude::*;
use serde::{Deserialize, Serialize};

use tokio::sync::mpsc::{Receiver, Sender};

use crate::{Config, command::{Command, DiscordCommand, Manager, TwitterCommand}, discord};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct TwitterConfig {
    pub consumer_key: String,
    pub consumer_secret: String,
    pub user_access_key: String,
    pub user_access_secret: String,
    pub subscriptions: Vec<String>,
}

impl Manager<TwitterCommand> for TwitterConfig {
    fn start_manager(
        &self,
        config_cloned: Arc<Config>,
        mut rx: Receiver<TwitterCommand>,
        tx: Sender<Command>,
    ) {
        let _ = tokio::spawn(async move {
            let consumer = KeyPair::new(
                config_cloned.twitter.consumer_key.clone(),
                config_cloned.twitter.consumer_secret.clone(),
            );
            let access = KeyPair::new(
                config_cloned.twitter.user_access_key.clone(),
                config_cloned.twitter.user_access_secret.clone(),
            );
            let token = egg_mode::Token::Access { consumer, access };
    
            // Spawn a new task to handle the operations on the subscription list
            let c = config_cloned.clone();
            tokio::spawn(async move {
                while let Some(cmd) = rx.recv().await {
                    match cmd {
                        TwitterCommand::AddTwitterSubscription(handle) => {
                            if !c.twitter.subscriptions.contains(&handle) {
                                let mut subscriptions = c.twitter.subscriptions.clone();
                                subscriptions.push(handle);
                                Config {
                                    twitter: TwitterConfig {
                                        subscriptions,
                                        ..c.twitter.clone()
                                    },
                                    discord: discord::DiscordConfig {
                                        ..c.discord.clone()
                                    },
                                }
                                .persist()
                                .unwrap(); //TODO remove
                            }
                        }
                    }
                }
            });
    
            // curl 'https://tweeterid.com/ajax.php' -H 'User-Agent: Mozilla/5.0 (X11; Linux x86_64; rv:89.0) Gecko/20100101 Firefox/89.0' -H 'Accept: */*' -H 'Accept-Language: en-US,en;q=0.5' --compressed -H 'Content-Type: application/x-www-form-urlencoded; charset=UTF-8' -H 'X-Requested-With: XMLHttpRequest' -H 'Origin: https://tweeterid.com' -H 'Connection: keep-alive' -H 'Referer: https://tweeterid.com/' -H 'Sec-Fetch-Dest: empty' -H 'Sec-Fetch-Mode: cors' -H 'Sec-Fetch-Site: same-origin' -H 'Pragma: no-cache' -H 'Cache-Control: no-cache' --data-raw 'input=%40polkadot'
            let mut ids = vec![];
            for handle in config_cloned.twitter.subscriptions.clone() {
                let mut search = egg_mode::user::search(handle, &token);
                match search.try_next().await {
                    Ok(Some(u)) => ids.push(u.id),
                    Err(e) => log::error!("Failed to search {}", e),
                    _ => {}
                }
            }
    
            let discord_tx = Arc::new(tx);
            egg_mode::stream::filter()
                .follow(&ids)
                .start(&token)
                .try_for_each(|m| {
                    if let StreamMessage::Tweet(tweet) = m {
                        if let Some(src) = tweet.user.clone() {
                            if ids.contains(&src.id) {
                                let discord_tx_cloned = Arc::clone(&discord_tx);
                                let _ = tokio::spawn(async move {
                                    if let Err(e) = discord_tx_cloned
                                        .send(Command::Discord(DiscordCommand::SendTweet(
                                            tweet.clone(),
                                        )))
                                        .await
                                    {
                                        log::error!("Failed to send command {}", e);
                                    }
                                });
                            }
                        }
                    }
                    futures::future::ready(Ok(()))
                })
                .await
                .expect("Stream error");
        });
    }
}
