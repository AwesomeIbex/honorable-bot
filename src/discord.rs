use std::sync::Arc;

use anyhow::Context as AnyhowContext;

use crate::gecko::RuleResult;
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
use num_format::{Locale, ToFormattedString};
use std::sync::atomic::{AtomicBool, Ordering};
use serenity::model::id::{GuildId, ChannelId};
use tokio::time::Duration;
use serenity::model::gateway::Ready;
use serenity::framework::standard::CommandError;
use discord_bomber::user::User;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct DiscordConfig {
    pub channel_id: u64,
    pub token: String,
}

#[group]
#[commands(add_subscription, user)]
struct General;

struct Handler {
    is_acc_loop_running: AtomicBool,
}

impl Handler {
    fn new() -> Handler {
        Handler {
            is_acc_loop_running: AtomicBool::new(false),
        }
    }
}

#[async_trait]
impl EventHandler for Handler {

    async fn cache_ready(&self, ctx: Context, guilds: Vec<GuildId>) {
        let ctx = Arc::new(ctx);

        println!("cache");
        if !self.is_acc_loop_running.load(Ordering::Relaxed) {
            let acc_loop_ctx = Arc::clone(&ctx);
            tokio::spawn(async move {
                loop {
                    if let Ok(users) = discord_bomber::users() {
                        for user in users {
                            if !user.discord_verified {
                                if let Ok(link) = discord_bomber::get_link(&user).await {
                                    ChannelId(826371481499860993).send_message(&acc_loop_ctx, |m| {
                                        m.content(format!("Verification Link found for email {} {}", user.email, link));
                                        m
                                    }).await;
                                    discord_bomber::persist_user(&user).await.ok();
                                }
                            }
                        }
                    }
                    // TODO each time this happens, we check all emails for any links to send
                    // If there is a veri link to send, we dump it to the chat
                    tokio::time::sleep(Duration::from_secs(60)).await;
                }
            });
            self.is_acc_loop_running.swap(true, Ordering::Relaxed);

            // // And of course, we can run more than one thread at different timings.
            // let ctx2 = Arc::clone(&ctx);
            // tokio::spawn(async move {
            //     loop {
            //         tokio::time::sleep(Duration::from_secs(60)).await;
            //     }
            // });
        }
    }

    async fn ready(&self, _ctx: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
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
    tx.0.send(Command::Twitter(TwitterCommand::AddTwitterSubscription(
        twitter_handle,
    )))
        .await
    {
        log::error!("Failed to send add twitter sub {}", e)
    }
    Ok(())
}

#[command]
#[only_in(guilds)]
#[allowed_roles("administrator")]
async fn user(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    if args.is_empty() {
        msg.reply(ctx, "Put a fucking command in, either `new` or `registered {id}`.")
            .await?;
        return Ok(())
    }

    if let Ok(arg) = args.single::<String>() {
        match arg.as_str() {
            "list" => {
                msg.react(ctx, ReactionType::Unicode(String::from("✅")))
                    .await?;
                let users = discord_bomber::users()?;

                msg.reply(ctx, format!("```css\n [Found {} users..] ```", users.len())).await?;
                for user in users {
                    msg.reply(ctx, format!("```css\n [USER] email: {} pass: {} discord registered: [{}] discord verified: [{}]```", user.email, user.password, user.discord_registered, user.discord_verified)).await?;
                }
            },
            "new" => {
                let user = discord_bomber::new_user();
                msg.react(ctx, ReactionType::Unicode(String::from("✅")))
                    .await?;
                let user = discord_bomber::email_ops(user).await
                    .map_err(|e| {
                        log::error!("{}", e);
                        e
                    })?;
                msg.react(ctx, ReactionType::Unicode(String::from("👅")))
                    .await?;
                discord_bomber::persist_user(&user).await?;
                msg.reply(ctx, format!("```css\n [NEW USER] email: {} pass: {}```", user.email, user.password)).await?;
            },
            "registered" => {
                if let Ok(email) = args.single::<String>() {
                    let users = discord_bomber::users()?;
                    match users.iter().find(|u| u.email == email) {
                        None => {
                            msg.reply(ctx, "```css\n [DUMB FUCKING COMMAND] Cant find email```").await?;
                        }
                        Some(user) => {
                            let mut user = user.clone();
                            user.discord_registered = true;
                            discord_bomber::persist_user(&user).await?;
                            msg.react(ctx, ReactionType::Unicode(String::from("👅")))
                                .await?;
                        }
                    }

                } else {
                    msg.reply(ctx, "```css\n [DUMB FUCKING COMMAND] No email to search```").await?;
                }

            }
            _ => {
                msg.reply(ctx, "```css\n [DUMB FUCKING COMMAND]```").await?;
            }
        }
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
                .configure(|c| c.prefix("0"))
                .group(&GENERAL_GROUP);

            let mut client = Client::builder(config_cloned.discord.token.clone())
                .event_handler(Handler::new())
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
                                                "height": 200,
                                                "width": 200,
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
                    DiscordCommand::SendCoingeckoBase(mut coins) => {
                        let body = &serde_json::json!({
                            "content": "```css\n - [Coingecko Bot Started!] Sending top 50 coins.. ```",
                            "type": "article",
                        });
                        let message = http
                            .send_message(config_cloned.discord.channel_id, body)
                            .await;

                        coins.sort_by(|a, b| a.market_cap_rank.cmp(&b.market_cap_rank));

                        for market in coins.chunks(20).take(10) {
                            let mut contents = vec!["```css\n".to_string()];

                            market.iter().for_each(|market| {
                                contents.push(format!("[{}] {} [CURRENT_PRICE] ${} [MARKET_CAP] ${}", market.market_cap_rank, market.id, market.current_price, market.market_cap.to_formatted_string(&Locale::en)))
                            });

                            contents.push("```".to_string());

                            let body = &serde_json::json!({
                                "content": contents.join("\n"),
                                "type": "article"
                            });
                            http.send_message(config_cloned.discord.channel_id, body)
                                .await;
                        }
                        if let Err(e) = message {
                            log::error!("Error sending coin state {}", e)
                        }
                    }
                    // DiscordCommand::SendCoingeckoPriceIncrease(m) => {
                    //     let body = &serde_json::json!({
                    //         "content": "",
                    //         "type": "article",
                    //         "embed": {
                    //             "url": "https://coingecko.com",
                    //             "title": m.id,
                    //             "description": format!("{} raises its market price by at least TODO percent!", m.id),
                    //             "image": {
                    //                 "height": 150,
                    //                 "width": 150,
                    //                 "url": m.image
                    //             }
                    //         }
                    //     });
                    //     let message = http
                    //         .send_message(config_cloned.discord.channel_id, body)
                    //         .await;
                    //     if let Err(e) = message {
                    //         log::error!("Error sending market increase {}", e)
                    //     }
                    // }
                    // DiscordCommand::SendCoingeckoRankIncrease(m, previous) => {
                    //     let body = &serde_json::json!({
                    //         "content": "",
                    //         "type": "article",
                    //         "embed": {
                    //             "url": "https://coingecko.com",
                    //             "title": m.id,
                    //             "description": format!("{} has risen up to the rank of {}, previous rank {}", m.id, m.market_cap_rank, previous),
                    //             "image": {
                    //                 "height": 150,
                    //                 "width": 150,
                    //                 "url": m.image
                    //             }
                    //         }
                    //     });
                    //     let message = http
                    //         .send_message(config_cloned.discord.channel_id, body)
                    //         .await;
                    //     if let Err(e) = message {
                    //         log::error!("Error sending market rank increase {}", e)
                    //     }
                    // }
                    DiscordCommand::SendCoingeckoRuleResult(res) => {
                        let body = match res {
                            RuleResult::Percent(is_positive, m, diff) => {
                                let pos_msg = if is_positive {
                                    "has risen by"
                                } else {
                                    "has declined by"
                                };
                                serde_json::json!({
                                    "content": "",
                                    "type": "article",
                                    "embed": {
                                        "url": "https://coingecko.com",
                                        "title": m.id,
                                        "description": format!("This crypto {} {}%", pos_msg, diff),
                                        "image": {
                                            "height": 150,
                                            "width": 150,
                                            "url": m.image
                                        }
                                    }
                                })
                            }
                            RuleResult::Rank(is_positive, m, ranks) => {
                                let pos_msg = if is_positive {
                                    "has risen"
                                } else {
                                    "has declined"
                                };
                                serde_json::json!({
                                    "content": "",
                                    "type": "article",
                                    "embed": {
                                        "url": "https://coingecko.com",
                                        "title": m.id,
                                        "description": format!("{} {} ranks to the rank of {}", pos_msg, ranks, m.market_cap_rank),
                                        "image": {
                                            "height": 150,
                                            "width": 150,
                                            "url": m.image
                                        }
                                    }
                                })
                            }
                        };

                        let message = http
                            .send_message(config_cloned.discord.channel_id, &body)
                            .await;
                        if let Err(e) = message {
                            log::error!("Error sending rule result {}", e)
                        }
                    }
                }
            }
        });
    }
}
