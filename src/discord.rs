use std::sync::Arc;

use anyhow::Context as AnyhowContext;

use crate::{
    command::{Command, CommandSender, DiscordCommand, Manager, TwitterCommand},
    Config,
};
use serde::{Deserialize, Serialize};
use serenity::client::{Client, Context, EventHandler};
use serenity::model::channel::Message;
use serenity::{async_trait, framework::standard::Args, model::channel::ReactionType};
use serenity::{
    framework::standard::{
        macros::{command, group},
        CommandResult, StandardFramework,
    },
    http::Http,
};
use tokio::sync::mpsc::{Receiver, Sender};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct DiscordConfig {
    pub channel_id: u64,
    pub token: String,
}

#[group]
#[commands(add_subscription)]
struct General;

struct Handler; //TODO can store shit in here apparently

#[async_trait]
impl EventHandler for Handler {}

#[command]
#[only_in(guilds)]
#[allowed_roles("administrator")]
async fn add_subscription(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    if args.is_empty() {
        msg.reply(ctx, "You need to provide a twitter handle.")
            .await?;
    } else {
        msg.react(ctx, ReactionType::Unicode(String::from("✅")))
            .await?;
    };

    let twitter_handle = args
        .single::<String>()
        .context("No twitter handle provided")?;
    msg.react(ctx, ReactionType::Unicode(String::from("👅")))
        .await?;

    let data = ctx.data.read().await;
    let tx = data
        .get::<CommandSender>()
        .expect("Expected CommandSender in TypeMap.");

    if let Err(e) =
        tx.0.send(Command::Twitter(TwitterCommand::AddTwitterSubscription(
            twitter_handle,
        )))
        .await
    {
        log::error!("Failed to send add twitter sub {}", e)
    }
    Ok(())
}

impl Manager<DiscordCommand> for DiscordConfig {
    fn start_manager(
        &self,
        config_cloned: Arc<Config>,
        mut rx: Receiver<DiscordCommand>,
        tx: Sender<Command>,
    ) {
        log::info!("Starting discord manager");
        let _ = tokio::spawn(async move {
            let framework = StandardFramework::new()
                .configure(|c| c.prefix("~"))
                .group(&GENERAL_GROUP);

            let mut client = Client::builder(config_cloned.discord.token.clone())
                .event_handler(Handler)
                .framework(framework)
                .await
                .expect("Error creating client");
            let http = Http::new_with_token(&config_cloned.discord.token);

            {
                let mut data = client.data.write().await;
                data.insert::<CommandSender>(Arc::new(CommandSender(tx.clone())));
            }

            let _client_manager = tokio::spawn(async move {
                if let Err(why) = client.start().await {
                    println!("An error occurred while running the client: {:?}", why);
                }
            });

            while let Some(cmd) = rx.recv().await {
                match cmd {
                    DiscordCommand::SendTweet(tweet) => {
                        let user_screen_name = tweet.user.as_ref().unwrap().screen_name.clone();
                        let tweet_url = format!(
                            "https://twitter.com/{}/status/{}",
                            user_screen_name, tweet.id
                        );
                        let usr = tweet.user.unwrap();
                        if let Err(e) = http
                            .send_message(
                                config_cloned.discord.channel_id,
                                &serde_json::json!({
                                    "content": tweet_url,
                                    "type": "article",
                                    "embed": {
                                        "url": tweet_url,
                                        "embed": {
                                            "url": tweet_url,
                                            "image": {
                                                "height": 300,
                                                "width": 300,
                                                "url": usr.profile_image_url
                                            },
                                            "title": usr.name,
                                            "description": tweet.text,
                                            "provider": {
                                                "url": tweet_url,
                                                "name": "test"
                                            }
                                        },
                                        "title": usr.name,
                                        "description": tweet.text,
                                        "provider": {
                                            "url": tweet_url,
                                            "name": "test"
                                        }
                                    }
                                }),
                            )
                            .await
                        {
                            log::error!("Error sending message {}", e)
                        }
                    }
                    DiscordCommand::SendCoingeckoBase(coins) => {
                        println!("coins: {:#?}", coins);
                        let body = &serde_json::json!({
                            "content": "asdasdasdsa",
                            "type": "article",
                            "embed": {
                                "url": "https://coingecko.com",
                                "title": "Coingecko base right now",
                                "description": "This is the base for the current coingecko state"
                            }
                        });
                        let message = http
                            .send_message(config_cloned.discord.channel_id, body)
                            .await;
                        if let Err(e) = message {
                            log::error!("Error sending coin state {}", e)
                        }
                    }
                    DiscordCommand::SendCoingeckoPriceIncrease(m) => {
                        let body = &serde_json::json!({
                            "content": "Coin price increase!",
                            "type": "article",
                            "embed": {
                                "url": "https://coingecko.com",
                                "title": m.id,
                                "description": format!("{} raises its market price by X percent!", m.id),
                                "image": {
                                    "height": 300,
                                    "width": 300,
                                    "url": m.image
                                }
                            }
                        });
                        let message = http
                            .send_message(config_cloned.discord.channel_id, body)
                            .await;
                        if let Err(e) = message {
                            log::error!("Error sending market increase {}", e)
                        }
                    }
                    DiscordCommand::SendCoingeckoRankIncrease(m) => {
                        let body = &serde_json::json!({
                            "content": "Market cap rank increase!",
                            "type": "article",
                            "embed": {
                                "url": "https://coingecko.com",
                                "title": m.id,
                                "description": format!("{} has risen up to the rank of {}!", m.id, m.market_cap_rank),
                                "image": {
                                    "height": 300,
                                    "width": 300,
                                    "url": m.image
                                }
                            }
                        });
                        let message = http
                            .send_message(config_cloned.discord.channel_id, body)
                            .await;
                        if let Err(e) = message {
                            log::error!("Error sending market increase {}", e)
                        }
                    }
                }
            }
        });
    }
}
