use std::{env, sync::Arc};

use anyhow::Context as AnyhowContext;
use egg_mode::{stream::StreamMessage, tweet::Tweet, KeyPair};
use futures::prelude::*;
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
use twitter_stream::{Token, TwitterStream};

#[group]
#[commands(add_subscription)]
struct General;

struct Handler;

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
#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    pretty_env_logger::init();
    dotenv::dotenv().ok();
    let (cmd_tx, mut cmd_rx) = mpsc::channel(64);
    let (discord_tx, mut discord_rx) = mpsc::channel(64);

    // discord, twitter, coingecko threads all sending to big command rx with tx's back to dtc
    // TODO coingecko checks
    // TODO file persistence

    let _twitter_manager = tokio::spawn(async move {
        println!("Starting connection to twitter..");
        let c_key = env::var("TWITTER_CONSUMER_KEY").expect("TWITTER_CONSUMER_KEY");
        let c_secret = env::var("TWITTER_CONSUMER_SECRET").expect("TWITTER_CONSUMER_SECRET");
        let access_key = env::var("TWITTER_ACCESS_KEY").expect("TWITTER_ACCESS_KEY");
        let access_secret = env::var("TWITTER_ACCESS_SECRET").expect("TWITTER_ACCESS_SECRET");

        let consumer = KeyPair::new(c_key, c_secret);
        let access = KeyPair::new(access_key, access_secret);
        let token = egg_mode::Token::Access { consumer, access };
        let subscriptions = vec![
            String::from("Polkadot"),
            String::from("Cardano"),
            String::from("Filecoin"),
            String::from("Microsoft"),
            String::from("Apple"),
            String::from("Bitcoin"),
            String::from("elonmusk"),
        ];

        // Spawn a new task to handle the operations on the subscription list
        let mut subscriptions_cloned = subscriptions.clone();
        tokio::spawn(async move {
            while let Some(cmd) = cmd_rx.recv().await {
                match cmd {
                    TwitterCommand::AddTwitterSubscription(handle) => {
                        if !subscriptions_cloned.contains(&handle) {
                            subscriptions_cloned.push(handle);
                            // Maybe could short circuit app and then reload from config???
                            // panic!("Reboot!");                        }
                        }
                    }
                }
            }
        });

        // curl 'https://tweeterid.com/ajax.php' -H 'User-Agent: Mozilla/5.0 (X11; Linux x86_64; rv:89.0) Gecko/20100101 Firefox/89.0' -H 'Accept: */*' -H 'Accept-Language: en-US,en;q=0.5' --compressed -H 'Content-Type: application/x-www-form-urlencoded; charset=UTF-8' -H 'X-Requested-With: XMLHttpRequest' -H 'Origin: https://tweeterid.com' -H 'Connection: keep-alive' -H 'Referer: https://tweeterid.com/' -H 'Sec-Fetch-Dest: empty' -H 'Sec-Fetch-Mode: cors' -H 'Sec-Fetch-Site: same-origin' -H 'Pragma: no-cache' -H 'Cache-Control: no-cache' --data-raw 'input=%40polkadot'
        let mut ids = vec![];
        for handle in subscriptions {
            let mut search = egg_mode::user::search(handle, &token);
            match search.try_next().await {
                Ok(Some(u)) => {
                    println!("Found twitter user {:?}", u.id);
                    ids.push(u.id)
                }
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
                        if !ids.contains(&src.id) {
                            let discord_tx_cloned = Arc::clone(&discord_tx);
                            let _ = tokio::spawn(async move {
                                discord_tx_cloned
                                    .send(DiscordCommand::SendTweet(tweet.clone()))
                                    .await;
                                println!(
                                    "Received tweet from {}:\n{}\n",
                                    tweet.user.unwrap().name,
                                    tweet.text
                                );
                            });
                        }
                    }
                }
                futures::future::ready(Ok(()))
            })
            .await
            .expect("Stream error");
    });

    println!("Sending random example..");
    let _ = cmd_tx
        .send(TwitterCommand::AddTwitterSubscription(String::from(
            "Polkadot",
        )))
        .await;

    let _discord_manager = tokio::spawn(async move {
        println!("Starting connection to discord..");
        let framework = StandardFramework::new()
            .configure(|c| c.prefix("~"))
            .group(&GENERAL_GROUP);

        let token = env::var("DISCORD_TOKEN").expect("discord bot token");

        let mut client = Client::builder(token.clone())
            .event_handler(Handler)
            .framework(framework)
            .await
            .expect("Error creating client");
        let http = Http::new_with_token(&token);

        {
            let mut data = client.data.write().await;
            data.insert::<CommandSender>(Arc::new(CommandSender(cmd_tx.clone())));
        }

        // spawn this elsewhere
        let client_manager = tokio::spawn(async move {
            println!("Creating discord client..");
            if let Err(why) = client.start().await {
                println!("An error occurred while running the client: {:?}", why);
            }
        });

        while let Some(cmd) = discord_rx.recv().await {
            match cmd {
                DiscordCommand::SendTweet(tweet) => {
                    let user_screen_name = tweet.user.as_ref().unwrap().screen_name.clone();
                    let tweet_url = format!("https://twitter.com/{}/status/{}", user_screen_name, tweet.id);
                    let usr = tweet.user.unwrap();
                    if let Err(e) = http
                        .send_message(
                            826371481499860993,
                            &serde_json::json!({
                                "content": tweet_url,
                                "type": "article",
                                "embed": {
                                    "url": tweet_url,
                                    "image": {
                                        "url": usr.profile_image_url
                                    },
                                    "title": usr.name,
                                    "description":"Has tweeted!",
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

async fn spawn_twitter(
    handle: String,
    token: Arc<Token<String, String>>,
    tx: Arc<Sender<DiscordCommand>>,
) {
    if let Err(e) = TwitterStream::track(&handle, &token)
        .try_flatten_stream()
        .try_for_each(|json| {
            println!("{}", json);
            // TODO send message to channel
            // tx.send(DiscordCommand::Send(json));
            future::ok(())
        })
        .await
    {
        log::error!("Failed to handle twitter stream {}", e)
    }
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
