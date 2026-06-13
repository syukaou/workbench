# WORKBENCH — 游戏策划工作台

面向单人/超小团队游戏开发者的本地优先桌面工作台。在引擎之外完成关卡的"设计 + 验证 + 序列化"。

MVP：AI 生成关卡拓扑 → 人扩展 POI → POI 挂真实体。

## 开发

```
main ← PR only（Hermes 负责合并）
  └─ U1-event-log       ← 确定性核心 + 事件日志
  └─ U2-entity-model    ← 本体/实体定义
  └─ U3-level-topology  ← 关卡拓扑模型
  └─ U4-topology-graph  ← 拓扑图渲染 + 编辑
  └─ U5-ai-cli          ← AI-CLI 提议通道
```

每分支独立开发 → PR 进 main → Hermes review 门控后合并。

## 技术栈

- Core: Rust (`workbench-core`)
- Shell: Tauri 2.x
- Frontend: TypeScript + React/Svelte
- Storage: SQLite + WAL + 手写事件溯源

详见 `docs/` 目录。
