use std::collections::HashSet;

use serenity::framework::standard::macros::command;
use serenity::framework::standard::{Args, CommandResult};
use serenity::model::{prelude::*};
use serenity::prelude::*;

use crate::cache::CacheNotifyKey;

// 앞으로 해야할거
// 추후 개발방향: 노션에 연동해서 프로젝트 참여 인원 확인 하기

#[command]
async fn member(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    //let user = &msg.author;

    //명령어가 입력된 채널 정보 가져오기
    let channel = msg.channel_id.to_channel(&ctx.http).await?.guild().unwrap();
    
    //명령어가 카테고리에 속해있는 채널에서 입력된건지 확인
    let project_name:String;
    match channel.parent_id {
        Some(category_id) => {
            let category_channel = category_id.to_channel(&ctx.http).await?.guild().unwrap();
            project_name = category_channel.name.clone();
        }
        None => {
            msg.reply(ctx, "❌ 이 명령어는 프로젝트 내에서만 사용 가능합니다").await?;
            return Ok(());
        },
    };

    // 캐쉬 가져오기
    let data_read = ctx.data.read().await;
    let cache_lock = data_read
        .get::<crate::cache::SharedCacheKey>()
        .expect("보관함에 캐시가 없습니다.")
        .clone();
    let cache = cache_lock.read().await;
    
    //인수 파싱
    let subcommand = match args.single::<String>() {
        Ok(cmd) => cmd,
        Err(_) => { //만약 뒤에 아무런 커맨드가 없다면 => 사용법 출력
            // 보관된 해쉬맵에서 바로 꺼내쓰기
            let included_set = cache.project_mapping.get(&project_name);

            //인원 출력전 분류 vector
            let mut included_mems = Vec::new(); //참여 인원

            //전체 맴버 순회하면서 캐쉬랑 맞춰보고 포함인원만 추출
            for (user_id, username) in &cache.all_members {
                //included_set이 비지 않은 경우와 빈 경우 나눠서 생각
                if let Some(set) = included_set {
                    if set.contains(user_id) {
                        included_mems.push(username.clone());
                    }
                }
            }

            // 결과 출력
            let mut content = String::from("사용법: `/member <add | remove> [@유저들]`\n\n");
            content.push_str("`참여 인원`\n");
            for mem in included_mems { content.push_str(&format!("{}\n", mem)); }

            msg.reply(ctx, content).await?;
            return Ok(());
        }
    };
    
    // 하위 커맨드 구현
    match subcommand.as_str() {
        "add" => {
            //서버 아이디 획득
            let guild_id = match msg.guild_id {
                Some(id) => id,
                None => return Ok(()),
            };

            // 역할 id 찾기
            let mut target_role_id = None;
            if let Ok(roles) = guild_id.roles(&ctx.http).await {
                if let Some(role) = roles.values().find(|r| r.name == project_name) {
                    target_role_id = Some(role.id);
                }
            }
            let role_id = match target_role_id {
                Some(id) => id,
                None => {
                    msg.reply(ctx, format!("❌ '{}' 이름의 프로젝트 역할을 찾을 수 없습니다.", project_name)).await?;
                    return Ok(());
                }
            };

            // 캐쉬에서 이미 참가중인 멤버 Id목록 가져오기
            let mut already_members = HashSet::new();
            if let Some(members) = cache.project_mapping.get(&project_name) {
                already_members = members.clone();
            }

            // 그 뒤에 나오는 인자들을 전부 파싱 후 벡터에 저장
            let mut added_users = Vec::new();
            for user_str_res in args.iter::<String>() {
                match user_str_res {
                    Ok(user_str) => {
                        let clean_str = user_str
                            .trim_start_matches("<@")
                            .trim_start_matches('!')
                            .trim_end_matches('>');

                        if let Ok(id_num) = clean_str.parse::<u64>() {
                            let user_id = UserId::new(id_num);

                            // 이미 들어가 있다면 생략
                            if already_members.contains(&user_id) {
                                continue;
                            }

                            match ctx.http.add_member_role(guild_id, user_id, role_id, None).await {
                                Ok(_) => { added_users.push(user_id); }
                                Err(why) => { println!("❌ [디스코드 API 에러] 역할 부여 실패: {:?}", why); }
                            }
                        }
                        else {
                            println!("⚠️ 숫자로 변환할 수 없는 올바르지 않은 유저 형식: {}", user_str);
                        }
                    }
                    Err(_) => {
                        println!("올바르지 않은 유저 형식은 건너뜁니다");
                    }
                }
            }

            if added_users.is_empty() {
                msg.reply(ctx, "❌ 올바른 유저 형식이 아니거나 불러올 수 있는 유저가 없습니다").await?;
            }
            else {
                let mentions: Vec<String> = added_users.iter().map(|id| format!("<@{}>", id)).collect();
                msg.reply(ctx, format!("✅ {} 님이 '{}' 프로젝트에 추가되었습니다!", mentions.join(", "), project_name)).await?;

                // 캐시 스레드 깨우기
                if let Some(tx) = ctx.data.read().await.get::<CacheNotifyKey>() {
                    let _ = tx.send(()).await;
                }
            }
        },
        "remove" => {
            //서버 아이디 획득
            let guild_id = match msg.guild_id {
                Some(id) => id,
                None => return Ok(()),
            };

            // 역할 id 찾기
            let mut target_role_id = None;
            if let Ok(roles) = guild_id.roles(&ctx.http).await {
                if let Some(role) = roles.values().find(|r| r.name == project_name) {
                    target_role_id = Some(role.id);
                }
            }
            let role_id = match target_role_id {
                Some(id) => id,
                None => {
                    msg.reply(ctx, format!("❌ '{}' 이름의 프로젝트 역할을 찾을 수 없습니다.", project_name)).await?;
                    return Ok(());
                }
            };

            // 캐쉬에서 이미 참가중인 멤버 Id목록 가져오기
            let mut already_members = HashSet::new();
            if let Some(members) = cache.project_mapping.get(&project_name) {
                already_members = members.clone();
            }

            // 그 뒤에 나오는 인자들을 전부 파싱 후 벡터에 저장
            let mut removed_users = Vec::new();
            for user_str_res in args.iter::<String>() {
                match user_str_res {
                    Ok(user_str) => {
                        let clean_str = user_str
                            .trim_start_matches("<@")
                            .trim_start_matches('!')
                            .trim_end_matches('>');

                        if let Ok(id_num) = clean_str.parse::<u64>() {
                            let user_id = UserId::new(id_num);

                            // 프로젝트에 없는 사람은 건너뛰기
                            if !already_members.contains(&user_id) {
                                println!("이 유저는 원래 프로젝트 멤버가 아닙니다. 건너뜁니다.");
                                continue;
                            }

                            match ctx.http.remove_member_role(guild_id, user_id, role_id, None).await {
                                Ok(_) => { removed_users.push(user_id); }
                                Err(why) => { println!("❌ [디스코드 API 에러] 역할 제거 실패: {:?}", why); }
                            }
                        }
                        else {
                            println!("⚠️ 숫자로 변환할 수 없는 올바르지 않은 유저 형식: {}", user_str);
                        }
                    }
                    Err(_) => {
                        println!("올바르지 않은 유저 형식은 건너뜁니다");
                    }
                }
            }

            if removed_users.is_empty() {
                msg.reply(ctx, "❌ 올바른 유저 형식이 아니거나 내보낼 수 있는 유저가 없습니다").await?;
            }
            else {
                let mentions: Vec<String> = removed_users.iter().map(|id| format!("<@{}>", id)).collect();
                msg.reply(ctx, format!("❌ {} 님이 '{}' 프로젝트에서 내보내졌습니다", mentions.join(", "), project_name)).await?;

                // 캐시 스레드 깨우기
                if let Some(tx) = ctx.data.read().await.get::<CacheNotifyKey>() {
                    let _ = tx.send(()).await;
                }
            }
        },
        _ => { //이외의 하위 명령어 입력 시
            msg.reply(ctx, "❌ 알 수 없는 하위 명령어입니다. (사용 가능: add, remove)").await?;
        }
    };
    
    Ok(())
}