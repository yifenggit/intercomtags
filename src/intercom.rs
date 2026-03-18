use crate::config::IntercomRegion;
use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const API_VERSION: &str = "2.14";

#[derive(Debug, Clone)]
pub struct IntercomClient {
    client: Client,
    token: String,
    retries: u32,
    api_base: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Tag {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub count: Option<i64>,
}

// Intercom API 2.14 返回格式: { "type": "list", "data": [...] }
#[derive(Debug, Serialize, Deserialize)]
struct TagsResponse {
    #[serde(rename = "type", default)]
    _type: Option<String>,
    #[serde(default)]
    data: Vec<Tag>,
    // 兼容旧格式
    #[serde(default)]
    tags: Vec<Tag>,
}

impl TagsResponse {
    fn get_tags(self) -> Vec<Tag> {
        if !self.data.is_empty() {
            self.data
        } else {
            self.tags
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct CreateTagRequest {
    name: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct TagContactRequest {
    id: String,
}

// 错误响应格式
#[derive(Debug, Serialize, Deserialize)]
struct ErrorList {
    #[serde(rename = "type")]
    _type: String,
    errors: Vec<ErrorDetail>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ErrorDetail {
    code: String,
    message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Contact {
    pub id: String,
    pub email: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SearchRequest {
    query: SearchQuery,
}

#[derive(Debug, Serialize, Deserialize)]
struct SearchQuery {
    field: String,
    operator: String,
    value: String,
}

// Intercom API 返回格式: { "type": "list", "data": [...] }
#[derive(Debug, Serialize, Deserialize)]
struct SearchResponse {
    #[serde(default)]
    data: Vec<Contact>,
    // 兼容字段
    #[serde(default)]
    contacts: Vec<Contact>,
    #[serde(default)]
    total_count: i64,
}

impl SearchResponse {
    fn get_contacts(self) -> Vec<Contact> {
        if !self.data.is_empty() {
            self.data
        } else {
            self.contacts
        }
    }
}

#[derive(Debug, Clone)]
pub struct TagResult {
    #[allow(dead_code)]
    pub email: String,
    pub success: bool,
    #[allow(dead_code)]
    pub message: String,
}

impl IntercomClient {
    pub fn new(token: String, retries: u32, region: &IntercomRegion) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .pool_max_idle_per_host(20)
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            token,
            retries,
            api_base: region.api_base().to_string(),
        }
    }

    /// 获取或创建标签
    pub async fn get_or_create_tag(&self, tag_name: &str) -> Result<Tag> {
        // 先尝试获取已存在的标签
        if let Some(tag) = self.get_tag(tag_name).await? {
            return Ok(tag);
        }

        // 标签不存在，创建新标签
        self.create_tag(tag_name).await
    }

    async fn get_tag(&self, tag_name: &str) -> Result<Option<Tag>> {
        let url = format!("{}/tags", self.api_base);
        
        let response = self.do_request_with_retry(|| {
            Ok(self.client
                .get(&url)
                .header("Authorization", format!("Bearer {}", self.token))
                .header("Accept", "application/json")
                .header("Intercom-Version", API_VERSION)
                .build()?)
        }).await?;

        // 获取状态码和响应文本
        let status = response.status();
        let body_text = response.text().await
            .context("读取响应内容失败")?;
        
        // 检查 HTTP 状态
        if !status.is_success() {
            anyhow::bail!("API 返回错误: HTTP {} - {}", status, body_text);
        }
        
        // 尝试解析 JSON
        let tags_response: TagsResponse = match serde_json::from_str(&body_text) {
            Ok(resp) => resp,
            Err(e) => {
                // 记录原始响应以便调试
                log::error!("解析标签列表失败. 原始响应: {}", &body_text[..body_text.len().min(500)]);
                anyhow::bail!("解析标签列表失败: {}. 响应预览: {}", e, &body_text[..body_text.len().min(100)])
            }
        };

        Ok(tags_response
            .get_tags()
            .into_iter()
            .find(|t| t.name == tag_name))
    }

    async fn create_tag(&self, tag_name: &str) -> Result<Tag> {
        let url = format!("{}/tags", self.api_base);
        
        let request_body = CreateTagRequest {
            name: tag_name.to_string(),
        };

        let response = self.do_request_with_retry(|| {
            Ok(self.client
                .post(&url)
                .header("Authorization", format!("Bearer {}", self.token))
                .header("Accept", "application/json")
                .header("Intercom-Version", API_VERSION)
                .json(&request_body)
                .build()?)
        }).await?;

        let tag: Tag = response
            .json()
            .await
            .context("解析创建标签响应失败")?;

        Ok(tag)
    }

    /// 搜索联系人
    pub async fn search_contact(&self, email: &str) -> Result<Option<Contact>> {
        let url = format!("{}/contacts/search", self.api_base);
        
        let request_body = SearchRequest {
            query: SearchQuery {
                field: "email".to_string(),
                operator: "=".to_string(),
                value: email.to_string(),
            },
        };

        let response = self.do_request_with_retry(|| {
            Ok(self.client
                .post(&url)
                .header("Authorization", format!("Bearer {}", self.token))
                .header("Accept", "application/json")
                .header("Intercom-Version", API_VERSION)
                .json(&request_body)
                .build()?)
        }).await?;

        let search_response: SearchResponse = response
            .json()
            .await
            .context("解析搜索响应失败")?;

        Ok(search_response.get_contacts().into_iter().next())
    }

    /// 单条给联系人打标签 (API 2.14)
    /// POST /contacts/{contact_id}/tags
    pub async fn tag_contact_single(&self, contact_id: &str, tag_id: &str) -> Result<TagResult> {
        let url = format!("{}/contacts/{}/tags", self.api_base, contact_id);
        
        let request_body = TagContactRequest {
            id: tag_id.to_string(),
        };

        let response = self.do_request_with_retry(|| {
            Ok(self.client
                .post(&url)
                .header("Authorization", format!("Bearer {}", self.token))
                .header("Accept", "application/json")
                .header("Content-Type", "application/json")
                .header("Intercom-Version", API_VERSION)
                .json(&request_body)
                .build()?)
        }).await?;

        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();

        if status.is_success() {
            Ok(TagResult {
                email: contact_id.to_string(),
                success: true,
                message: "打标签成功".to_string(),
            })
        } else {
            // 解析错误响应
            let error_msg = if let Ok(error_list) = serde_json::from_str::<ErrorList>(&body_text) {
                if let Some(first_error) = error_list.errors.first() {
                    format!("{}: {}", first_error.code, first_error.message)
                } else {
                    format!("HTTP {}", status)
                }
            } else {
                format!("HTTP {} - {}", status, &body_text[..body_text.len().min(100)])
            };
            
            Ok(TagResult {
                email: contact_id.to_string(),
                success: false,
                message: error_msg,
            })
        }
    }

    /// 带重试的请求执行（支持限流保护）
    async fn do_request_with_retry<F>(&self, build_request: F) -> Result<reqwest::Response>
    where
        F: Fn() -> Result<reqwest::Request>,
    {
        let mut last_error = None;

        for attempt in 0..self.retries {
            let request = build_request()?;
            
            match self.client.execute(request).await {
                Ok(response) => {
                    let status = response.status();
                    
                    // 401 认证错误 - 直接返回，不重试
                    if status == 401 {
                        let body_text = response.text().await.unwrap_or_default();
                        let error_msg = if let Ok(error_list) = serde_json::from_str::<ErrorList>(&body_text) {
                            if let Some(first_error) = error_list.errors.first() {
                                format!("{}: {}", first_error.code, first_error.message)
                            } else {
                                "Access Token Invalid".to_string()
                            }
                        } else {
                            format!("认证失败: {}", status)
                        };
                        return Err(anyhow::anyhow!("API Token 无效 - {}", error_msg));
                    }
                    
                    // 429 限流错误 - 需要更长的退避
                    if status == 429 {
                        let retry_after = response.headers()
                            .get("retry-after")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|v| v.parse::<u64>().ok())
                            .unwrap_or(5);
                        
                        last_error = Some(anyhow::anyhow!("触发限流 (429)，请降低并发数"));
                        log::warn!("触发限流，Retry-After: {}秒", retry_after);
                        
                        if attempt < self.retries - 1 {
                            // 等待 Retry-After 时间 + 指数退避
                            let backoff = Duration::from_secs(retry_after + 2_u64.pow(attempt as u32));
                            tokio::time::sleep(backoff).await;
                            continue;
                        }
                    }
                    
                    // 5xx 服务器错误需要重试
                    if status.is_server_error() {
                        last_error = Some(anyhow::anyhow!("服务器错误: {}", status));
                        if attempt < self.retries - 1 {
                            // 指数退避: 2^attempt 秒
                            let backoff = Duration::from_secs(2_u64.pow(attempt as u32));
                            tokio::time::sleep(backoff).await;
                            continue;
                        }
                    }
                    
                    return Ok(response);
                }
                Err(e) => {
                    last_error = Some(anyhow::anyhow!("请求失败: {}", e));
                    if attempt < self.retries - 1 {
                        // 指数退避: 2^attempt 秒
                        let backoff = Duration::from_secs(2_u64.pow(attempt as u32));
                        tokio::time::sleep(backoff).await;
                        continue;
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("重试失败")))
    }
}
