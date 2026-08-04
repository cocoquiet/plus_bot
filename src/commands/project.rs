use serenity::all::{CommandOptionType, CreateCommand, CreateCommandOption, Permissions};
use serenity::builder::{CreateChannel, EditChannel, EditInteractionResponse, EditRole};
use serenity::model::application::{CommandDataOptionValue, CommandInteraction};
use serenity::model::channel::{
    Channel, ChannelType, PermissionOverwrite, PermissionOverwriteType,
};
use serenity::model::id::RoleId;
use serenity::prelude::*;

use crate::cache::{CacheCommand, CacheNotifyKey};

pub fn register_project_command() -> CreateCommand {
    CreateCommand::new("project")
        .description("프로젝트 관련 명령어")
        .dm_permission(false)
        .default_member_permissions(Permissions::MANAGE_CHANNELS)
        // 1. generate 서브커맨드 (/project generate [name])
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::SubCommand,
                "generate",
                "새 프로젝트를 생성합니다.",
            )
            .add_sub_option(
                CreateCommandOption::new(CommandOptionType::String, "name", "생성할 프로젝트 이름")
                    .required(true),
            ),
        )
        // 2. rename 서브커맨드 (/project rename [new_name])
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::SubCommand,
                "rename",
                "프로젝트 이름을 변경합니다.",
            )
            .add_sub_option(
                CreateCommandOption::new(
                    CommandOptionType::String,
                    "new_name",
                    "변경할 새 프로젝트 이름",
                )
                .required(true),
            ),
        )
        // 3. delete 서브커맨드 (/project delete)
        .add_option(CreateCommandOption::new(
            CommandOptionType::SubCommand,
            "delete",
            "프로젝트를 완전히 삭제합니다.",
        ))
}

// 2. 슬래시 커맨드 실행 함수 (Message/Args 대신 CommandInteraction 사용)
pub async fn run_project_command(
    ctx: &Context,
    command: &CommandInteraction,
) -> serenity::Result<()> {
    // 슬래시 커맨드는 3초 이내 응답이 필수적이므로 먼저 defer(생각 중...) 처리로 타임아웃 방지
    command.defer(&ctx.http).await?;

    let guild_id = match command.guild_id {
        Some(id) => id,
        None => {
            command
                .edit_response(
                    &ctx.http,
                    EditInteractionResponse::new()
                        .content("❌ 이 명령어는 서버 안에서만 사용 가능합니다."),
                )
                .await?;
            return Ok(());
        }
    };

    // 첫 번째 옵션에서 서브커맨드 추출
    let subcommand_option = match command.data.options.first() {
        Some(opt) => opt,
        None => {
            command
                .edit_response(
                    &ctx.http,
                    EditInteractionResponse::new().content("❌ 올바른 하위 명령어를 선택해주세요."),
                )
                .await?;
            return Ok(());
        }
    };

    // 공유 캐시 및 알림 채널 불러오기
    let data_read = ctx.data.read().await;
    let cache_lock = data_read
        .get::<crate::cache::SharedCacheKey>()
        .expect("보관함에 캐시가 없습니다.")
        .clone();
    let cache = cache_lock.read().await;

    let tx = data_read
        .get::<CacheNotifyKey>()
        .expect("보관함에 캐시 갱신 신호가 없습니다.")
        .clone();

    // 서브커맨드 이름 매칭 분기
    match subcommand_option.name.as_str() {
        // --- 1. 프로젝트 생성 (/project generate [name]) ---
        "generate" => {
            let mut project_name = String::new();

            // 서브커맨드 하위에 포함된 인자(name) 추출
            if let CommandDataOptionValue::SubCommand(sub_opts) = &subcommand_option.value {
                if let Some(opt) = sub_opts.iter().find(|o| o.name == "name") {
                    if let CommandDataOptionValue::String(val) = &opt.value {
                        project_name = val.clone();
                    }
                }
            }

            if project_name.is_empty() {
                command
                    .edit_response(
                        &ctx.http,
                        EditInteractionResponse::new()
                            .content("⚠️ 생성할 프로젝트 이름을 제대로 입력해주세요."),
                    )
                    .await?;
                return Ok(());
            }

            // 동명의 프로젝트 검증
            let exists = cache.project_mapping.keys().any(|k| k == &project_name);
            if exists {
                command
                    .edit_response(
                        &ctx.http,
                        EditInteractionResponse::new().content(format!(
                            "❌ 이미 '{}' 이름의 프로젝트가 존재합니다.",
                            project_name
                        )),
                    )
                    .await?;
                return Ok(());
            }

            command
                .edit_response(
                    &ctx.http,
                    EditInteractionResponse::new().content(format!(
                        "🏗️ '{}' 프로젝트 생성을 시작합니다. 세팅 중...",
                        project_name
                    )),
                )
                .await?;

            // 🤖 봇 자신 ID 가져오기
            let bot_id = match ctx.http.get_current_user().await {
                Ok(user) => user.id,
                Err(why) => {
                    command
                        .edit_response(
                            &ctx.http,
                            EditInteractionResponse::new()
                                .content(format!("❌ 봇 정보를 가져오지 못했습니다: {:?}", why)),
                        )
                        .await?;
                    return Ok(());
                }
            };

            // 역할 생성
            let role_builder = EditRole::new().name(&project_name);
            let project_role = match guild_id.create_role(&ctx.http, role_builder).await {
                Ok(role) => role,
                Err(why) => {
                    command
                        .edit_response(
                            &ctx.http,
                            EditInteractionResponse::new()
                                .content(format!("❌ 역할 생성 실패: {:?}", why)),
                        )
                        .await?;
                    return Ok(());
                }
            };

            // 명령어를 실행한 멤버에게 역할 부여
            if let Err(why) = ctx
                .http
                .add_member_role(guild_id, command.user.id, project_role.id, None)
                .await
            {
                println!("⚠️ [generate] 유저에게 역할 부여 실패: {:?}", why);
            }

            // 권한 오버라이트 설정
            let everyone_role_id = RoleId::new(guild_id.get());
            let deny_everyone = PermissionOverwrite {
                allow: Permissions::empty(),
                deny: Permissions::VIEW_CHANNEL,
                kind: PermissionOverwriteType::Role(everyone_role_id),
            };

            let allow_project_role = PermissionOverwrite {
                allow: Permissions::VIEW_CHANNEL | Permissions::SEND_MESSAGES,
                deny: Permissions::empty(),
                kind: PermissionOverwriteType::Role(project_role.id),
            };

            let allow_bot = PermissionOverwrite {
                allow: Permissions::VIEW_CHANNEL
                    | Permissions::SEND_MESSAGES
                    | Permissions::MANAGE_CHANNELS,
                deny: Permissions::empty(),
                kind: PermissionOverwriteType::Member(bot_id),
            };

            // 프로젝트 멤버들의 채팅을 막기 위한 오버라이트 (View만 허용, Send 차단)
            let readonly_project_role = PermissionOverwrite {
                allow: Permissions::VIEW_CHANNEL,
                deny: Permissions::SEND_MESSAGES, // 👈 깃허브, 정보 채널 등에서 발언권 차단
                kind: PermissionOverwriteType::Role(project_role.id),
            };

            // PM(명령어 호출자)에게만 발언권을 주기 위한 오버라이트
            let allow_pm = PermissionOverwrite {
                allow: Permissions::VIEW_CHANNEL | Permissions::SEND_MESSAGES,
                deny: Permissions::empty(),
                kind: PermissionOverwriteType::Member(command.user.id), // 👈 PM 개인 유저 ID
            };

            // PM 이름 빌드 및 캐시 동기화
            let pm_name = match cache.all_members.get(&command.user.id) {
                Some(name) => name.clone(),
                None => {
                    let latest_name = command
                        .member
                        .as_ref()
                        .and_then(|m| m.nick.clone())
                        .unwrap_or_else(|| {
                            command
                                .user
                                .global_name
                                .clone()
                                .unwrap_or_else(|| command.user.name.clone())
                        });

                    let _ = tx
                        .send(CacheCommand::UpdateSingleMember {
                            user_id: command.user.id,
                            display_name: command.user.name.clone(),
                        })
                        .await;

                    latest_name
                }
            };

            // 카테고리 생성
            let category_name = format!("{}(PM: {})", project_name, pm_name);
            let category_builder = CreateChannel::new(&category_name)
                .kind(ChannelType::Category)
                .permissions(vec![
                    deny_everyone.clone(),
                    allow_project_role.clone(),
                    allow_bot.clone(),
                ]);

            match guild_id.create_channel(&ctx.http, category_builder).await {
                Ok(category) => {
                    let text_channels = vec![
                        "📜information",
                        "🤖bot",
                        "🌐dev",
                        "🚩issue",
                        "✅progress",
                        "📢notice",
                        "🎡random",
                        "🖥️github",
                    ];

                    for ch_name in text_channels {
                        let mut builder = CreateChannel::new(ch_name)
                            .kind(ChannelType::Text)
                            .category(category.id);

                        if ch_name == "🖥️github" {
                            builder = builder.permissions(vec![
                                deny_everyone.clone(),
                                readonly_project_role.clone(),
                                allow_bot.clone(),
                            ]);
                        } else if ch_name == "📜information" {
                            builder = builder.permissions(vec![
                                deny_everyone.clone(),
                                readonly_project_role.clone(),
                                allow_pm.clone(),
                                allow_bot.clone(),
                            ]);
                        }

                        let _ = guild_id.create_channel(&ctx.http, builder).await;
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    }

                    let voice_builder = CreateChannel::new("🎙️ voice chat")
                        .kind(ChannelType::Voice)
                        .category(category.id);
                    let _ = guild_id.create_channel(&ctx.http, voice_builder).await;

                    command
                        .edit_response(
                            &ctx.http,
                            EditInteractionResponse::new().content(format!(
                                "🚀 <@{}> 님, 프로젝트 서버 세팅이 완료되었습니다!",
                                command.user.id
                            )),
                        )
                        .await?;
                }
                Err(why) => {
                    command
                        .edit_response(
                            &ctx.http,
                            EditInteractionResponse::new()
                                .content(format!("❌ 카테고리 생성 실패: {:?}", why)),
                        )
                        .await?;
                }
            }
            let _ = tx.send(CacheCommand::RefreshAll).await;
        }

        // --- 2. 프로젝트 이름 변경 (/project rename [new_name]) ---
        "rename" => {
            let mut new_name = String::new();
            if let CommandDataOptionValue::SubCommand(sub_opts) = &subcommand_option.value {
                if let Some(opt) = sub_opts.iter().find(|o| o.name == "new_name") {
                    if let CommandDataOptionValue::String(val) = &opt.value {
                        new_name = val.clone();
                    }
                }
            }

            if new_name.is_empty() {
                command
                    .edit_response(
                        &ctx.http,
                        EditInteractionResponse::new().content("⚠️ 변경할 새 이름을 입력해주세요."),
                    )
                    .await?;
                return Ok(());
            }

            if let Channel::Guild(channel) = command.channel_id.to_channel(&ctx.http).await? {
                if let Some(category_id) = channel.parent_id {
                    let exists = cache.project_mapping.keys().any(|k| k == &new_name);
                    if exists {
                        command
                            .edit_response(
                                &ctx.http,
                                EditInteractionResponse::new().content(format!(
                                    "❌ 이미 '{}' 이름의 프로젝트가 존재합니다.",
                                    new_name
                                )),
                            )
                            .await?;
                        return Ok(());
                    }

                    let mut old_name = String::new();
                    if let Ok(Channel::Guild(cat_channel)) = category_id.to_channel(&ctx.http).await
                    {
                        let category_name: Vec<&str> = cat_channel.name.split("(PM: ").collect();
                        old_name = category_name[0].trim().to_string();
                    }

                    let pm_name = match cache.project_pms.get(&old_name) {
                        Some(pm_id) => cache
                            .all_members
                            .get(pm_id)
                            .cloned()
                            .unwrap_or_else(|| "Unknown PM".to_string()),
                        None => "Unknown PM".to_string(),
                    };
                    let category_name = format!("{}(PM: {})", new_name, pm_name);
                    let builder = EditChannel::new().name(&category_name);

                    match category_id.edit(&ctx.http, builder).await {
                        Ok(_) => {
                            let mut role_renamed = false;
                            if let Ok(roles) = guild_id.roles(&ctx.http).await {
                                for role in roles.values() {
                                    if !old_name.is_empty()
                                        && role.name.to_lowercase() == old_name.to_lowercase()
                                    {
                                        let role_builder = EditRole::new().name(&new_name);
                                        if guild_id
                                            .edit_role(&ctx.http, role.id, role_builder)
                                            .await
                                            .is_ok()
                                        {
                                            role_renamed = true;
                                        }
                                        break;
                                    }
                                }
                            }

                            let msg_content = if role_renamed {
                                format!("📝 프로젝트 카테고리와 역할 이름이 모두 '{}'으로 변경되었습니다.", new_name)
                            } else {
                                format!("📝 프로젝트 이름은 '{}'으로 변경되었으나, 동명의 기존 역할을 찾지 못했습니다.", new_name)
                            };
                            command
                                .edit_response(
                                    &ctx.http,
                                    EditInteractionResponse::new().content(msg_content),
                                )
                                .await?;
                            let _ = tx.send(CacheCommand::RefreshAll).await;
                        }
                        Err(why) => {
                            command
                                .edit_response(
                                    &ctx.http,
                                    EditInteractionResponse::new()
                                        .content(format!("❌ 이름 변경 실패: {:?}", why)),
                                )
                                .await?;
                        }
                    }
                } else {
                    command
                        .edit_response(
                            &ctx.http,
                            EditInteractionResponse::new().content(
                                "❌ 이 명령어는 프로젝트 카테고리 내부 채널에서 실행해야 합니다.",
                            ),
                        )
                        .await?;
                }
            }
        }

        // --- 3. 프로젝트 완전 삭제 (/project delete) ---
        "delete" => {
            let guild = guild_id.to_partial_guild(&ctx.http).await?;
            let member = match &command.member {
                Some(m) => m,
                None => return Ok(()),
            };

            // 관리자 권한 확인
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

            if !is_admin {
                command
                    .edit_response(
                        &ctx.http,
                        EditInteractionResponse::new().content(
                            "❌ 이 명령어는 서버 관리자(ADMINISTRATOR) 권한이 필요합니다.",
                        ),
                    )
                    .await?;
                return Ok(());
            }

            if let Channel::Guild(channel) = command.channel_id.to_channel(&ctx.http).await? {
                if let Some(category_id) = channel.parent_id {
                    let mut category_name = String::new();
                    if let Ok(Channel::Guild(cat_channel)) = category_id.to_channel(&ctx.http).await
                    {
                        category_name = cat_channel.name.clone();

                        // 프로젝트 내부인지 그냥 카테고리 내부인지 확인
                        let project_name = category_name
                            .split("(PM:")
                            .next()
                            .unwrap_or("")
                            .trim()
                            .to_string();

                        // 프로젝트 매핑에 존재하지 않는다면, 그냥 카테고리 내부에서 명령어를 친 것임
                        if cache.project_mapping.contains_key(&project_name) == false {
                            command
                                .edit_response(
                                    &ctx.http,
                                    EditInteractionResponse::new().content(
                                        "❌ 프로젝트 카테고리 내부 채널에서 명령어를 입력해주세요.",
                                    ),
                                )
                                .await?;
                            return Ok(());
                        }
                    }

                    command
                        .edit_response(
                            &ctx.http,
                            EditInteractionResponse::new()
                                .content("🧹 프로젝트 채널들과 역할을 완전히 삭제합니다..."),
                        )
                        .await?;

                    // 1. 하위 채널 청소 (현재 명령어가 쳐진 채널 제외하고 '확실히' 대기하며 삭제)
                    if let Ok(channels) = guild_id.channels(&ctx.http).await {
                        for (id, guild_channel) in &channels {
                            if guild_channel.parent_id == Some(category_id)
                                && *id != command.channel_id
                            {
                                // ⚠️ 에러를 씹지 않고(?), 완전히 삭제가 끝날 때까지 동기적으로 기다립니다.
                                if let Err(why) = guild_channel.id.delete(&ctx.http).await {
                                    tracing::error!("하위 채널 삭제 실패: {:?}", why);
                                }
                                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                            }
                        }

                        // 2. 명령어가 실행된 현재 채널 삭제
                        if let Err(why) = command.channel_id.delete(&ctx.http).await {
                            tracing::error!("현재 명령어 채널 삭제 실패: {:?}", why);
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    }

                    // 3. 카테고리 자체 삭제 대기
                    if let Err(why) = category_id.delete(&ctx.http).await {
                        tracing::error!("카테고리 채널 삭제 실패: {:?}", why);
                    }

                    // 4. 동명 프로젝트 역할 찾아서 삭제 대기
                    let project_name = category_name
                        .split("(PM:")
                        .next()
                        .unwrap_or("")
                        .trim()
                        .to_string();

                    if let Ok(roles) = guild_id.roles(&ctx.http).await {
                        for role in roles.values() {
                            if !project_name.is_empty()
                                && role.name.to_lowercase() == project_name.to_lowercase()
                            {
                                if let Err(why) = guild_id.delete_role(&ctx.http, role.id).await {
                                    tracing::error!("프로젝트 역할 삭제 실패: {:?}", why);
                                }
                                break;
                            }
                        }
                    }

                    // ⭐ [핵심 안정화 장치]
                    // 디스코드 API 서버 측에서 삭제 처리가 완료되고, 게이트웨이 백엔드에 반영되는 최소한의 물리적 시간을 벌어줍니다.
                    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

                    // 5. 완벽히 비워진 상태에서 캐시 전체 갱신 요청
                    let _ = tx.send(CacheCommand::RefreshAll).await;
                } else {
                    command.edit_response(&ctx.http, EditInteractionResponse::new().content("❌ 삭제할 프로젝트 카테고리 내부의 채널에서 명령어를 입력해주세요.")).await?;
                }
            }
        }
        _ => {}
    }

    Ok(())
}
