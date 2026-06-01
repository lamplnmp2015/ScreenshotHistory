# 截图历史管理 + 智能检索 —— 技术实现文档 (Rust + Tauri)

## 1. 项目概述

**目标**：开发一个常驻系统托盘的后台工具，自动记录用户所有截图（包括微信/QQ/PrintScreen等），支持 OCR 文字检索、时间线浏览、相似图片查找等功能。

**核心价值**：  
- 自动保存，永不丢失  
- 通过截图中的文字快速找回历史截图  
- 极低内存占用 (< 50 MB)  
- 跨平台支持（Windows / macOS / Linux）

## 2. 技术栈

| 层次             | 技术选择                                        |
| ---------------- | ----------------------------------------------- |
| 后端语言         | Rust 1.80+ (stable)                             |
| 前端框架         | Vue 3 + TypeScript + Vite                       |
| UI 组件库        | Naive UI 或 Tauri 内置的 webview                |
| 跨端框架         | Tauri 2.0 (使用 WRY/WebView2)                   |
| 剪贴板监听       | `arboard` + `tauri-plugin-clipboard`            |
| 全局热键         | `global-hotkey` (或 `tauri-plugin-global-shortcut`) |
| 图片处理         | `image` (Rust) + `base64` (传输)                |
| OCR 引擎         | **方案A**：本地调用 `tesseract` 命令行（跨平台）<br>**方案B**：自建 `leptess` (Tesseract binding) |
| 数据库           | SQLite (`rusqlite`) + 全文搜索 (FTS5)           |
| 系统托盘         | `tauri-plugin-tray` (官方推荐)                  |
| 前端状态管理     | Pinia                                            |
| 打包工具         | `tauri build` (生成 .msi/.dmg/.deb)             |

> **OCR 选择建议**：优先使用 `tesseract` 系统依赖 + `leptess` 绑定，避免额外 HTTP 服务。需要额外安装语言包（如 `tesseract-ocr-chi-sim`）。

## 3. 项目结构
screenshot-history/
├── src-tauri/ # Rust 后端
│ ├── Cargo.toml
│ ├── tauri.conf.json
│ ├── src/
│ │ ├── main.rs # Tauri 入口
│ │ ├── clipboard_monitor.rs # 剪贴板监听模块
│ │ ├── image_saver.rs # 保存图片文件
│ │ ├── ocr_engine.rs # OCR 调用模块
│ │ ├── db/ # SQLite 操作
│ │ │ └── mod.rs
│ │ └── tray.rs # 系统托盘逻辑
│ └── icons/ # 应用图标
├── src/ # 前端 (Vue 3)
│ ├── main.ts
│ ├── App.vue
│ ├── components/
│ │ ├── HistoryList.vue # 截图历史列表
│ │ ├── SearchBar.vue # 搜索框 + OCR 关键词
│ │ └── ImagePreview.vue # 大图预览
│ ├── stores/
│ │ └── history.ts # 截图历史 Pinia store
│ └── assets/
├── tesseract/ # 可选：打包 tesseract 语言包
└── README.md

## 4. 核心功能模块设计

### 4.1 全局剪贴板监听 (Rust)

**职责**：  
- 注册剪贴板内容变化回调  
- 当新内容为图片（PNG/JPEG/BMP）时，提取原始数据  
- 通过 Tauri 事件向前端发送“新截图捕获”通知，并传递图片 Base64 和元数据。

**技术实现**：  
使用 `arboard` 库轮询或监听（Tauri 插件 `clipboard-manager` 提供事件驱动更优）。  
由于 `tauri-plugin-clipboard` 目前不支持监听图片变化，我们采用 **后台定时轮询 + 哈希对比** 策略（每 500ms 检查一次，内存占用极低）。

**关键代码骨架** (`clipboard_monitor.rs`)：
```rust
use arboard::Clipboard;
use std::time::{Duration, Instant};
use tauri::Manager;

pub fn start_clipboard_monitor(app_handle: tauri::AppHandle) {
    std::thread::spawn(move || {
        let mut clipboard = Clipboard::new().unwrap();
        let mut last_hash = String::new();
        loop {
            if let Ok(image) = clipboard.get_image() {
                let img_data = image.to_png(); // 转为 PNG bytes
                let hash = format!("{:x}", md5::compute(&img_data));
                if hash != last_hash {
                    last_hash = hash;
                    // 发送事件到前端
                    let _ = app_handle.emit_all("new-screenshot", img_data);
                }
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    });
}
注意：在 main.rs 中调用 start_clipboard_monitor(app.handle())。
4.2 截图保存与元数据提取 (Rust)
职责：

收到前端转发的图片数据后，生成唯一文件名（时间戳 + UUID）

保存到用户配置目录（如 ~/Pictures/ScreenshotHistory/YYYY/MM/）

获取当前活动窗口名称（用于标注截图来源）

将路径、时间、窗口名存入 SQLite 数据库，并将 OCR 任务加入异步队列。

获取前台窗口（Windows/macOS/Linux 通用）：

使用 active-win 或 window-vibrancy 等库。推荐 active-win-rs。

rust
use active_win::get_active_window;

fn get_foreground_app() -> String {
    match get_active_window() {
        Ok(win) => win.app_name,
        Err(_) => "Unknown".to_string(),
    }
}
4.3 OCR 文字提取 (Rust)
方案：调用系统已安装的 tesseract 命令行（需用户提前安装），或通过 leptess 直接绑定。
这里选择 leptess 避免外部进程开销：

rust
use leptess::LepTess;

pub fn extract_text_from_image(image_path: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut tess = LepTess::new(None, "eng+chi_sim")?;
    tess.set_image(image_path)?;
    let text = tess.get_utf8_text()?;
    Ok(text)
}
优化：OCR 在独立线程池中异步处理，避免阻塞 UI。

4.4 数据库设计 (SQLite + FTS5)
表结构：

sql
-- 截图主表
CREATE TABLE screenshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_path TEXT NOT NULL,
    timestamp INTEGER NOT NULL,      -- Unix 毫秒
    source_app TEXT,
    width INTEGER,
    height INTEGER,
    file_size INTEGER
);

-- 全文搜索虚拟表 (FTS5)
CREATE VIRTUAL TABLE screenshots_fts USING fts5(
    content,        -- OCR 识别出的全文
    content=screenshots
);

-- 触发器：当插入 screenshots 时自动关联 FTS
CREATE TRIGGER screenshots_ai AFTER INSERT ON screenshots BEGIN
    INSERT INTO screenshots_fts(rowid, content) VALUES (new.id, '');
END;
查询：

sql
SELECT s.* FROM screenshots s
JOIN screenshots_fts fts ON s.id = fts.rowid
WHERE screenshots_fts MATCH ?   -- 用户输入的关键词
ORDER BY s.timestamp DESC;
4.5 前端界面 (Vue 3)
核心页面：

主窗口：历史列表（网格视图），支持滚动加载、按日期分组。

搜索栏：实时搜索（防抖 300ms），高亮匹配文字。

预览弹窗：点击图片放大，显示元数据（来源、尺寸、OCR 全文）。

系统托盘：右键菜单 → 打开主窗口 / 退出。

与前端的通信：

后端通过 emit_all("new-screenshot", base64) 推送新截图

前端通过 listen 事件接收并更新列表

前端调用 Tauri 命令：get_history(offset, limit)、search_by_text(keyword)、delete_screenshot(id) 等。

4.6 系统托盘及全局快捷键
使用 tauri-plugin-tray：

rust
// tray.rs
use tauri::{CustomMenuItem, SystemTray, SystemTrayMenu, SystemTrayEvent};

pub fn create_tray() -> SystemTray {
    let quit = CustomMenuItem::new("quit".to_string(), "退出");
    let show = CustomMenuItem::new("show".to_string(), "打开主窗口");
    let menu = SystemTrayMenu::new().add_item(show).add_item(quit);
    SystemTray::new().with_menu(menu)
}
全局热键（如 CmdOrCtrl+Shift+H 呼出主窗口）使用 tauri-plugin-global-shortcut。

5. 关键实现步骤（Claude Code 执行顺序）
初始化项目
cargo install create-tauri-app
cargo tauri init
选择 Vue + TypeScript。

配置 Cargo.toml 依赖
添加：arboard, image, rusqlite, tauri-plugin-tray, tauri-plugin-global-shortcut, leptess, md5, active-win-rs, chrono。

实现剪贴板监听模块 (clipboard_monitor.rs)

编写轮询函数，使用 arboard::Clipboard::get_image()

计算图片哈希去重

通过 app_handle.emit_all 发送图片 base64。

实现图片保存模块 (image_saver.rs)

接收 base64 数据，解码为字节

生成保存路径 (根据当前年月创建目录)

写文件并返回路径。

实现数据库模块 (db/mod.rs)

初始化数据库，创建表结构

实现插入、查询、删除等 CRUD。

实现 OCR 模块 (ocr_engine.rs)

调用 leptess，同步执行 OCR

开启独立线程池，从消息队列中取出图片路径进行识别，将结果更新回数据库的 FTS 表。

实现 Tauri 命令 (main.rs)

get_history_page

search_by_text

delete_screenshot

open_image_folder

前端开发

使用 @tauri-apps/api 的 listen 监听新截图事件，刷新列表

实现虚拟滚动（处理成百上千张截图）

实现防抖搜索框。

打包与分发

配置 tauri.conf.json 中的 bundle 选项

设置应用图标、产品名称

运行 tauri build 生成安装包。

6. 跨平台注意事项
平台	剪贴板格式	OCR 语言包安装命令	全局热键支持
Windows	CF_BITMAP	tesseract-ocr-setup.exe 勾选中文包	✔️
macOS	NSPasteboard	brew install tesseract tesseract-lang	✔️
Linux	取决于桌面	sudo apt install tesseract-ocr-chi-sim	✔️ (需处理 X11)
在 Linux 下可能需要额外配置 libxdo 等依赖，Tauri 会自动处理。

7. 性能优化要点
内存控制：后端不保存原始图片数据，仅存路径和缩略图的 base64（可选）。

缩略图生成：保存原图时同步生成 200px 宽度的缩略图，用于前端列表显示，减少大图加载。

数据库索引：在 timestamp 列建立索引。

OCR 去重：对于完全相同内容的截图（如同一区域连续截图），只 OCR 一次，共享识别结果。

异步非阻塞：所有 I/O 操作（保存图片、OCR、数据库写入）都应使用 tokio 或 std::thread 避免阻塞 UI。