mod app;
mod config;
mod file_parser;
mod intercom;

use app::IntercomTagsApp;
use eframe::egui;
use std::path::PathBuf;

fn setup_logging() -> Result<(), Box<dyn std::error::Error>> {
    let log_dir = get_log_dir();
    std::fs::create_dir_all(&log_dir)?;

    let log_file = log_dir.join(format!(
        "intercomtags_{}.log",
        chrono::Local::now().format("%Y%m%d_%H%M%S")
    ));

    let file_config = fern::Dispatch::new()
        .level(log::LevelFilter::Debug)
        .format(|out, message, record| {
            out.finish(format_args!(
                "[{}] [{}] [{}] {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
                record.level(),
                record.target(),
                message
            ))
        })
        .chain(fern::log_file(log_file)?);

    let console_config = fern::Dispatch::new()
        .level(log::LevelFilter::Info)
        .format(|out, message, record| {
            out.finish(format_args!(
                "[{}] [{}] {}",
                chrono::Local::now().format("%H:%M:%S"),
                record.level(),
                message
            ))
        })
        .chain(std::io::stdout());

    fern::Dispatch::new()
        .level(log::LevelFilter::Debug)
        .chain(file_config)
        .chain(console_config)
        .apply()?;

    log::info!("日志初始化成功，日志目录: {:?}", log_dir);
    Ok(())
}

fn get_log_dir() -> PathBuf {
    let mut path = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("intercomtags");
    path.push("logs");
    path
}

fn setup_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let panic_message = if let Some(s) = info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Unknown panic".to_string()
        };
        
        let location = if let Some(location) = info.location() {
            format!("{}: {}", location.file(), location.line())
        } else {
            "Unknown location".to_string()
        };

        log::error!("════════════════════════════════════════════════════════════");
        log::error!("程序崩溃 (PANIC)");
        log::error!("位置: {}", location);
        log::error!("错误: {}", panic_message);
        log::error!("════════════════════════════════════════════════════════════");
        
        // 确保日志写入文件
        let _ = std::process::Command::new("sync").status();
    }));
}

fn main() -> eframe::Result<()> {
    if let Err(e) = setup_logging() {
        eprintln!("日志初始化失败: {}", e);
    }
    
    setup_panic_hook();
    log::info!("程序启动");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([800.0, 600.0])
            .with_title("Intercom Tags Manager"),
        ..Default::default()
    };

    eframe::run_native(
        "Intercom Tags Manager",
        options,
        Box::new(|cc| {
            // 安装图片加载器
            egui_extras::install_image_loaders(&cc.egui_ctx);
            
            // 设置样式（在初始化时设置，避免运行时频繁修改导致崩溃）
            setup_style(&cc.egui_ctx);
            
            // 加载中文字体
            load_chinese_fonts(&cc.egui_ctx);
            
            Ok(Box::new(IntercomTagsApp::new()))
        }),
    )
}

fn setup_style(ctx: &egui::Context) {
    use egui::Color32;
    
    let mut style = (*ctx.style()).clone();
    
    // 主题颜色
    let bg_color = Color32::from_rgb(249, 250, 251);
    
    style.visuals.panel_fill = bg_color;
    
    ctx.set_style(style);
}

fn load_chinese_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    
    // macOS 中文字体路径（按优先级排序）
    let font_paths = [
        "/System/Library/Fonts/PingFang.ttc",           // 苹方字体（现代）
        "/System/Library/Fonts/Hiragino Sans GB.ttc",   // 冬青黑体
        "/Library/Fonts/Arial Unicode.ttf",             // Arial Unicode
        "/System/Library/Fonts/Helvetica.ttc",          // Helvetica fallback
    ];
    
    let mut loaded_fonts = vec![];
    
    for path in &font_paths {
        if let Ok(font_data) = std::fs::read(path) {
            let font_name = format!("font_{}", loaded_fonts.len());
            fonts.font_data.insert(
                font_name.clone(),
                std::sync::Arc::new(egui::FontData::from_owned(font_data)),
            );
            loaded_fonts.push(font_name);
            log::info!("Loaded font: {}", path);
        }
    }
    
    // 将中文字体插入到 Proportional 和 Monospace 字体族的最前面
    // 这样中文字符会优先使用这些字体
    if !loaded_fonts.is_empty() {
        if let Some(proportional) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            // 在默认字体后插入中文字体
            let default_fonts = proportional.clone();
            proportional.clear();
            proportional.extend(loaded_fonts.clone());
            proportional.extend(default_fonts);
        }
        
        if let Some(monospace) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
            let default_fonts = monospace.clone();
            monospace.clear();
            monospace.extend(loaded_fonts);
            monospace.extend(default_fonts);
        }
        
        ctx.set_fonts(fonts);
        log::info!("Chinese fonts configured successfully");
    } else {
        log::warn!("No Chinese fonts found, UI may display incorrectly");
    }
}
