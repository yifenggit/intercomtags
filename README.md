# Intercom Tags Manager

基于 Rust + egui 的 macOS 桌面应用，用于批量给 Intercom 联系人打标签。

## 系统要求

- macOS 10.13 或更高版本
- Intercom API Token

## 安装

### 从 .dmg 安装（推荐）

1. 下载 `Intercom Tags Manager-x.x.x.dmg`
2. 双击打开 DMG 文件
3. 将应用拖拽到 `Applications` 文件夹
4. **首次打开需移除隔离属性**（见下方说明）

> ⚠️ 由于应用未签名，macOS 会显示"已损坏"警告，需要执行以下命令：

```bash
# 移除隔离属性
xattr -cr /Applications/Intercom\ Tags\ Manager.app
```

或使用右键方式：
1. 右键点击应用 → 选择"打开"
2. 在弹出对话框中点击"打开"确认

> 📌 这是因为应用没有 Apple 开发者签名，macOS Gatekeeper 会阻止运行。

### 从源码构建

```bash
# 克隆仓库
git clone https://github.com/yourname/intercomtags.git
cd intercomtags

# 构建
cargo build --release

# 运行
./target/release/intercomtags
```

## 功能特性

### 1. 文件选择
- 支持选择 CSV 和 XLSX 文件
- 自动解析邮箱和标签列
- 智能跳过标题行

### 2. 手动输入
- 直接在多行文本框输入邮箱
- 每行一个邮箱地址
- 可以统一设置标签

### 3. 配置管理
- API Token 配置（支持密码隐藏）
- 重试次数设置（1-10次）
- 并发数控制（1-20）
- 配置自动保存

### 4. 执行监控
- 实时进度条显示
- 成功/失败统计
- 详细日志输出
- 时间戳记录

## 使用方法

### 启动应用

```bash
./target/release/intercomtags
```

### 基本流程

1. **配置 Token**
   - 在左侧配置区域输入 Intercom API Token
   - 调整重试次数和并发数（可选）
   - 点击"保存配置"

2. **选择输入方式**
   
   **方式一：文件输入**
   - 选择"📁 文件"选项卡
   - 点击"📂 选择文件 (CSV/XLSX)"
   - 选择包含邮箱的文件
   
   **方式二：手动输入**
   - 选择"✏️ 手动输入"选项卡
   - 输入标签名称
   - 在文本框中输入邮箱（每行一个）

3. **执行打标签**
   - 点击"▶️ 开始执行"按钮
   - 观察右侧日志输出
   - 查看成功/失败统计

### 文件格式

**单列格式**
```
email
user1@example.com
user2@example.com
```
需要手动输入标签名称

**双列格式**
```
email,tag
user1@example.com,VIP
user2@example.com,VIP
```
文件中已包含标签信息

## 开发

### 环境要求

- Rust 1.70+
- macOS

### 编译

```bash
# 开发版本
cargo build

# 发布版本（优化编译）
cargo build --release
```

### 打包为 .dmg

```bash
# 使用打包脚本
./build-dmg.sh [版本号]

# 示例
./build-dmg.sh 1.0.0
```

输出文件位于 `dist/` 目录：
- `Intercom Tags Manager.app` - macOS 应用
- `Intercom Tags Manager-1.0.0.dmg` - 安装包

### 项目结构

```
intercomtags/
├── src/
│   ├── main.rs         # 入口、GUI 初始化
│   ├── app.rs          # 应用主逻辑
│   ├── intercom.rs     # Intercom API 封装
│   ├── file_parser.rs  # 文件解析（CSV/XLSX）
│   └── config.rs       # 配置管理
├── build-dmg.sh        # DMG 打包脚本
├── Cargo.toml
└── README.md
```

## 技术栈

- **GUI框架**: egui 0.33 + eframe
- **异步运行时**: tokio
- **HTTP客户端**: reqwest
- **文件解析**: calamine (Excel) + csv
- **序列化**: serde + serde_json
- **错误处理**: anyhow + thiserror

## 数据存储

| 类型 | 位置 |
|------|------|
| 配置文件 | `~/Library/Application Support/intercomtags/config.json` |
| 日志文件 | `~/Library/Application Support/intercomtags/logs/` |
| 导出文件 | 用户选择的目录 |

## 常见问题

### 1. API Token 无效

确保使用正确的 Intercom Access Token：
- 登录 Intercom → Settings → Developer → Authentication
- 创建或复制 Access Token

### 2. 找不到联系人

- 确认邮箱地址正确
- 确认联系人在当前 Workspace 中存在
- 检查 API Token 权限

### 3. 请求超时

- 减少并发数（默认 5）
- 检查网络连接
- 查看日志获取详细错误信息

### 4. 文件解析失败

支持的文件格式：
- CSV（UTF-8 编码）
- XLSX（Excel 2007+）

确保文件包含正确的列名：`email` 或 `email, tag`

## 与原 Go 版本的对比

| 特性 | Go版本 | Rust版本 |
|------|--------|----------|
| 界面 | 命令行 | 图形界面 |
| 文件选择 | 命令行参数 | 文件对话框 |
| Token配置 | 配置文件/参数 | 界面输入+保存 |
| 重试配置 | 命令行参数 | 界面滑块 |
| 进度显示 | 文本输出 | 进度条+日志 |
| 断点续传 | 支持 | 计划中 |

## 开发计划

- [x] 图形界面
- [x] 文件选择（CSV/XLSX）
- [x] 手动输入邮箱
- [x] 配置保存
- [x] 并发控制
- [x] 重试机制
- [x] 日志记录
- [ ] 断点续传功能
- [ ] 导出结果到文件
- [ ] 批量操作历史记录
- [ ] Token 验证功能
- [ ] 暗色/亮色主题切换

## 更新日志

### v0.1.0
- 初始版本
- 支持批量给 Intercom 联系人打标签
- 支持 CSV/XLSX 文件导入
- 支持手动输入邮箱

## 许可证

MIT License

## 贡献

欢迎提交 Issue 和 Pull Request！
