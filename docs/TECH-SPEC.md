# TECH-SPEC.md — 〈WORKBENCH〉 技术契约 v1

> 配套 `SPEC-v1.md`(功能)、`CLAUDE.md`(红线)、`WORKFLOW.md`(协作)。
> 本文中的选型是**已拍板的决定**。结论来自 2025–2026 技术调研(见仓库内调研报告)。

---

## 1. 已冻结的技术栈

| 层 | 选型 | 备注 |
|---|---|---|
| 核心 | **Rust**,单一特权进程(= Tauri 核心) | 对外暴露单一 typed 契约接口(CLAUDE INV-6) |
| 外壳 | **Tauri 2.x**,原生 only | 已稳定可长期演进 |
| 前端 | **TypeScript + web**,框架选 **React 或 Svelte 之一并固定** | 决定下面用 React Flow 还是 Svelte Flow |
| 存储 | **SQLite**(`rusqlite` 或 `sqlx`)+ WAL,**手写事件溯源** | 不自建存储引擎;不引入 `cqrs-es` |
| AI 接入 | **Tauri sidecar + tokio spawn 外部 CLI**,`--output-format json/stream-json` + JSON schema | 借鉴 open-design 实现 |
| 2D 节点图 | **React Flow / Svelte Flow**(MIT) | 匹配前端框架 |
| 3D(近期) | **Three.js 只读预览** | 先验证 Tauri 各平台 WebGL |
| 3D(远期) | **外包 Blender**(socket addon)+ Unity 当数据消费端 | 永不自建交互式 3D 编辑器 |
| 发布 | `tauri build` + itch.io `butler` | 增量推送 |

---

## 2. 五块地基:决定 + 关键坑

### 2.1 Tauri 2.x
- 已稳定(2.0 于 2024-10 发布),适合长期演进。安装包小(<10MB)、内存低、安全模型按需授权。
- **关键坑(必须早测):Linux webkitgtk 对 WebGL/WebGL2/WASM 支持不稳**(context lost、Nvidia 需环境变量 workaround)。这直接影响 3D。
- **大数据(关卡序列化)别频繁过 IPC**——Windows 上大 payload 慢;优先用 `convertFileSrc` 直读文件。
- 三平台 webview 不一致(Win=WebView2/Chromium,Mac/Linux=WebKit),必须三平台都测。

### 2.2 Rust 事件溯源 on SQLite
- **手写**:一张 `events(aggregate_id, seq, type, payload_json, ts)` 表,`(aggregate_id, seq)` 唯一键 + 乐观锁;读时按 seq fold 重建;projection 是可随时从事件流重建的派生表。serde 做 (de)serialization,开 WAL。
- 对你的 C++ 背景:核心事件存储 + fold + 一两个 projection **约 1–2 周**。
- **不要**一上来引入 `cqrs-es`(它的 DDD/aggregate 抽象对单机工具偏重);等 projection 查询/并发真复杂了再议。

### 2.3 CLI agent 子进程(SPEC 的 U5,最高风险)
- **Tauri sidecar**(`bundle > externalBin`,按 target-triple 命名)把外部二进制打包;**长任务用 `spawn()` 不是 `execute()`**,通过 `CommandEvent::Stdout(line)` 逐行收再 emit。capabilities 里授权 `shell:allow-spawn`。
- **解析为 typed proposal**:用 `--output-format json --json-schema '{...}'`(Claude)/ `--output-schema`(Codex),**同一份 schema.json 当两个 CLI 的可移植契约**;流式用 `stream-json` 逐行 `JSON.parse`。
- 坑:stream-json schema 有版本(要 pin);解析失败的行当 `raw` 事件别崩;子进程用 `\r` 刷屏会导致 stdout 直到退出才 flush(agentic CLI 一般行式 JSON,影响小)。

### 2.4 2D 节点图
- **React Flow / Svelte Flow(MIT,可商用)**:拖拽/缩放/连线/多选/增删开箱即用,自定义节点 = 普通组件、可挂属性面板。正是"节点=房间、边=带方向连接、可编辑"的核心用例。
- 自己用 SVG/Canvas 从头做:只在需要极特殊交互时才值得。拓扑数据走事件溯源,图只是 projection 可视层。

### 2.5 open-design(nexu-io)——直接抄的活蓝本(Apache-2.0)
架构几乎一一对应(它用 Node daemon,你用 Rust core 替代)。**逐条借鉴:**
1. **PATH 检测**:扫 `PATH` 找 CLI 二进制(Windows 追加 `PATHEXT` 的 .EXE/.CMD/.BAT)+ `--version` 3 秒超时探测 + 二进制存在即标 available;**第二信号查 `~/.claude/`、`~/.codex/` 等配置目录**(绕过 GUI 启动进程的"最小 PATH");结果缓存 + Rescan。
2. **spawn/解析**:每 agent 一套 `buildArgs` flag;`stream-json` JSON Lines 逐行解析;解析失败发 `raw`;**tool_use 在完整消息到达时才 emit**(与立即 emit 的 text delta 分开);partial JSON 用 block map 累积。
3. **大 prompt 走 stdin**:绕过 Windows `CreateProcess` 命令行长度上限(32767 字符,经 cmd.exe 降到 8192)导致的 spawn ENAMETOOLONG;对走 argv 的加长度预算守卫。
4. **iframe 沙箱**:`sandbox="allow-scripts"` **不加** `allow-same-origin`;注入内存版 storage shim(因无 same-origin 会让 localStorage 抛错);postMessage comment bridge 做"点预览元素 → 给 agent 发精准编辑指令"。
5. **持久化按自己需求**:你用 Rust+SQLite 事件溯源(比 open-design 的 `history.jsonl` 更适合 typed/projection 查询/事务);artifact/资产仍放扁平项目目录、git 友好。

---

## 3. 3D 路线(已破幻想 · 分阶段)

**结论:不自建交互式 3D 关卡编辑器。** 那是数人月–数人年工程,且 Tauri 的 Linux webview 跑 WebGL 最脆——在最脆地基上盖最重的楼。

| 阶段 | 做什么 |
|---|---|
| 近期 | **Three.js 只读 3D 预览**(加载 glTF)。**先验证 Tauri 各平台(尤其 Linux webkitgtk)的 WebGL**——这是 go/no-go 基准 |
| 远期 | **交互式 3D 编辑外包 Blender**:写轻量 Blender addon(socket server,仿 Blender MCP / Plasticity Bridge);Rust core 通过 glTF/USD 文件 + socket 把 typed 关卡数据推给 Blender,addon 重建/编辑场景并回传 |
| 远期 | **Unity 当数据消费端 / 目标运行时**(JSON/ScriptableObject/USD 重建关卡) |
| 远期 | TA/VFX 野心**全寄托 Blender 侧**(几何节点/Cycles/EEVEE);工作台永远只当"typed 关卡数据唯一真源 + 集成脊柱" |

**这和整个架构哲学闭环**:就像"用已装的 CLI 当 AI 引擎",3D 就"用已装的 Blender 当 3D 引擎"——同一招。交换格式近期用 **glTF**(USD 适合多 DCC 大管线,小团队别过早上;OpenUSD Core Spec 1.0 已于 2025-12 发布但材质/动画 schema 仍未定稿)。

**会改变建议的基准:** 若只发 Windows/Mac(放弃 Linux 一等支持),WebGL 风险大降,自建只读/轻量 3D 的上限略升——但"自建完整交互式编辑器"仍不做。

---

## 4. 明确推迟 / 授权执行者自定(不回头问人类)

| 项 | 现状 / 约束 |
|---|---|
| 存储:图 vs 关系 | **已定:typed 关系 + SQLite 事件溯源**。约束:必须支持 typed schema + 事件日志 |
| 前端框架 React vs Svelte | 第一次动手时选一个并固定;决定 React Flow / Svelte Flow |
| 原生封装 | 已定 Tauri,native only |
| CLI 之外的 AI 接入(本地模型直连等) | MVP 仅 CLI;post-MVP 再说 |
| 精确几何 / 序列化进引擎契约格式 | D-full / 统一出口阶段再定 |
| 工况间数据是否共享一份 | 只在 D-full(多工况)才有意义;MVP 只有拓扑一种表达 |

---

## 5. 许可证纪律(重申)
MIT/Apache/BSD 自由用;弱 copyleft 谨慎;**GPL/AGPL 只能当外部独立程序调用,绝不进代码库**(Tiled 是 GPL → 只读其格式/调独立进程)。详见 CLAUDE.md §5。
