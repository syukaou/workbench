# MVP 验收清单 — SPEC §4.3 闭环

> 自动化已覆盖**数据通路**:`cargo test`（含 `mvp_acceptance_spec_4_3_end_to_end` 把整条 §4.3 跑通，加 INV-3/5/6 各环节护栏）+ CI（push/PR 跑红线测试 + UI 构建）。
> 这份清单是**人类的视觉验收**——headless 跑不了 GUI，需你亲跑一遍对照打勾。

## 怎么跑

```bash
# 1. （可选）真 AI 提议：起本地 CLI 提议 server（否则前端回退 WASM mock）
cargo run -p workbench-core --bin workbench-server   # localhost:5198

# 2. 前端
cd ui && npm install && npm run dev                  # 打开 localhost:5173
```

## §4.3 验收清单（逐条打勾）

- [ ] **AI 出骨架（提议，未落盘）**：底部 AI 面板输入「中央大厅 + 三支线 + 一条单向捷径」→ 点 Propose。画布出现**虚线、弱化**的 pending 节点/边（待接受态，DESIGN §5 / INV-3）。
- [ ] **接受前 core 不变**：pending 显示时，已落盘拓扑不变（提议只在 overlay，没进 core）。
- [ ] **接受 → 落盘**：点接受 → pending 变实线、成为正式节点；单向捷径方向正确、中央大厅有 spawn 标记。
- [ ] **人扩展**：手动加一个房间（工具栏/快捷键），连一条边，右键给节点加标记。
- [ ] **A 区实体**：右侧栏实体管理器定义一个 `Boss` 类型 → 建实例填 `hp` 等字段（全程不靠 seed 数据）。
- [ ] **POI 挂实体**：选中某节点 → 给它加一个 POI → 绑定刚建的 Boss 实例。
- [ ] **撤销任意步**：顶栏 Undo 能逐步回滚（建节点/连边/挂 POI 都可撤），Redo 复原。撤销走的是 core 事件日志（不是 UI 本地栈）。
- [ ] **存档/重启持久**：Save 下载 `.workbench.json` → 刷新页面 → Load 该文件 → 拓扑、坐标、实体、POI 全在。
- [ ] **3D 只读预览**：切到 3D，能看到节点/边；回 2D 改动后 3D 随之更新。
- [ ] **单一产品观感**：侧栏（shadcn/Radix）与画布（React Flow）配色一致（同一套 design token，DESIGN §2），深色冷性中性，不是两张皮。

## 通过判据

上面全勾 = SPEC §4.3「第一个对策划真实减负的可用版本」达成。任一条不过 → 在对应 worktree/卡上记问题，排查修复。

## 红线自查（CLAUDE.md §1，已由不变量测试守护）

- INV-1/2：所有写入走 core 命令 → 事件日志；UI 只是 `get_state()` 的渲染缓存。
- INV-3：AI 提议接受前不改 core。
- INV-5：撤销 = 从事件日志重折叠；存档读档纯靠 fold(events) 重建。
- INV-4/6/7：core 无 LLM/HTTP/渲染依赖；只暴露 `WorkbenchCore` 单一契约；渲染只在前端。
