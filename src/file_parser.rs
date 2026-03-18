use anyhow::{Context, Result};
use calamine::{Reader, Xlsx, open_workbook, Data};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct EmailWithTag {
    pub email: String,
    pub tag: Option<String>,
}

pub fn parse_file(path: &Path) -> Result<Vec<EmailWithTag>> {
    let extension = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_lowercase());

    match extension.as_deref() {
        Some("csv") => parse_csv(path),
        Some("xlsx") | Some("xls") => parse_xlsx(path),
        _ => anyhow::bail!("不支持的文件格式，仅支持 CSV 和 XLSX"),
    }
}

fn parse_csv(path: &Path) -> Result<Vec<EmailWithTag>> {
    let content = std::fs::read_to_string(path)
        .context("无法读取CSV文件")?;
    
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .from_reader(content.as_bytes());

    let mut results = Vec::new();
    
    for result in reader.records() {
        let record = result.context("解析CSV记录失败")?;
        
        if record.is_empty() {
            continue;
        }

        let email = record.get(0).unwrap_or("").trim().to_string();
        
        if is_header_row(&email) {
            continue;
        }

        if !is_valid_email(&email) {
            continue;
        }

        let tag = record.get(1).map(|s| s.trim().to_string());
        
        results.push(EmailWithTag { email, tag });
    }

    Ok(results)
}

fn parse_xlsx(path: &Path) -> Result<Vec<EmailWithTag>> {
    let mut workbook: Xlsx<_> = open_workbook(path)
        .context("无法打开Excel文件")?;
    
    let sheet_name = workbook.sheet_names().first()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Excel文件没有工作表"))?;

    let range = workbook.worksheet_range(&sheet_name)
        .context("无法读取工作表")?;

    let mut results = Vec::new();

    for row in range.rows() {
        if row.is_empty() {
            continue;
        }

        let email = get_cell_value(row, 0);
        
        if is_header_row(&email) {
            continue;
        }

        if !is_valid_email(&email) {
            continue;
        }

        let tag = get_cell_value(row, 1);
        let tag = if tag.is_empty() { None } else { Some(tag) };
        
        results.push(EmailWithTag { email, tag });
    }

    Ok(results)
}

fn get_cell_value(row: &[Data], col: usize) -> String {
    row.get(col)
        .map(|cell| match cell {
            Data::String(s) => s.trim().to_string(),
            Data::Float(f) => f.to_string(),
            Data::Int(i) => i.to_string(),
            _ => String::new(),
        })
        .unwrap_or_default()
}

fn is_header_row(email: &str) -> bool {
    let lower = email.to_lowercase();
    let headers = [
        "email", "邮箱", "e-mail", "mail", "地址",
        "email address", "电子邮件", "email_address",
    ];
    
    headers.iter().any(|h| lower == *h)
}

fn is_valid_email(email: &str) -> bool {
    !email.is_empty() && email.contains('@')
}
