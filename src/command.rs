use std::sync::Arc;

use egg_mode::tweet::Tweet;

use serenity::prelude::TypeMapKey;

use tokio::sync::mpsc::Sender;

pub enum TwitterCommand {
    AddTwitterSubscription(String),
}
pub enum DiscordCommand {
    SendTweet(Tweet),
}
pub struct CommandSender(pub Sender<TwitterCommand>);
impl TypeMapKey for CommandSender {
    type Value = Arc<CommandSender>;
}
