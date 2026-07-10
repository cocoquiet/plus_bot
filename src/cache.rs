/*
cache.rs
프로젝트를 저장하는 캐쉬 구조 구현
*/

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serenity::all::{UserId};
use tokio::sync::{RwLock, mpsc};

use serenity::prelude::TypeMapKey;
use serenity::http::Http;
use serenity::model::id::GuildId;
use serenity::gateway::ShardManager;

//봇이 전체적으로 공유할 캐쉬 구조체
pub struct BotCache {
    // 유저 아이디로 관리
    pub all_members: HashMap<UserId, String>,
    pub project_mapping: HashMap<String, HashSet<UserId>>,
}

pub struct SharedCacheKey;

impl TypeMapKey for SharedCacheKey {
    type Value = Arc<RwLock<BotCache>>;
}

// 💡 봇 전체에서 "캐시 갱신 신호"를 보낼 수 있도록 Sender를 전역 키로 등록합니다.
pub struct CacheNotifyKey;
impl TypeMapKey for CacheNotifyKey {
    type Value = mpsc::Sender<()>;
}

pub struct ShardManagerContainer;
impl TypeMapKey for ShardManagerContainer {
    type Value = Arc<ShardManager>;
}

// 캐쉬 업데이트 함수
async fn update_cache(cache: &Arc<RwLock<BotCache>>, http: &Arc<Http>, guild_id: GuildId) {
    if let Ok(members) = guild_id.members(&http, None, None).await {
        if let Ok(server_roles) = guild_id.roles(&http).await {
            let mut new_cache = BotCache {
                all_members:HashMap::new(),
                project_mapping: HashMap::new(),
            };

            //맴버 별로 순회하면서 해당 프로젝트에 참여중인지 아닌지 확인
            for member in members {
                let user_id = member.user.id;      // 💡 유저 고유 ID 추출
                let username = member.user.name.clone();

                new_cache.all_members.insert(user_id, username);

                //맴버가 가진 역할과 프로젝트명 비교
                for role_id in &member.roles {
                    // 포함된 프로젝트에 매핑
                    if let Some(role) = server_roles.get(role_id) {
                        new_cache.project_mapping
                            .entry(role.name.clone())
                            .or_insert_with(HashSet::new)
                            .insert(user_id);
                        
                    }
                }
            }

            //새로 갱신한 값 덮어쓰기
            {
                let mut lock = cache.write().await;
                *lock = new_cache;
            }
            println!("백그라운드 데이터 갱신 완료");
        }
    }
}

// 쓰레드 구성
pub fn start_cache_thread(cache: Arc<RwLock<BotCache>>, http: Arc<Http>, guild_id: GuildId) -> mpsc::Sender<()> {
    // 버퍼 크기가 10인 비동기 채널 생성(가동신호 수신용)
    let (tx, mut rx) = mpsc::channel::<()>(10);

    tokio::spawn(async move {
        println!("백그라운드 동기화 스레드 가동");

        // 봇이 켜졌을떄 한 번 연동
        update_cache(&cache, &http, guild_id).await;

        while let Some(_) = rx.recv().await {
            println!("[캐시 갱신] 명령어 요청에 의해 즉시 캐시를 동기화 합니다");
            update_cache(&cache, &http, guild_id).await;
        }

        // 만약 봇이 꺼지거나 tx를 가진 곳이 전부 드롭되면 루프 종료.
        println!("백그라운드 동기화 스레드 종료");
    });

    tx
}