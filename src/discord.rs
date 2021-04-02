use std::sync::Arc;

use anyhow::Context as AnyhowContext;

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

use crate::{
    command::{CommandSender, DiscordCommand, TwitterCommand},
    Config,
};

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
        tx.0.send(TwitterCommand::AddTwitterSubscription(twitter_handle))
            .await
    {
        log::error!("Failed to send add twitter sub {}", e)
    }
    Ok(())
}

pub fn start_manager(
    config_cloned: Arc<Config>,
    mut rx: Receiver<DiscordCommand>,
    cmd_tx: Sender<TwitterCommand>,
) {
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
    });
}
