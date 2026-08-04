use std::fmt::Write;

use serenity::all::{
    Channel, CommandDataOptionValue, CommandInteraction, CommandOptionType, CreateCommand,
    CreateCommandOption,
};
use serenity::builder::EditInteractionResponse;
use serenity::prelude::*;

use crate::cache::{CacheCommand, CacheNotifyKey};

// 앞으로 해야할거
// 추후 개발방향: 노션에 연동해서 프로젝트 참여 인원 확인 하기

// 1. 명령어 등록 함수
pub fn register_member_command() -> CreateCommand {
    CreateCommand::new("member")
        .description("프로젝트 멤버를 관리하는 명령어입니다.")
        .dm_permission(false)
        // 1. list 서브커맨드 (/member list)
        .add_option(CreateCommandOption::new(
            CommandOptionType::SubCommand,
            "list",
            "현재 프로젝트에 참여 중인 인원 목록을 확인합니다.",
        ))
        // 2. add 서브커맨드 (/member add [user1] [user2] [user3])
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::SubCommand,
                "add",
                "프로젝트에 새 멤버를 추가하고 역할을 부여합니다.",
            )
            .add_sub_option(
                CreateCommandOption::new(CommandOptionType::User, "user1", "추가할 첫 번째 유저")
                    .required(true),
            )
            .add_sub_option(
                CreateCommandOption::new(
                    CommandOptionType::User,
                    "user2",
                    "추가할 두 번째 유저 (선택)",
                )
                .required(false),
            )
            .add_sub_option(
                CreateCommandOption::new(
                    CommandOptionType::User,
                    "user3",
                    "추가할 세 번째 유저 (선택)",
                )
                .required(false),
            ),
        )
        // 3. remove 서브커맨드 (/member remove [user1] [user2] [user3])
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::SubCommand,
                "remove",
                "프로젝트에서 멤버를 내보내고 역할을 회수합니다.",
            )
            .add_sub_option(
                CreateCommandOption::new(CommandOptionType::User, "user1", "내보낼 첫 번째 유저")
                    .required(true),
            )
            .add_sub_option(
                CreateCommandOption::new(
                    CommandOptionType::User,
                    "user2",
                    "내보낼 두 번째 유저 (선택)",
                )
                .required(false),
            )
            .add_sub_option(
                CreateCommandOption::new(
                    CommandOptionType::User,
                    "user3",
                    "내보낼 세 번째 유저 (선택)",
                )
                .required(false),
            ),
        )
}

// 2. 명령어 실행 함수
pub async fn run_member_command(
    ctx: &Context,
    command: &CommandInteraction,
) -> serenity::Result<()> {
    // 타임아웃 방지를 위한 상호작용 defer 처리
    command.defer(&ctx.http).await?;

    // 명령어가 입력된 채널 정보 가져오기
    let channel = match command.channel_id.to_channel(&ctx.http).await? {
        Channel::Guild(ch) => ch,
        _ => {
            command
                .edit_response(
                    &ctx.http,
                    EditInteractionResponse::new()
                        .content("❌ 이 명령어는 서버 텍스트 채널에서만 사용 가능합니다."),
                )
                .await?;
            return Ok(());
        }
    };

    // 카테고리 정보 추출을 통해 프로젝트 이름 확인
    let project_name = match channel.parent_id {
        Some(category_id) => {
            let category_channel: serenity::all::GuildChannel =
                category_id.to_channel(&ctx.http).await?.guild().unwrap();
            let category_name_parts: Vec<&str> = category_channel.name.split("(PM: ").collect();
            category_name_parts[0].trim().to_string()
        }
        None => {
            command
                .edit_response(
                    &ctx.http,
                    EditInteractionResponse::new()
                        .content("❌ 이 명령어는 프로젝트 카테고리 내 채널에서만 사용 가능합니다."),
                )
                .await?;
            return Ok(());
        }
    };

    // 전역 공유 캐시 가져오기
    let data_read = ctx.data.read().await;
    let cache_lock = data_read
        .get::<crate::cache::SharedCacheKey>()
        .expect("보관함에 캐시가 없습니다.")
        .clone();
    let cache = cache_lock.read().await;

    // 첫 번째 옵션에서 어떤 서브커맨드가 들어왔는지 확인
    let subcommand_option = match command.data.options.first() {
        Some(opt) => opt,
        None => {
            command
                .edit_response(
                    &ctx.http,
                    EditInteractionResponse::new().content("⚠️ 올바른 하위 명령어를 선택해주세요."),
                )
                .await?;
            return Ok(());
        }
    };

    // 캐시 스레드 채널 송신자(Sender) 획득
    let tx = data_read
        .get::<CacheNotifyKey>()
        .expect("보관함에 캐시 갱신 신호가 없습니다.")
        .clone();

    match subcommand_option.name.as_str() {
        // --- 1. 참여 인원 조회 (/member list) ---
        "list" => {
            let included_set = cache.project_mapping.get(&project_name);
            let mut included_mems = Vec::new();

            let pm_id = match cache.project_pms.get(&project_name) {
                Some(id) => id,
                None => {
                    command
                        .edit_response(
                            &ctx.http,
                            EditInteractionResponse::new().content("❌ 프로젝트 PM을 찾을 수 없습니다. 프로젝트 카테고리에서 명령어를 사용해주세요."),
                        )
                        .await?;
                    return Ok(());
                }
            };
            included_mems.push(cache.all_members.get(&pm_id).unwrap().to_string());

            for (user_id, username) in &cache.all_members {
                if *user_id == *pm_id {
                    continue; // PM은 이미 추가했으므로 건너뛰기
                }

                if let Some(set) = included_set {
                    if set.contains(user_id) {
                        included_mems.push(username.clone());
                    }
                }
            }

            // 참여 인원 명단을 문자열로 포맷팅
            let mut content = String::new();
            if included_mems.is_empty() {
                // 프로젝트에 참여 인원이 없을 경우 안내 메시지
                content.push_str(&format!(
                    "❌ 서버에서 '{}' 프로젝트에 해당하는 역할을 찾을 수 없습니다.\n",
                    project_name
                ));
            } else {
                content.push_str(&format!(
                    "📌 **'{}' 프로젝트 참여 인원 명단**\n",
                    project_name
                ));
                for mem in included_mems {
                    // content.push_str(&format!("• {}\n", mem)); // 반복문 안에서 format 사용 시 성능 저하 우려
                    write!(content, "• {}\n", mem).unwrap(); // write! 매크로는 버퍼 뒤에 바로 문자열을 포매팅해 추가해 성능저하 적음
                }
            }

            command
                .edit_response(&ctx.http, EditInteractionResponse::new().content(content))
                .await?;
        }

        // --- 2. 멤버 추가 / 내보내기 공통 로직 (add / remove) ---
        "add" | "remove" => {
            let is_add = subcommand_option.name == "add";

            let guild_id = match command.guild_id {
                Some(id) => id,
                None => return Ok(()),
            };

            let guild = guild_id.to_partial_guild(&ctx.http).await?;
            let member = match &command.member {
                Some(m) => m,
                None => return Ok(()),
            };

            // 권한 체크 (PM 이거나 관리자 권한을 가진 유저인지 판별)
            let is_pm = match cache.project_pms.get(&project_name) {
                Some(pm_id) => *pm_id == command.user.id,
                None => false,
            };

            let mut is_admin = guild.owner_id == command.user.id;
            if !is_admin {
                for role_id in &member.roles {
                    if let Some(role) = guild.roles.get(role_id) {
                        if role.permissions.administrator() {
                            is_admin = true;
                            break;
                        }
                    }
                }
            }

            if !is_pm && !is_admin {
                command
                    .edit_response(
                        &ctx.http,
                        EditInteractionResponse::new().content(
                            "❌ 이 명령어를 사용할 권한이 없습니다. (PM 혹은 관리자만 가능)",
                        ),
                    )
                    .await?;
                return Ok(());
            }

            // 서버 내에서 대응되는 프로젝트 역할 ID 검색
            let mut target_role_id = None;
            if let Ok(roles) = guild_id.roles(&ctx.http).await {
                if let Some(role) = roles.values().find(|r| r.name == project_name) {
                    target_role_id = Some(role.id);
                }
            }

            let role_id = match target_role_id {
                Some(id) => id,
                None => {
                    command
                        .edit_response(
                            &ctx.http,
                            EditInteractionResponse::new().content(format!(
                                "❌ 서버에서 '{}' 프로젝트에 해당하는 역할을 찾을 수 없습니다.",
                                project_name
                            )),
                        )
                        .await?;
                    return Ok(());
                }
            };

            // 현재 이미 참여 중인 기존 캐시 데이터 획득
            let already_members = cache
                .project_mapping
                .get(&project_name)
                .cloned()
                .unwrap_or_default();
            let mut target_user_ids = Vec::new();

            // 슬래시 커맨드 파라미터(user1, user2, user3) 순회하며 유저 ID 추출
            if let CommandDataOptionValue::SubCommand(sub_opts) = &subcommand_option.value {
                for opt in sub_opts {
                    if let CommandDataOptionValue::User(user_id) = opt.value {
                        if user_id == command.user.id {
                            command
                                .edit_response(
                                    &ctx.http,
                                    EditInteractionResponse::new()
                                        .content("❌ 본인을 추가하거나 제거할 수 없습니다."),
                                )
                                .await?;
                            return Ok(());
                        }
                        target_user_ids.push(user_id);
                    }
                }
            }
            target_user_ids.dedup(); // 중복 유저 ID 제거

            let mut processed_users = Vec::new();

            for user_id in target_user_ids {
                // 봇이나 서버에 존재하지 않는 유저는 캐시에 없으므로 건너뛰기
                if cache.all_members.get(&user_id).is_none() {
                    continue;
                }

                if is_add {
                    // 추가 모드일 때 이미 존재하는 유저는 건너뛰기
                    if already_members.contains(&user_id) {
                        continue;
                    }

                    if ctx
                        .http
                        .add_member_role(guild_id, user_id, role_id, None)
                        .await
                        .is_ok()
                    {
                        processed_users.push(user_id);
                    }
                } else {
                    // 제거 모드일 때 멤버 명단에 없는 유저는 건너뛰기
                    if !already_members.contains(&user_id) {
                        continue;
                    }

                    if ctx
                        .http
                        .remove_member_role(guild_id, user_id, role_id, None)
                        .await
                        .is_ok()
                    {
                        processed_users.push(user_id);
                    }
                }
            }

            if processed_users.is_empty() {
                command
                    .edit_response(
                        &ctx.http,
                        EditInteractionResponse::new()
                            .content("❌ 변경된 정보가 없거나 적용할 대상 유저가 없습니다."),
                    )
                    .await?;
            } else {
                let mentions: Vec<String> = processed_users
                    .iter()
                    .map(|id| format!("<@{}>", id))
                    .collect();

                if is_add {
                    command
                        .edit_response(
                            &ctx.http,
                            EditInteractionResponse::new().content(format!(
                                "✅ {} 님이 '{}' 프로젝트 멤버로 추가되었습니다!",
                                mentions.join(", "),
                                project_name
                            )),
                        )
                        .await?;

                    // 캐시 동기화 신호 전송
                    let _ = tx
                        .send(CacheCommand::AddProjectMembers {
                            project_name: project_name.clone(),
                            user_ids: processed_users.into_iter().collect(),
                        })
                        .await;
                } else {
                    command
                        .edit_response(
                            &ctx.http,
                            EditInteractionResponse::new().content(format!(
                                "🗑️ {} 님이 '{}' 프로젝트 멤버에서 제외되었습니다.",
                                mentions.join(", "),
                                project_name
                            )),
                        )
                        .await?;

                    // 캐시 동기화 신호 전송
                    let _ = tx
                        .send(CacheCommand::RemoveProjectMembers {
                            project_name: project_name.clone(),
                            user_ids: processed_users.into_iter().collect(),
                        })
                        .await;
                }
            }
        }
        _ => {}
    }

    Ok(())
}
