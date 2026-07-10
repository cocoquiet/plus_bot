//! Requires the 'framework' feature flag be enabled in your project's `Cargo.toml`.
//!
//! This can be enabled by specifying the feature in the dependency section:
//!
//! ```toml
//! [dependencies.serenity]
//! git = "https://github.com/serenity-rs/serenity.git"
//! features = ["framework", "standard_framework"]
//! ```
#![allow(deprecated)] // We recommend migrating to poise, instead of using the standard command framework.
mod commands;
mod cache; // 봇이 관리할 캐쉬 모듈 등록

use std::collections::HashMap;
use std::collections::HashSet;
use std::env;
use std::sync::Arc;

use serenity::async_trait;
use serenity::framework::standard::macros::group;
use serenity::framework::standard::Configuration;
use serenity::framework::StandardFramework;
use serenity::http::Http;
use serenity::model::id::GuildId;
use serenity::model::event::ResumedEvent;
use serenity::model::gateway::Ready;
use serenity::prelude::*;

use tracing::{error, info};

use crate::cache::ShardManagerContainer;

use crate::commands::project::*;
use crate::commands::member::*;

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _: Context, ready: Ready) {
        info!("Connected as {}", ready.user.name);
    }

    async fn resume(&self, _: Context, _: ResumedEvent) {
        info!("Resumed");
    }
}

#[group]
#[commands(project, member)]
struct General;

#[tokio::main]
async fn main() {
    // 현재 작업중인 CWD에 대한 상대경로로 .env파일에서 환경변수 로드
    dotenv::dotenv().expect("Failed to load .env file");

    // Initialize the logger to use environment variables.
    //
    // In this case, a good default is setting the environment variable `RUST_LOG` to `debug`.
    tracing_subscriber::fmt::init();

    //env파일에서 토큰 및 서버 id 로드
    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");
    let guild_id = GuildId::new(
        env::var("SERVER_ID")
            .expect("Expected SERVER_ID in env")
            .parse::<u64>()
            .expect("Expected a server id in the environment")
    );

    let http = Http::new(&token);

    // We will fetch your bot's owners and id
    let (owners, _bot_id) = match http.get_current_application_info().await {
        Ok(info) => {
            let mut owners = HashSet::new();
            if let Some(owner) = &info.owner {
                owners.insert(owner.id);
            }

            (owners, info.id)
        },
        Err(why) => panic!("Could not access application info: {:?}", why),
    };

    // Create the framework
    let framework = StandardFramework::new().group(&GENERAL_GROUP);
    framework.configure(Configuration::new().owners(owners).prefix("/"));

    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;
    let mut client = Client::builder(&token, intents)
        .framework(framework)
        .event_handler(Handler)
        .await
        .expect("Err creating client");

    let shared_cache = Arc::new(RwLock::new(cache::BotCache {
        all_members: HashMap::new(),
        project_mapping: HashMap::new(),
    }));

    {
        let mut data = client.data.write().await;
        data.insert::<ShardManagerContainer>(client.shard_manager.clone());
        data.insert::<cache::SharedCacheKey>(shared_cache.clone());
    }

    // Ctrl + C 종료 핸들러 스레드 시작
    let shard_manager = client.shard_manager.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.expect("Could not register ctrl+c handler");
        shard_manager.shutdown_all().await;
    });

    //스레드 시작
    cache::start_cache_thread(shared_cache.clone(), client.http.clone(), guild_id);

    if let Err(why) = client.start().await {
        error!("Client error: {:?}", why);
    }
}