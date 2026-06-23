# AGENTS.md — WORKBENCH 开工必读

> 任何 agent(Claude Code / OpenCode / 其它)接手本仓库,**先读这一页**,再读架构红线 [`docs/CLAUDE.md`](docs/CLAUDE.md)。
> 本页 = **稳定的方向/操作**(§1–§5)+ **易变的当前状态**(§6–§7,请按页内命令自行复核最新值,别盲信)。

---

## 1. 当前架构(权威 · 覆盖各 SPEC 里的冻结表述)

- **形态 = localhost web 应用。** Rust `core` crate 编译成 **WASM** 在浏览器运行;store 是**内存版**(`core/src/memory_store.rs`)。可选的原生 `workbench-server` 在 **localhost:5198** 提供真实 AI 提议。
- **前端** = Vite + **React**(已定,不再是 React/Svelte 二选一)+ React Flow(2D 拓扑画布)+ Three.js(3D 只读预览)。源码 `ui/src/`。
- **`ui/src/core-pkg/` 是已提交的 WASM 生成物**,被 `ui/src/coreBridge.ts` 直接 import,是 UI 实际调用的 core(改 core 后必须重建,见 §3)。
- **冻结/远期(尚未落地,别照着建运行时):** Tauri 外壳、SQLite 持久层、itch/`butler` 发布。仓库里**没有** `src-tauri/`,Cargo 里**没有** tauri/sqlite。`docs/CLAUDE.md` §4/§6 和 `docs/TECH-SPEC.md` 描述的是这个远期目标,不是现状(以 `docs/CLAUDE.md` §0.5 为准)。
- **不变量 INV-1..8 + 单一契约边界(`WorkbenchCore` 唯一导出)是准确且已编码成测试的**(`core/src/invariant_tests.rs`),与外壳是 web 还是将来 Tauri 无关。红线全文见 `docs/CLAUDE.md` §1。

## 2. 怎么运行 / 测试

```bash
# 前端(默认 http://localhost:5173;端口被占就 npm run dev -- --port 5180 换干净端口)
cd ui && npm install && npm run dev

# 可选:真实 AI 提议服务(localhost:5198)。不开则前端回退 WASM mock(UI 显示 "using local mock")
cargo run -p workbench-core --bin workbench-server
```

**两道闸门(worker 完成卡前必须都过):**
```bash
cargo test --workspace      # 含 INV-1..8 不变量测试(当前 94 passed）
cd ui && npm run build      # tsc -b + vite build
```

> ⚠️ **headless 测试抓不到运行时 GUI bug。** cargo test + tsc/vite build 全绿,app 仍可能白屏(React hooks 顺序、wasm `SystemTime` panic、stale `.tsbuildinfo` 都真实发生过)。涉及 UI 的改动,**在真浏览器里加载 app 走一遍验收**,并对可量化项(如对比度)**实测**——截图会漏掉可测的回归。

## 3. 改了 core/ 的 Rust?必须重建 WASM

`ui/src/core-pkg/` 是 core 的 wasm-bindgen 产物,**已提交**且被 UI 直接 import;**CI 不会重建它**。改了 `core/` Rust 后 UI 仍在用旧 WASM,直到你重建:

```bash
wasm-pack build core --target web --out-dir ../ui/src/core-pkg -- --no-default-features
```

`--no-default-features` 去掉 native 的 rusqlite,否则 wasm32 编译失败。**忘了重建 = 浏览器里悄悄跑旧 core。**(尚无 `scripts/build-wasm.sh` 封装 + CI 同步校验,已列为 harden 待办,见 §7。)

## 4. 开发流程 / 怎么派活

- `main` = **PR-only + CI 门控**。每张卡走独立 worktree 分支(`.worktrees/<id>/`),**绝不直推 main**。
- **派一个 worker 干一张卡:**
  ```bash
  scripts/run-worker.sh <card_id> <board>     # 例:scripts/run-worker.sh t_xxxx workbench-harden
  ```
  它 exec `claude -p "/work <id>"`(Opus,bypassPermissions),按 `.claude/commands/work.md` 契约:claim → 读卡规格 → 在 worktree 实现 + 配套不变量测试 → 闸门(cargo test + ui build)→ push feature 分支 → complete 交 handoff;**碰红线则 `block`,绝不硬做**。
- **板由环境变量 `HERMES_KANBAN_BOARD` 选**(run-worker.sh 第 2 参数会导出它)。**这个 Hermes CLI 不认 `--board` 旗标作板选择**,只认环境变量。
- **DEV 引擎 = 编排器驱动 Opus,不是 Hermes 自动调度器。** `hermes kanban dispatch` / gateway 会 spawn 它自己的 `hermes chat` agent(gpt-5.5/deepseek、无 `/work` 红线契约、够不到 Opus、不自动建 worktree),与验证过的 worker 不兼容——别用它跑 DEV。
- **新建卡保持 unassigned。** 有一个**全局共享、未设上限的 gateway 调度器**在所有板上跑,会抢调度 assigned-to-installed-profile 的卡;unassigned 它会跳过。别去动那个共享 gateway(它服务用户的其它项目)。

## 5. CI & 合并闸

- **GitHub Actions** `.github/workflows/ci.yml`:job `rust`(`cargo test --workspace`,含不变量)+ job `ui`(`npm ci` + `npm run build`,node 22)。push / PR 触发。**不是 TeamCity**(老文档里的 TeamCity 已过时)。
- **main 分支保护**:**请先复核** —— `gh api repos/syukaou/workbench/branches/main/protection`。本次交接整理时它**尚未启用**(返回 404),即 PR-only 只是约定、未物理强制。**这是转交前最高优先的待办**:仓库 owner 在 GitHub 开启 require PR + 两个 check(`Rust — cargo test (incl. invariants INV-1..8)`、`UI — tsc + vite build`)+ include-admins。
- `workbench-guardian-v2` cron(`ff6a081e0a2f`)会直推/自动合并 main,**有意保持暂停,别 resume**。分支保护启用后它即便被 resume 也无害(直推会被拒)。

## 6. 当前看板状态 & 下一步(易变 · 用 `hermes kanban` 复核)

复核:`HERMES_KANBAN_BOARD=workbench-harden ~/.local/bin/hermes kanban list` · `gh pr list`

- **`workbench-mvp`** — Stage-1 归档,26 卡全 done,**当只读,别在上面开新卡**。
- **`workbench-harden`** — 活跃(技术债 / INV):
  - `t_9459a3fb` **done** — INV-4/7 守卫改成递归扫 core 源码(原来只扫 Cargo.toml,`cli_bridge`/`cli_server` 的 `std::process`/`std::net` 违例悄悄过 CI)。落在 **PR #16,待合**。
  - `t_281dddf1` **blocked** — 把 `cli_bridge`+`cli_server` 迁出 core(**轻量方案**:搬进新 `workbench-bridge` crate,仍 localhost web,**不上 Tauri**)。阻塞原因 = 等 PR #16 先合(它要改守卫的 allowlist,必须从带守卫的 main 切分支)。

**严格按序,别越序:**
1. 仓库 owner 启用 main 分支保护(§5)。
2. 合 **PR #16**(守卫落 main)。
3. `hermes kanban --board workbench-harden unblock t_281dddf1` → 用 **Opus** 跑它(架构卡,人复核;它会把守卫 allowlist 清空 → INV-4 物理恢复)。**别在 PR #16 合进 main 之前启动 t_281dddf1。**
4. 之后可并行铺其它 harden 卡 / 起 `workbench-feature` 板(post-MVP:资产流水线、统一导出、D-full 等,见 `docs/SPEC-v1.md` §8)。

## 7. 已知的、有意保留的违例 / 别动 + harden 待办

- `core/src/cli_bridge.rs`(`std::process` 调 opencode)+ `core/src/cli_server.rs`(`std::net` 监听 :5198):**有意的、`#[cfg(native)]`-gated 的 INV-4 原型期遗留**,由 `t_281dddf1` 排期迁出。**别顺手清理/删除**——会破坏迁移卡。注意 main 上的源码级守卫尚未合(只在 PR #16);别去信一个还没落 main 的守卫。
- `cli_bridge.rs::parse_proposals` 会静默丢弃解析失败的提议(原型期),`propose()` 把解析错误吞成空列表。按 SPEC-v1 §5.7 U5 应把失败行原样暴露——harden 待办。
- 其它 harden 待办(可建卡):补 `scripts/build-wasm.sh` + CI 校验 WASM 同步;给 `core/src/wasm.rs`(UI 实际调用层,native `cargo test` 不编译它)加 wasm-bindgen smoke 测试;清掉 4 个 build/clippy 警告(`log.rs:165`、`memory_store.rs:94` 的 dead_code 等);收紧 INV-4 的 Cargo.toml 子串扫描(短 needle 如 `gl`/`http` 有误报风险);`ui/src/mockData.ts` 其实是真 WASM 桥接层(非 mock 数据),文件名误导,可改名 `coreData.ts`。
