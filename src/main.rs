use std::{env, sync::Arc};

use anyhow::Context as AnyhowContext;
use futures::prelude::*;
use serenity::framework::standard::{
    macros::{command, group},
    CommandResult, StandardFramework,
};
use serenity::model::channel::Message;
use serenity::{async_trait, framework::standard::Args, model::channel::ReactionType};
use serenity::{
    client::{Client, Context, EventHandler},
    prelude::TypeMapKey,
};
use tokio::sync::mpsc::{self, Sender};
use twitter_stream::{Token, TwitterStream};

#[group]
#[commands(add_subscription)]
struct General;

struct Handler;

#[async_trait]
impl EventHandler for Handler {}

struct CommandSender(Sender<Command>);
impl TypeMapKey for CommandSender {
    type Value = Arc<CommandSender>;
}
enum Command {
    AddTwitterSubscription(String),
}
#[tokio::main]
async fn main() {
    let (tx, mut rx) = mpsc::channel(64);
    // TODO coingecko checks

    // Start twitter
    let twitter_manager = tokio::spawn(async move {
        // Establish a connection to the server
        let token = Arc::new(Token::from_parts(
            env::var("TWITTER_CONSUMER_KEY").expect("TWITTER_CONSUMER_KEY"),
            env::var("TWITTER_CONSUMER_SECRET").expect("TWITTER_CONSUMER_SECRET"),
            env::var("TWITTER_ACCESS_KEY").expect("TWITTER_ACCESS_KEY"),
            env::var("TWITTER_ACCESS_SECRET").expect("TWITTER_ACCESS_SECRET"),
        ));

        let mut subscriptions = vec![String::from("Twitter")];
        // Start receiving messages
        while let Some(cmd) = rx.recv().await {
            match cmd {
                Command::AddTwitterSubscription(handle) => {
                    let token_cloned = Arc::clone(&token);

                    if !subscriptions.contains(&handle) {
                        subscriptions.push(handle.clone());
                        tokio::spawn(async move {
                            spawn_twitter(handle, token_cloned).await;
                        });
                    }
                }
            }
        }
    });

    // Send random example
    let _ = tx
        .send(Command::AddTwitterSubscription(String::from("Twitter")))
        .await;

    // Start discord
    let framework = StandardFramework::new()
        .configure(|c| c.prefix("~"))
        .group(&GENERAL_GROUP);

    let token = env::var("DISCORD_TOKEN").expect("discord bot token");
    let mut client = Client::builder(token)
        .event_handler(Handler)
        .framework(framework)
        .await
        .expect("Error creating client");

    {
        let mut data = client.data.write().await;
        data.insert::<CommandSender>(Arc::new(CommandSender(tx.clone())));
    }

    if let Err(why) = client.start().await {
        println!("An error occurred while running the client: {:?}", why);
    }
}

async fn spawn_twitter(handle: String, token: Arc<Token<String, String>>) {
    TwitterStream::track(&handle, &token)
        .try_flatten_stream()
        .try_for_each(|json| {
            // TODO send message to channel
            println!("{}", json);
            future::ok(())
        })
        .await
        .unwrap();
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
        tx.0.send(Command::AddTwitterSubscription(twitter_handle))
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
