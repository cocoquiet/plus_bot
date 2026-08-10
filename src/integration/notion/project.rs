use futures::future::join_all;
use reqwest;
use serde_json::Value;

use crate::integration::notion::member::get_member_by_id;

use super::{env::*, member::Member};

#[derive(Debug, Clone)]
pub enum Status {
    NotStarted,  // 시작 전
    InProgress,  // 진행 중
    Maintenance, // 유지보유
    Completed,   // 완료
}

impl From<&str> for Status {
    fn from(value: &str) -> Self {
        match value {
            "시작 전" => Status::NotStarted,
            "진행 중" => Status::InProgress,
            "유지보유" => Status::Maintenance,
            "완료" => Status::Completed,
            _ => Status::NotStarted, // 기본값으로 NotStarted를 반환
        }
    }
}

#[derive(Debug, Clone)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub status: Status,
    pub github: String,
    pub pm: Member,
    pub participants: Vec<Member>,
}

impl Project {
    pub async fn from_value(value: &Value) -> Self {
        let properties = &value["properties"];

        Project {
            id: value["id"].as_str().unwrap_or_default().to_string(),
            name: properties["name"]["title"][0]["text"]["content"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            status: Status::from(
                properties["status"]["select"]["name"]
                    .as_str()
                    .unwrap_or_default(),
            ),
            github: properties["github"]["url"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            pm: get_member_by_id(
                properties["PM"]["people"][0]["id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            )
            .await
            .unwrap_or_default(),
            participants: join_all(
                properties["participants"]["people"]
                    .as_array()
                    .unwrap_or(&Vec::new())
                    .iter()
                    .map(|member| {
                        get_member_by_id(member["id"].as_str().unwrap_or_default().to_string())
                    }),
            )
            .await
            .into_iter()
            .map(|m| m.unwrap_or_default())
            .collect(),
        }
    }
}

pub async fn get_projects() -> Result<Vec<Project>, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    let database_response = client
        .get(&format!(
            "https://api.notion.com/v1/databases/{}",
            get_notion_project_database_id()
        ))
        .header("Authorization", format!("Bearer {}", get_notion_token()))
        .header("Notion-Version", get_notion_version())
        .send()
        .await?;

    let data_source_id = match database_response.status() {
        reqwest::StatusCode::OK => {
            let json: Value = database_response.json().await?;
            json["data_sources"][0]["id"]
                .as_str()
                .ok_or("Notion API 응답에서 데이터 소스 ID를 추출하는 데 실패했습니다.")?
                .to_string()
        }
        _ => {
            eprintln!(
                "Notion API 요청이 실패했습니다. Status: {}",
                database_response.status()
            );
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Notion API 요청 실패",
            )));
        }
    };

    let data_source_response = client
        .post(&format!(
            "https://api.notion.com/v1/data_sources/{}/query",
            data_source_id
        ))
        .header("Authorization", format!("Bearer {}", get_notion_token()))
        .header("Notion-Version", get_notion_version())
        .header("Content-Type", "application/json")
        .send()
        .await?;

    match data_source_response.status() {
        reqwest::StatusCode::OK => {
            let json: Value = data_source_response
                .json()
                .await
                .expect("Notion API 응답을 JSON으로 파싱하는 데 실패했습니다.");
            let projects: Vec<Project> = join_all(
                json["results"]
                    .as_array()
                    .expect("Notion API 응답에서 results 배열을 추출하는 데 실패했습니다.")
                    .iter()
                    .map(Project::from_value),
            )
            .await;

            Ok(projects)
        }
        _ => {
            eprintln!(
                "Notion API 요청이 실패했습니다. Status: {}",
                data_source_response.status()
            );
            Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Notion API 요청 실패",
            )))
        }
    }
}

pub async fn create_project(project: &Project) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    let response = client
        .post(&format!("https://api.notion.com/v1/pages",))
        .header("Authorization", format!("Bearer {}", get_notion_token()))
        .header("Notion-Version", get_notion_version())
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "parent": { "database_id": get_notion_project_database_id() },
            "template": {
                "type": "default"
            },
            "properties": {
                "name": { "title": [{ "text": { "content": project.name } }] },
                "status": { "select": { "name": match project.status {
                    Status::NotStarted => "시작 전",
                    Status::InProgress => "진행 중",
                    Status::Maintenance => "유지보유",
                    Status::Completed => "완료",
                } } },
                "github": { "url": project.github },
                "PM": { "people": [{ "id": project.pm.id }] },
                "participants": { "people": project.participants.iter().map(|p| serde_json::json!({ "id": p.id })).collect::<Vec<_>>() }
            }
        }))
        .send()
        .await?;

    if response.status().is_success() {
        Ok(())
    } else {
        eprintln!(
            "Notion API 요청이 실패했습니다. Status: {}",
            response.status()
        );
        Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Notion API 요청 실패",
        )))
    }
}
