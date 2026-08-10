use serenity::all::{
    CommandInteraction, CommandOptionType, CreateCommand, CreateEmbed, CreateEmbedFooter, GuildId,
};
use serenity::builder::EditInteractionResponse;
use serenity::prelude::*;

use std::fmt::Write;

pub fn register_help_command() -> CreateCommand {
    CreateCommand::new("help")
        .description("봇의 모든 명령어와 사용법을 안내합니다.")
        .dm_permission(false)
}

pub async fn run_help_command(
    ctx: &Context,
    command: &CommandInteraction,
    guild_id: GuildId,
) -> serenity::Result<()> {
    command.defer(&ctx.http).await?;

    let commands = match guild_id.get_commands(&ctx.http).await {
        Ok(cmds) => cmds,
        Err(why) => {
            tracing::error!("명령어 가져오기 실패: {}", why);
            command
                .edit_response(
                    &ctx.http,
                    EditInteractionResponse::new()
                        .content("❌ 명령어 목록을 불러오는 중 오류가 발생했습니다."),
                )
                .await?;
            return Ok(());
        }
    };

    // 임베드 객체 생성 및 기본 스타일 정의
    let mut embed = CreateEmbed::new()
        .title("🤖 봇 명령어 도움말 (Help Menu)")
        .description("이 서버에서 사용할 수 있는 전체 슬래시 명령어 목록입니다.\n각 명령어를 클릭하면 즉시 입력 창이 활성화됩니다.\n")
        .color(0x5865F2) // 디스코드 공식 Blurple 색상 코드
        .footer(CreateEmbedFooter::new("💡 필요에 따라 하위 명령어를 선택해 사용하세요."));

    if commands.is_empty() {
        embed = embed.field("⚠️ 안내", "현재 등록된 슬래시 명령어가 없습니다.", false);
    } else {
        for cmd in commands {
            // 명령어 이름, 설명, 하위 명령어 목록을 임베드 필드로 추가
            let mut field_value = String::new();
            write!(field_value, "{}\n", cmd.description).unwrap(); // field_value.push_str(&format!("{}\n", cmd.description));

            // 하위 명령어가 있는 경우, 하위 명령어 목록을 추가
            let mut subcommands = Vec::new();
            for option in &cmd.options {
                if option.kind == CommandOptionType::SubCommand {
                    // 하위 명령어 이름, 설명을 포맷팅하여 목록에 추가
                    // 벡터에 추가하므로 format!을 사용하여 문자열 생성
                    subcommands.push(format!(
                        "`/{} {}` : {}",
                        cmd.name, option.name, option.description
                    ));
                }
            }

            if !subcommands.is_empty() {
                field_value.push_str("\n**세부 명령어:**\n");
                for sub in subcommands {
                    write!(field_value, "└ {}\n", sub).unwrap();
                }
            }

            let field_title = format!("/{}", cmd.name.to_uppercase());
            embed = embed.field(field_title, field_value, false);
        }
    }

    command
        .edit_response(&ctx.http, EditInteractionResponse::new().embed(embed))
        .await?;

    Ok(())
}
