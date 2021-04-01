use futures::prelude::*;
use tokio::sync::mpsc;
use twitter_stream::{Token, TwitterStream};

enum Command {
    AddTwitterSubscription(String),
}
#[tokio::main]
async fn main() {
    let (tx, mut rx) = mpsc::channel(64);

    let twitter_manager = tokio::spawn(async move {
        // Establish a connection to the server
        let token = Token::from_parts(
            "consumer_key",
            "consumer_secret",
            "access_key",
            "access_secret",
        );

        let mut subscriptions = vec![String::from("Twitter")];
        // Start receiving messages
        while let Some(cmd) = rx.recv().await {
            match cmd {
                Command::AddTwitterSubscription(handle) => {
                    if !subscriptions.contains(&handle) {
                        subscriptions.push(handle.clone());
                        tokio::spawn(async move {
                            spawn_twitter(handle, token).await;
                        });
                    }
                }
            }
        }
    });

    let _ = tx
        .send(Command::AddTwitterSubscription(String::from("Twitter")))
        .await;
}

async fn spawn_twitter(handle: String, token: Token<&str, &str>) {
    // TODO channel for this
    TwitterStream::track(&handle, &token)
        .try_flatten_stream()
        .try_for_each(|json| {
            println!("{}", json);
            future::ok(())
        })
        .await
        .unwrap();
}

// TODO command from discord to add to list of twitter streams
