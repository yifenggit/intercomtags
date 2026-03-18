use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum IntercomRegion {
    US,
    EU,
    AU,
}

impl Default for IntercomRegion {
    fn default() -> Self {
        IntercomRegion::AU
    }
}

impl IntercomRegion {
    pub fn api_base(&self) -> &'static str {
        match self {
            IntercomRegion::US => "https://api.intercom.io",
            IntercomRegion::EU => "https://api.eu.intercom.io",
            IntercomRegion::AU => "https://api.au.intercom.io",
        }
    }
    
    pub fn as_str(&self) -> &'static str {
        match self {
            IntercomRegion::US => "US",
            IntercomRegion::EU => "EU",
            IntercomRegion::AU => "AU",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub token: String,
    pub retries: u32,
    pub concurrency: u32,
    pub region: IntercomRegion,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            token: String::new(),
            retries: 3,
            concurrency: 3,
            region: IntercomRegion::default(),
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let config_path = Self::get_config_path();
        
        if config_path.exists() {
            match fs::read_to_string(&config_path) {
                Ok(content) => {
                    match serde_json::from_str(&content) {
                        Ok(config) => {
                            log::info!("配置加载成功: {:?}", config_path);
                            return config;
                        }
                        Err(e) => {
                            log::error!("配置解析失败: {}", e);
                        }
                    }
                }
                Err(e) => {
                    log::error!("读取配置文件失败: {}", e);
                }
            }
        }
        
        Self::default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let config_path = Self::get_config_path();
        
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        let content = serde_json::to_string_pretty(self)?;
        fs::write(&config_path, content)?;
        
        log::info!("配置保存成功: {:?}", config_path);
        Ok(())
    }

    fn get_config_path() -> PathBuf {
        let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        path.push("intercomtags");
        path.push("config.json");
        path
    }
}
