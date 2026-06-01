# Screenshot History

常驻系统托盘的截图历史管理工具。**自动记录你的每一张截图**（微信/QQ/PrintScreen/截图工具等），并能**按截图里的文字（OCR）**把历史截图搜回来。

技术栈：**Rust + Tauri 2** 后端 + **Vue 3 / Pinia / TypeScript** 前端。体积小、空闲内存占用低，不额外打包浏览器内核（复用系统 WebView）。

---

## 功能

- **自动捕获** — 后台每 500ms 轮询剪贴板，截图工具（Win+Shift+S、PrtScn、Snipping Tool、微信/QQ 截图等）把图片放进剪贴板时即保存，按 `年/月` 归档。
- **区分「复制」与「截图」** — 你只是*复制*的图片（在资源管理器里复制文件、从网页复制图片）会被自动跳过，历史里只留真正的截图。
- **OCR 全文检索** — 每张截图经 Tesseract（中文+英文）识别，存入 SQLite FTS5 全文索引；可按截图里出现的文字搜索。
- **时间线浏览** — 倒序缩略图网格，高分屏下缩略图依然清晰。
- **详情预览** — 原图预览支持**滚轮缩放**（对准光标缩放）、**拖拽平移**、**双击复位**；显示来源应用、文件名、时间、尺寸、大小和 OCR 全文。
- **灵活删除** — 可选只删历史记录，或连磁盘上的图片文件一起删。
- **系统托盘 + 全局热键** — 关闭窗口仅最小化到托盘；随时按 **Ctrl+Shift+H** 显隐窗口。

---

## 存储位置

| 内容 | 位置 |
|------|------|
| 截图原图 (PNG) + 缩略图 | `<图片>\ScreenshotHistory\Screenshots\YYYY\MM\` |
| 数据库 (SQLite + FTS5) | `<图片>\ScreenshotHistory\history.db` |
| 用户自行添加的 OCR 模型 | `%LOCALAPPDATA%\ScreenshotHistory\tessdata\` |

截图按内容哈希去重，重复复制同一张图不会产生重复记录。

---

## 安装使用

从 Releases 下载最新的 **`Screenshot History_x.y.z_x64_en-US.msi`** 安装即可。**OCR（中文+英文）开箱即用** —— 安装包已内置 Tesseract 引擎和语言模型，无需另装任何东西。

安装后应用常驻托盘，自动记录截图。按 **Ctrl+Shift+H** 打开主窗口浏览/搜索。

---

## OCR 说明

应用通过调用 `tesseract` 命令行实现 OCR，按以下顺序查找可用引擎：

1. **随应用打包**（安装包自带）—— `<resources>\tesseract\`
2. 系统 **PATH** 上的 `tesseract`
3. 常见安装目录 —— `C:\Program Files\Tesseract-OCR\`、winget/choco 安装位置

识别用 `chi_sim+eng`、页面分割模式 `--psm 4`（单列变长文本），比默认模式更适合截图。缺中文模型时自动退回纯英文。完全找不到引擎也不影响捕获和浏览，只是 OCR 文字为空。

要增加其它语言，把对应的 `*.traineddata` 放进 `%LOCALAPPDATA%\ScreenshotHistory\tessdata\`（这个目录里也要保留 `eng.traineddata`，因为 `--tessdata-dir` 只指向单一目录）。

---

## 从源码构建

### 环境要求

- [Rust](https://rustup.rs/) stable（Windows 用 MSVC 工具链）+ MSVC 生成工具
- [Node.js](https://nodejs.org/) 18+
- Windows 10/11

### 步骤

```bash
# 1. 安装前端依赖
npm install

# 2.（可选，让安装包内置 OCR）准备打包用的引擎。
#    会把 Tesseract + chi_sim/eng 模型收集到 src-tauri/tesseract/。
#    需要 PowerShell；本机没装 Tesseract 时用 winget 自动装。
powershell -ExecutionPolicy Bypass -File scripts/prepare-tesseract.ps1

# 3a. 开发模式（Vite + Tauri 热重载）
npm run tauri:dev

# 3b. 打包发布安装包（MSI + NSIS）
npm run tauri:build

# 3c. 或只出 exe / 只出 MSI
npm run tauri:build -- --no-bundle
npm run tauri:build -- --bundles msi
```

产物：

- exe → `src-tauri/target/release/screenshot-history.exe`
- MSI → `src-tauri/target/release/bundle/msi/`（内置 OCR 时约 42 MB）
- NSIS → `src-tauri/target/release/bundle/nsis/`

> **注意**：内置的 Tesseract 引擎（约 123 MB）**不纳入 git** —— 打包前请先运行 `scripts/prepare-tesseract.ps1` 生成 `src-tauri/tesseract/`。不生成也能构建，只是运行时改为依赖系统已装的 Tesseract。

仅检查某一侧：

```bash
npm run build                  # 前端类型检查 + 打包
cd src-tauri && cargo check    # 后端编译检查
```

---

## 项目结构

```
src/                     # Vue 3 前端
  App.vue                #   外壳：搜索栏 + 列表 + 预览 + 实时事件
  api.ts                 #   Tauri 命令的 TS 类型封装
  stores/history.ts      #   Pinia store（历史 + 搜索状态）
  components/
    SearchBar.vue        #   搜索框 + OCR 状态指示
    HistoryList.vue      #   缩略图时间线网格
    ImagePreview.vue     #   原图预览：缩放/平移/详情/删除

src-tauri/src/           # Rust 后端
  lib.rs                 #   装配：状态、线程、托盘、热键、窗口图标
  clipboard_monitor.rs   #   500ms 轮询 → 保存 → 入库 → 推送 → 入队 OCR
  foreground.rs          #   前台应用名 + 「复制 vs 截图」启发式判断
  image_saver.rs         #   PNG 保存 + 高分屏缩略图生成
  db/mod.rs              #   SQLite 表结构、FTS5 搜索、增删改、迁移
  ocr_engine.rs          #   tesseract CLI 解析 + 调用
  ocr_worker.rs          #   后台 OCR 消费线程
  commands.rs            #   暴露给前端的 Tauri 命令
  tray.rs                #   系统托盘菜单

scripts/
  gen-icons.mjs          # 从源图生成应用图标（PNG + ICO）
  prepare-tesseract.ps1  # 收集打包用的 OCR 引擎 + 模型
```

## 数据流

```
截图工具 ──> 剪贴板
              │  (500ms 轮询，哈希去重，跳过复制的图片)
              ▼
      clipboard_monitor  ──保存──>  磁盘上的 PNG + 缩略图
              │                          │
              ├──插入记录──> SQLite ──────┤
              │                          │
              ├──emit "new-screenshot"──> 前端（立即出卡片）
              │
              └──入队──> ocr_worker ──tesseract──> ocr_text
                              │
                              ├──更新记录 + FTS 索引
                              └──emit "ocr-updated"──> 前端
```

## 实现差异（相对早期技术文档）

1. **OCR 用命令行而非 `leptess`** —— `leptess` 在 Windows 上需 Leptonica/Tesseract 的 C 开发库 + clang 才能编译。为保证开箱即可构建、跨平台一致、并能在缺引擎时优雅降级，改用调用 `tesseract` 命令行，并把引擎随安装包打包以做到开箱即用。
2. **API 为 Tauri 2** —— 托盘、事件、路径等 API 采用 Tauri 2 写法（`TrayIconBuilder`、`emit`、`app.path()` 等），而非文档示例的 1.x 写法。

## License

MIT
