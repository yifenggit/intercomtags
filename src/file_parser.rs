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
    let mut email_col: Option<usize> = None;
    let mut tag_col: Option<usize> = None;
    let mut first_row = true;
    
    for result in reader.records() {
        let record = result.context("解析CSV记录失败")?;
        
        if record.is_empty() {
            continue;
        }

        // 解析标题行，识别邮箱列和标签列
        if first_row {
            first_row = false;
            for (i, field) in record.iter().enumerate() {
                let header = field.trim().to_lowercase();
                if is_email_header(&header) {
                    email_col = Some(i);
                } else if is_tag_header(&header) {
                    tag_col = Some(i);
                }
            }
            
            // 如果识别到邮箱列，跳过标题行继续处理
            if email_col.is_some() {
                continue;
            }
            // 否则默认第一列是邮箱，第二列是标签
            email_col = Some(0);
            tag_col = Some(1);
        }

        let email = record.get(email_col.unwrap_or(0))
            .unwrap_or("")
            .trim()
            .to_string();

        if !is_valid_email(&email) {
            continue;
        }

        let tag = tag_col.and_then(|col| {
            let t = record.get(col).map(|s| s.trim().to_string()).unwrap_or_default();
            if t.is_empty() { None } else { Some(t) }
        });
        
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
    let mut email_col: Option<usize> = None;
    let mut tag_col: Option<usize> = None;
    let mut first_row = true;

    for row in range.rows() {
        if row.is_empty() {
            continue;
        }

        // 解析标题行，识别邮箱列和标签列
        if first_row {
            first_row = false;
            for (i, cell) in row.iter().enumerate() {
                let header = match cell {
                    Data::String(s) => s.trim().to_lowercase(),
                    _ => String::new(),
                };
                if is_email_header(&header) {
                    email_col = Some(i);
                } else if is_tag_header(&header) {
                    tag_col = Some(i);
                }
            }
            
            // 如果识别到邮箱列，跳过标题行继续处理
            if email_col.is_some() {
                continue;
            }
            // 否则默认第一列是邮箱，第二列是标签
            email_col = Some(0);
            tag_col = Some(1);
        }

        let email = get_cell_value(row, email_col.unwrap_or(0));

        if !is_valid_email(&email) {
            continue;
        }

        let tag = tag_col.and_then(|col| {
            let t = get_cell_value(row, col);
            if t.is_empty() { None } else { Some(t) }
        });
        
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

fn is_email_header(header: &str) -> bool {
    let headers = [
        // 英文
        "email", "emails", "e-mail", "e_mail",
        "mail", "mailbox", "email_address", "emailaddress",
        // 中文
        "邮箱", "电子邮件", "电子邮箱", "邮件地址",
        "联系邮箱", "用户邮箱", "客户邮箱",
        // 常见变体
        "用户邮箱", "账号", "账号邮箱", "账户", "登录邮箱",
        "email地址", "邮箱地址",
    ];
    headers.iter().any(|h| header == *h)
}

fn is_tag_header(header: &str) -> bool {
    let headers = [
        // 英文
        "tag", "tags", "label", "labels",
        // 中文
        "标签", "标记", "分类",
        // 常见变体
        "客户标签", "用户标签", "备注标签", "tag标签",
    ];
    headers.iter().any(|h| header == *h)
}

fn is_valid_email(email: &str) -> bool {
    !email.is_empty() && email.contains('@')
}
