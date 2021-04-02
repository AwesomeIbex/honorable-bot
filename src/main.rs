use std::{
    env,
    fs::{File, OpenOptions},
    io::{BufReader, Write},
    sync::Arc,
};

use anyhow::Context as AnyhowContext;
use egg_mode::{stream::StreamMessage, tweet::Tweet, KeyPair};
use futures::prelude::*;
use serde::{Deserialize, Serialize};
use serenity::model::channel::Message;
use serenity::{async_trait, framework::standard::Args, model::channel::ReactionType};
use serenity::{
    client::{Client, Context, EventHandler},
    prelude::TypeMapKey,
};
use serenity::{
    framework::standard::{
        macros::{command, group},
        CommandResult, StandardFramework,
    },
    http::Http,
};
use tokio::sync::mpsc::{self, Sender};

#[group]
#[commands(add_subscription)]
struct General;

struct Handler; //TODO can store shit in here apparently

#[async_trait]
impl EventHandler for Handler {}

struct CommandSender(Sender<TwitterCommand>);
impl TypeMapKey for CommandSender {
    type Value = Arc<CommandSender>;
}
enum TwitterCommand {
    AddTwitterSubscription(String),
}
enum DiscordCommand {
    SendTweet(Tweet),
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct Config {
    twitter: Twitter,
    discord: Discord,
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
#[derive(Deserialize, Serialize, Debug, Clone)]
struct Twitter {
    consumer_key: String,
    consumer_secret: String,
    user_access_key: String,
    user_access_secret: String,
    subscriptions: Vec<String>,
}
#[derive(Deserialize, Serialize, Debug, Clone)]
struct Discord {
    channel_id: u64,
    token: String,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    pretty_env_logger::init();
    let config = Config::read()?;

    let (cmd_tx, mut cmd_rx) = mpsc::channel(64);
    let (discord_tx, mut discord_rx) = mpsc::channel(64);

    // discord, twitter, coingecko threads all sending to big command rx select with tx's back to dtc
    // TODO coingecko checks

    let config_cloned = Arc::clone(&config);
    let _twitter_manager = tokio::spawn(async move {
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
            while let Some(cmd) = cmd_rx.recv().await {
                match cmd {
                    TwitterCommand::AddTwitterSubscription(handle) => {
                        if !c.twitter.subscriptions.contains(&handle) {
                            let mut subscriptions = c.twitter.subscriptions.clone();
                            subscriptions.push(handle);
                            Config {
                                twitter: Twitter {
                                    subscriptions,
                                    ..c.twitter.clone()
                                },
                                discord: Discord {
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

        let discord_tx = Arc::new(discord_tx);
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
                                    .send(DiscordCommand::SendTweet(tweet.clone()))
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

    let _ = cmd_tx
        .send(TwitterCommand::AddTwitterSubscription(String::from(
            "Polkadot",
        )))
        .await;

    let config_cloned = Arc::clone(&config);
    let _discord_manager = tokio::spawn(async move {
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
            data.insert::<CommandSender>(Arc::new(CommandSender(cmd_tx.clone())));
        }

        let _client_manager = tokio::spawn(async move {
            if let Err(why) = client.start().await {
                println!("An error occurred while running the client: {:?}", why);
            }
        });

        while let Some(cmd) = discord_rx.recv().await {
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
                                    "image": {
                                        "url": usr.profile_image_url
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
            }
        }
    })
    .await; //TODO wait

    Ok(())
}

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
        tx.0.send(TwitterCommand::AddTwitterSubscription(twitter_handle))
            .await
    {
        log::error!("Failed to send add twitter sub {}", e)
    }

    // TODO write confirmation
    // let channel = ctx.cache
    //     .clone()
    //     .channel(ctx.channel)
    //     .await
    //     .context("Failed to find channel with this id")?;
    // channel
    //     .id()
    //     .say(&ctx, args.rest())
    //     .await
    //     .context("Failed to send message to channel")?;

    Ok(())
}
