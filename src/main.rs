use std::{env, sync::Arc};

use anyhow::Context as AnyhowContext;
use futures::prelude::*;
use serde_json::Value;
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
    model::id::ChannelId,
};
use tokio::sync::mpsc::{self, Sender};
use twitter_stream::{Credentials, Token, TwitterStream};

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
    Send(Value),
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

    let discord_tx_cloned = Arc::new(discord_tx);
    let twitter_manager = tokio::spawn(async move {
        println!("Starting connection to twitter..");
        let token = Arc::new(Token::from_parts(
            env::var("TWITTER_CONSUMER_KEY").expect("TWITTER_CONSUMER_KEY"),
            env::var("TWITTER_CONSUMER_SECRET").expect("TWITTER_CONSUMER_SECRET"),
            env::var("TWITTER_ACCESS_KEY").expect("TWITTER_ACCESS_KEY"),
            env::var("TWITTER_ACCESS_SECRET").expect("TWITTER_ACCESS_SECRET"),
        ));

        let mut subscriptions = vec![];
        // Start receiving messages
        while let Some(cmd) = cmd_rx.recv().await {
            match cmd {
                TwitterCommand::AddTwitterSubscription(handle) => {
                    let token_cloned = Arc::clone(&token);
                    let discord_tx = Arc::clone(&discord_tx_cloned);

                    if !subscriptions.contains(&handle) {
                        subscriptions.push(handle.clone());
                        tokio::spawn(async move {
                            spawn_twitter(handle, token_cloned, discord_tx).await;
                        });
                    }
                }
            }
        }
    });

    println!("Sending random example..");
    let _ = cmd_tx
        .send(TwitterCommand::AddTwitterSubscription(String::from(
            "Polkadot",
        )))
        .await;

    let discord_manager = tokio::spawn(async move {
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
                DiscordCommand::Send(value) => {
                    if let Err(e) = http.send_message(826371481499860993, &value).await {
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
