use dotenv::dotenv;

pub fn get_notion_token() -> String {
    dotenv().ok();
    std::env::var("NOTION_TOKEN").expect("NOTION_TOKEN 환경 변수가 설정되어 있지 않습니다.")
}

pub fn get_notion_version() -> String {
    dotenv().ok();
    std::env::var("NOTION_VERSION").expect("NOTION_VERSION 환경 변수가 설정되어 있지 않습니다.")
}

pub fn get_notion_member_database_id() -> String {
    dotenv().ok();
    std::env::var("NOTION_MEMBER_DATABASE_ID")
        .expect("NOTION_MEMBER_DATABASE_ID 환경 변수가 설정되어 있지 않습니다.")
}

pub fn get_notion_project_database_id() -> String {
    dotenv().ok();
    std::env::var("NOTION_PROJECT_DATABASE_ID")
        .expect("NOTION_PROJECT_DATABASE_ID 환경 변수가 설정되어 있지 않습니다.")
}
