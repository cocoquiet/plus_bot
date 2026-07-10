use serenity::framework::standard::macros::command;
use serenity::framework::standard::{Args, CommandResult};
use serenity::model::prelude::*;
use serenity::prelude::*;
use serenity::builder::{CreateChannel, EditChannel, EditRole};
use serenity::model::channel::{PermissionOverwrite, PermissionOverwriteType};
use serenity::model::id::RoleId;

#[command]
async fn project(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let subcommand = match args.single::<String>() {
        Ok(cmd) => cmd,
        Err(_) => {
            msg.reply(ctx, "❌ 사용법: `/project <generate | rename | delete> [프로젝트명]`").await?;
            return Ok(());
        }
    };

    match subcommand.as_str() {
        // --- 1. 프로젝트 생성 (역할 생성 + 비공개 카테고리 + 봇 예외 권한 + 채널 일괄 생성) ---
        "generate" => {
            let project_name = match args.single::<String>() {
                Ok(name) => name,
                Err(_) => {
                    msg.reply(ctx, "⚠️ 생성할 프로젝트 이름을 입력해주세요.\n사용법: `/project generate 프로젝트A`").await?;
                    return Ok(());
                }
            };

            if let Some(guild_id) = msg.guild_id {
                // 🔍 [개선 2] 동명의 프로젝트(역할)가 이미 존재하는지 실시간 API로 확인
                if let Ok(roles) = guild_id.roles(&ctx.http).await {
                    let exists = roles.values().any(|role| role.name == project_name);
                    if exists {
                        msg.reply(ctx, format!("❌ 이미 '{}' 이름의 프로젝트가 존재합니다. 다른 이름을 사용해주세요.", project_name)).await?;
                        return Ok(());
                    }
                }

                msg.reply(ctx, format!("🏗️ '{}' 프로젝트 생성을 시작합니다. 역할 및 채널을 세팅 중...", project_name)).await?;

                // 🤖 봇 자신의 ID를 API로부터 안전하게 가져옵니다.
                let bot_id = match ctx.http.get_current_user().await {
                    Ok(user) => user.id,
                    Err(why) => {
                        msg.reply(ctx, format!("❌ 봇 정보를 가져오지 못했습니다: {:?}", why)).await?;
                        return Ok(());
                    }
                };

                let role_builder = EditRole::new().name(&project_name);
                let project_role = match guild_id.create_role(&ctx.http, role_builder).await {
                    Ok(role) => role,
                    Err(why) => {
                        msg.reply(ctx, format!("❌ 역할 생성 실패: {:?}", why)).await?;
                        return Ok(());
                    }
                };

                match guild_id.member(&ctx.http, msg.author.id).await {
                    Ok(member) => {
                        if let Err(why) = member.add_role(&ctx.http, project_role.id).await {
                            println!("⚠️ [generate] 유저에게 역할 부여 실패: {:?}", why);
                        } else {
                            println!("✅ [generate] 유저에게 역할(ID: {}) 부여 완료", project_role.id);
                        }
                    },
                    Err(why) => {
                        println!("⚠️ [generate] 멤버 정보를 서버에서 가져오지 못함: {:?}", why);
                    }
                }

                let everyone_role_id = RoleId::new(guild_id.get());
                
                // 1) @everyone은 채널을 보지 못하게 차단
                let deny_everyone = PermissionOverwrite {
                    allow: Permissions::empty(),
                    deny: Permissions::VIEW_CHANNEL,
                    kind: PermissionOverwriteType::Role(everyone_role_id),
                };

                // 2) 프로젝트 전용 역할 소지자는 보기 및 메시지 전송 허용
                let allow_project_role = PermissionOverwrite {
                    allow: Permissions::VIEW_CHANNEL | Permissions::SEND_MESSAGES,
                    deny: Permissions::empty(),
                    kind: PermissionOverwriteType::Role(project_role.id),
                };

                // 3) 🤖 봇 자신(Member)에게는 명시적으로 채널 보기, 메시지 보내기, 채널 관리 권한을 허용합니다.
                let allow_bot = PermissionOverwrite {
                    allow: Permissions::VIEW_CHANNEL | Permissions::SEND_MESSAGES | Permissions::MANAGE_CHANNELS,
                    deny: Permissions::empty(),
                    kind: PermissionOverwriteType::Member(bot_id),
                };

                // 카테고리 빌더에 세 가지 권한 규칙을 모두 주입
                let category_builder = CreateChannel::new(&project_name)
                    .kind(ChannelType::Category)
                    .permissions(vec![deny_everyone, allow_project_role, allow_bot]);

                match guild_id.create_channel(&ctx.http, category_builder).await {
                    Ok(category) => {
                        // 유저 커스텀 채널 목록 유지
                        let text_channels = vec![
                            "🤖bot",
                            "🌐dev",
                            "🚩issue",
                            "✅progress",
                            "📢notice",
                            "🎡random",
                            "🖥️github",
                        ];

                        for ch_name in text_channels {
                            let builder = CreateChannel::new(ch_name)
                                .kind(ChannelType::Text)
                                .category(category.id); // 유저 커스텀 메서드 유지
                            let _ = guild_id.create_channel(&ctx.http, builder).await;
                            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                        }

                        let voice_builder = CreateChannel::new("🎙️ voice chat")
                            .kind(ChannelType::Voice)
                            .category(category.id); // 유저 커스텀 메서드 유지
                        let _ = guild_id.create_channel(&ctx.http, voice_builder).await;

                        msg.channel_id.say(&ctx.http, format!("🚀 <@{}> 님, 비공개 프로젝트 서버 세팅이 완료되었습니다!\n(전용 역할이 부여되었습니다.)", msg.author.id)).await?;
                    },
                    Err(why) => {
                        msg.reply(ctx, format!("❌ 카테고리 생성 실패: {:?}", why)).await?;
                    }
                }
            }
        },

        // --- 2. 프로젝트 이름 변경 (카테고리 이름 + 역할 이름 동시 변경) ---
        "rename" => {
            let new_name = match args.single::<String>() {
                Ok(name) => name,
                Err(_) => {
                    msg.reply(ctx, "⚠️ 변경할 새 이름을 입력해주세요.\n사용법: `/project rename 새이름`").await?;
                    return Ok(());
                }
            };

            if let Some(guild_id) = msg.guild_id {
                if let Channel::Guild(channel) = msg.channel_id.to_channel(&ctx.http).await? {
                    if let Some(category_id) = channel.parent_id {
                        
                        let mut old_name = String::new();
                        if let Ok(Channel::Guild(cat_channel)) = category_id.to_channel(&ctx.http).await {
                            old_name = cat_channel.name.clone();
                        }

                        let builder = EditChannel::new().name(&new_name);

                        match category_id.edit(&ctx.http, builder).await {
                            Ok(_) => {
                                let mut role_renamed = false;
                                if let Ok(roles) = guild_id.roles(&ctx.http).await {
                                    for role in roles.values() {
                                        // 📝 [디버그] 서버에 존재하는 모든 역할 이름을 하나씩 출력해서 대조
                                        println!("   -> 서버의 역할 목록 탐색 중: '{}' (ID: {})", role.name, role.id);

                                        // 🔍 [수정] !old_name.is_empty() 체크 및 대소문자 구분 없는 비교로 변경
                                        if !old_name.is_empty() && role.name.to_lowercase() == old_name.to_lowercase() {
                                            println!("🎯 [디버그] 일치하는 역할을 찾았습니다! 이름을 변경합니다.");

                                            let role_builder = EditRole::new().name(&new_name);
                                            if let Err(why) = guild_id.edit_role(&ctx.http, role.id, role_builder).await {
                                                println!("⚠️ 역할 이름 변경 실패: {:?}", why);
                                            } else {
                                                role_renamed = true;
                                            }
                                            break;
                                        }
                                    }
                                }

                                if role_renamed {
                                    msg.reply(ctx, format!("📝 프로젝트 카테고리와 역할 이름이 모두 '{}'으로 변경되었습니다.", new_name)).await?;
                                } else {
                                    msg.reply(ctx, format!("📝 프로젝트 이름은 '{}'으로 변경되었으나, 동명의 기존 역할을 찾지 못했습니다.", new_name)).await?;
                                }
                            },
                            Err(why) => {
                                msg.reply(ctx, format!("❌ 이름 변경 실패: {:?}", why)).await?;
                            }
                        }
                    } else {
                        msg.reply(ctx, "❌ 이 명령어는 카테고리(프로젝트) 내부에 속한 채널에서 실행해야 합니다.").await?;
                    }
                }
            }
        },

        // --- 3. 프로젝트 완전 삭제 (하위 채널 + 카테고리 + 역할까지 일괄 청소) ---
        "delete" => {
            if let Some(guild_id) = msg.guild_id {
                let guild = guild_id.to_partial_guild(&ctx.http).await?;
                let member = guild_id.member(&ctx.http, msg.author.id).await?;
                
                let mut is_admin = guild.owner_id == msg.author.id;
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
                    msg.reply(ctx, "❌ 이 명령어는 서버 관리자(ADMINISTRATOR) 권한이 필요합니다.").await?;
                    return Ok(());
                }

                if let Channel::Guild(channel) = msg.channel_id.to_channel(&ctx.http).await? {
                    if let Some(category_id) = channel.parent_id {
                        let mut category_name = String::new();
                        if let Ok(Channel::Guild(cat_channel)) = category_id.to_channel(&ctx.http).await {
                            category_name = cat_channel.name.clone();
                        }

                        msg.reply(ctx, "🧹 프로젝트 채널들과 역할을 완전히 삭제합니다...").await?;

                        if let Ok(channels) = guild_id.channels(&ctx.http).await {
                            for (id, guild_channel) in &channels {
                                if guild_channel.parent_id == Some(category_id) && *id != msg.channel_id {
                                    let _ = guild_channel.id.delete(&ctx.http).await;
                                    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                                }
                            }
                            let _ = msg.channel_id.delete(&ctx.http).await;
                            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                        }

                        let _ = category_id.delete(&ctx.http).await;

                        // 🔍 [수정] 대소문자 구분 없는 비교로 변경하여 확실하게 삭제되도록 보완
                        if let Ok(roles) = guild_id.roles(&ctx.http).await {
                            for role in roles.values() {
                                if !category_name.is_empty() && role.name.to_lowercase() == category_name.to_lowercase() {
                                    if let Err(why) = guild_id.delete_role(&ctx.http, role.id).await {
                                        println!("⚠️ 역할 [{}] 삭제 실패: {:?}", role.name, why);
                                    } else {
                                        println!("✅ 역할 [{}] 삭제 성공", role.name);
                                    }
                                    break;
                                }
                            }
                        }
                    } else {
                        msg.reply(ctx, "❌ 삭제할 프로젝트 카테고리 내부의 채널에서 명령어를 입력해주세요.").await?;
                    }
                }
            }
        },

        _ => {
            msg.reply(ctx, "❌ 알 수 없는 하위 명령어입니다. (사용 가능: generate, rename, delete)").await?;
        }
    }

    Ok(())
}