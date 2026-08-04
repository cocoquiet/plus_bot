/*
cache.rs
프로젝트를 저장하는 캐쉬 구조 구현
*/

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serenity::all::{ChannelType, UserId};
use serenity::gateway::ShardManager;
use serenity::http::Http;
use serenity::model::id::GuildId;
use serenity::prelude::TypeMapKey;

use tokio::sync::{mpsc, RwLock};

//봇이 전체적으로 공유할 캐쉬 구조체
pub struct BotCache {
    // 유저 아이디로 관리
    pub all_members: HashMap<UserId, String>,
    pub project_mapping: HashMap<String, HashSet<UserId>>,
    pub project_pms: HashMap<String, UserId>,
}

// 캐시 스레드가 처리할 명령 목록
pub enum CacheCommand {
    RefreshAll, // 모든 캐시를 갱신
    UpdateSingleMember {
        // 특정 유저의 이름만 타겟팅해서 즉시 갱신 (추후 확장용)
        user_id: serenity::model::id::UserId,
        display_name: String,
    },
    AddProjectMembers {
        // 특정 프로젝트에 참여중인 유저 목록 추가
        project_name: String,
        user_ids: HashSet<UserId>,
    },
    RemoveProjectMembers {
        // 특정 프로젝트에 참여중인 유저 목록 제거
        project_name: String,
        user_ids: HashSet<UserId>,
    },
}

pub struct SharedCacheKey;

impl TypeMapKey for SharedCacheKey {
    type Value = Arc<RwLock<BotCache>>;
}

// 💡 봇 전체에서 "캐시 갱신 신호"를 보낼 수 있도록 Sender를 전역 키로 등록합니다.
pub struct CacheNotifyKey;
impl TypeMapKey for CacheNotifyKey {
    type Value = mpsc::Sender<CacheCommand>;
}

pub struct ShardManagerContainer;
impl TypeMapKey for ShardManagerContainer {
    type Value = Arc<ShardManager>;
}

// 쓰레드 구성
pub fn start_cache_thread(
    cache: Arc<RwLock<BotCache>>,
    http: Arc<Http>,
    guild_id: GuildId,
) -> mpsc::Sender<CacheCommand> {
    // 버퍼 크기가 32인 비동기 채널 생성(가동신호 수신용)
    let (tx, mut rx) = mpsc::channel::<CacheCommand>(32);

    tokio::spawn(async move {
        println!("백그라운드 동기화 스레드 가동");

        // 봇이 켜졌을떄 한 번 연동
        refresh_cache(&cache, &http, guild_id).await;

        while let Some(command) = rx.recv().await {
            match command {
                CacheCommand::RefreshAll => {
                    println!("[캐시] 전체 캐시 즉시 동기화 요청 처리 중...");
                    refresh_cache(&cache, &http, guild_id).await;
                }
                CacheCommand::UpdateSingleMember {
                    user_id,
                    display_name,
                } => {
                    println!("[캐시] {} 님의 단일 캐시 업데이트 중...", display_name);
                    update_single_member(&cache, user_id, display_name).await;
                }
                CacheCommand::AddProjectMembers {
                    project_name,
                    user_ids,
                } => {
                    println!("[캐시] {} 프로젝트 참여자 목록 추가 중...", project_name);
                    add_project_members(&cache, project_name, user_ids).await;
                }
                CacheCommand::RemoveProjectMembers {
                    project_name,
                    user_ids,
                } => {
                    println!("[캐시] {} 프로젝트에서 참여자 제외 중...", project_name);
                    remove_project_members(&cache, project_name, user_ids).await;
                }
            }
        }
        // 만약 봇이 꺼지거나 tx를 가진 곳이 전부 드롭되면 루프 종료.
        println!("백그라운드 동기화 스레드 종료");
    });

    tx
}

// 캐쉬 갱신 함수
async fn refresh_cache(cache: &Arc<RwLock<BotCache>>, http: &Arc<Http>, guild_id: GuildId) {
    if let Ok(members) = guild_id.members(&http, None, None).await {
        if let Ok(server_roles) = guild_id.roles(&http).await {
            let mut new_cache = BotCache {
                all_members: HashMap::new(),
                project_mapping: HashMap::new(),
                project_pms: HashMap::new(),
            };

            // 맴버 별로 순회하면서 해당 프로젝트에 참여중인지 아닌지 확인
            for member in members {
                if member.user.bot {
                    continue; // 봇은 캐시에서 제외
                }

                let user_id = member.user.id; // 💡 유저 고유 ID 추출
                let username = member.display_name(); // 서버에 표시되는 이름 추출

                new_cache.all_members.insert(user_id, username.to_string());

                //맴버가 가진 역할과 프로젝트명 비교
                for role_id in &member.roles {
                    // 포함된 프로젝트에 매핑
                    if let Some(role) = server_roles.get(role_id) {
                        new_cache
                            .project_mapping
                            .entry(role.name.clone())
                            .or_insert_with(HashSet::new)
                            .insert(user_id);
                    }
                }
            }
            // 프로젝트 목록을 받아와서 PM추출하기
            if let Ok(channels) = guild_id.channels(&http).await {
                for (_channel_id, channel) in channels {
                    // 카테고리(프로젝트)이면서 이름에 "(PM: " 택스트가 붙어 있는지 필터링
                    if channel.kind == ChannelType::Category && channel.name.contains("(PM: ") {
                        //프로젝트명과 pm이름 분리
                        let parts: Vec<&str> = channel.name.split("(PM: ").collect();
                        if parts.len() == 2 {
                            let project_name = parts[0].to_string();
                            let pm_name = parts[1].trim_end_matches(')');

                            // pm이름을 이용해 user_id 역추척하기
                            if let Some((&pm_id, _)) = new_cache
                                .all_members
                                .iter()
                                .find(|(_, name)| **name == pm_name)
                            {
                                new_cache.project_pms.insert(project_name, pm_id);
                            }
                        }
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

// 단일 유저 캐시 갱신
async fn update_single_member(
    cache: &Arc<RwLock<BotCache>>,
    user_id: UserId,
    display_name: String,
) {
    let mut guard = cache.write().await;
    guard.all_members.insert(user_id, display_name);
}

// 프로젝트 참여자 추가
async fn add_project_members(
    cache: &Arc<RwLock<BotCache>>,
    project_name: String,
    user_ids: HashSet<UserId>,
) {
    let mut guard = cache.write().await;
    guard
        .project_mapping
        .entry(project_name)
        .or_default()
        .extend(user_ids);
}

// 프로젝트 참여자 제거
async fn remove_project_members(
    cache: &Arc<RwLock<BotCache>>,
    project_name: String,
    user_ids: HashSet<UserId>,
) {
    let mut guard = cache.write().await;

    // 💡 프로젝트가 존재할 때만 내부 HashSet을 가져와서 수정합니다.
    if let Some(members) = guard.project_mapping.get_mut(&project_name) {
        for id in user_ids {
            members.remove(&id);
        }

        // 만약 탈퇴 후 프로젝트에 아무도 안 남았다면 맵에서 프로젝트 자체를 삭제
        if members.is_empty() {
            guard.project_mapping.remove(&project_name);
            println!(
                "[캐시] {} 프로젝트에 참여자가 없어 매핑을 완전히 삭제했습니다.",
                project_name
            );
        }
    }
}
