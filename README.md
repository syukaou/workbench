# WORKBENCH — 游戏策划工作台

面向单人/超小团队游戏开发者的本地优先桌面工作台。在引擎之外完成关卡的"设计 + 验证 + 序列化"。

MVP：AI 生成关卡拓扑 → 人扩展 POI → POI 挂真实体。

> **新接手?先读根目录 [`AGENTS.md`](AGENTS.md)** —— 运行/构建/测试、开发流程、当前看板状态与下一步都在那里。架构红线见 [`docs/CLAUDE.md`](docs/CLAUDE.md)。

## 运行

当前是 localhost web 应用(WASM core 跑在浏览器 + 内存 store)：

```bash
# 前端(默认 http://localhost:5173）
cd ui && npm install && npm run dev

# 可选：原生 AI 提议服务(localhost:5198，需要它才有真实 AI 提议；否则前端回退到 WASM mock)
cargo run -p workbench-core --bin workbench-server
```

测试 / 闸门：`cargo test --workspace`(含 INV-1..8 不变量测试)+ `cd ui && npm run build`(tsc + vite)。

## 技术栈

- **Core**: Rust (`workbench-core`) — 确定性事件溯源核心,编译成 WASM 在浏览器运行(`ui/src/core-pkg/`,已提交的生成产物);native 构建另有 `workbench-server`。
- **Frontend**: TypeScript + React + Vite + React Flow(2D 拓扑)+ Three.js(3D 只读预览)。
- **Store**: 当前内存版(`core/src/memory_store.rs`)。SQLite + Tauri 外壳是**冻结的远期打包目标**,尚未落地(见 `docs/CLAUDE.md` §0.5)。
- **CI**: GitHub Actions(`.github/workflows/ci.yml`)。

## 开发流程

`main` = PR-only + CI 门控。开发由自治 `claude -p /work` worker 在 Hermes Kanban 板上逐卡推进(`scripts/run-worker.sh <card_id> <board>`)。详见 [`AGENTS.md`](AGENTS.md) 与 `docs/WORKFLOW.md`。

更多设计文档见 `docs/` 目录。
