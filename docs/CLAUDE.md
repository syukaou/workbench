# CLAUDE.md — 〈WORKBENCH〉 开发缰绳

> 这份文件你(Claude Code / OpenCode)**每次开工都要读**。它是本项目唯一的持续约束。
> 本项目是纯 vibe coding:**人类不会逐行读代码**。你是实现者 + 第一道守门人,自动化测试是物理护栏。
> 配套文件:`SPEC-v1.md`(做什么)、`TECH-SPEC.md`(用什么技术)、`WORKFLOW.md`(怎么协作/发布)。

---

## 0. 这是什么

一个面向**单人/超小团队游戏开发者**的本地优先桌面工作台:在引擎之外完成关卡的"设计 + 验证 + 序列化",抽象设计(数值/规则/关系)与空间设计(关卡)绑定在同一个 typed 模型里。**明确不服务多人专业团队。**

MVP 只做最小可用内核(见 SPEC-v1 §4):AI 生成关卡拓扑 → 人扩展 POI → POI 挂真实体。

---

## 1. 架构不变量(红线 · 违反即停)

这些是本项目全部价值的地基。**任何一条被侵蚀,做出来的就不再是这个产品。** 每条都给了"违反长什么样",你要能一眼认出自己是否跑偏。

| # | 不变量 | 违反长什么样(出现即停,不许这样做) |
|---|---|---|
| **INV-1** | **单一真源**:所有设计状态都是事件日志派生出的 typed 模型 | 状态被存在别处——UI 组件 state 里、临时 JSON、全局变量里当"真相" |
| **INV-2** | **唯一写入通道**:一切写入走 command → 校验 → 串行 append 到事件日志 | 任何地方绕过日志直接改模型/直接写状态表 |
| **INV-3** | **AI 是提议者不是裁决者**:AI 输出是 typed 提议,经人 review 接受后才落盘 | AI 的输出被自动 apply 进模型,没有"提议→人接受"这一步 |
| **INV-4** | **确定性核心**:core(本体+日志+提交+projection)内**无 LLM 调用、无随机、无网络** | core crate 里出现 LLM/HTTP 调用、`rand`、联网 |
| **INV-5** | **事件溯源**:state = fold(events);事件 append-only,永不改/删(用补偿事件);projection 永远可从日志重建 | 改写或删除历史事件;projection 改不回去/无法重建 |
| **INV-6** | **单一契约接口**:Rust core 对外只暴露一个 typed 契约边界,所有消费者(前端、未来的引擎/Blender/VFX 集成)都走它 | 前端或某集成绕过契约直接捅 core 内部 |
| **INV-7** | **渲染只在前端**:2D/3D/VFX 渲染**永不进 core** | core crate 里出现任何渲染/图形代码 |
| **INV-8** | **Hook/反应也只产提议**:自动反应同样走 提议→校验→落盘 管道,带 origin 标记 + 深度守卫保证收敛 | hook 直接改模型;或 hook 触发 hook 无限循环 |

**核心心智:core 是确定性状态机(typed 数据 + 事件日志 + projection),AI 和渲染都是它外面、可替换的层。** 把贵的、不可逆的判断留在确定性那侧。

---

## 2. 绝对禁止清单

无论看起来多顺手、多能让某功能"先跑起来",以下事情一律不做:

1. 在 core crate 里调 LLM / 联网 / 用随机数(违反 INV-4)。
2. 绕过事件日志写状态(违反 INV-2/5)。
3. 把 AI 提议自动 apply、跳过人 review(违反 INV-3)。
4. 改写/删除历史事件(违反 INV-5;要撤销就追加补偿事件)。
5. 让前端/集成绕过契约接口访问 core 内部(违反 INV-6)。
6. 把渲染/图形代码放进 core(违反 INV-7)。
7. **自建交互式 3D 关卡编辑器**——远期 3D 编辑外包给 Blender(见 TECH-SPEC §3),近期只做 Three.js 只读预览。
8. **自己写存储引擎**——持久化用 SQLite,你只写其上的事件溯源逻辑。
9. 引入 **GPL/AGPL** 代码进代码库——强 copyleft 只能当外部独立程序调用(见 §5)。
10. 在 Tauri webview 用 `localStorage`/`sessionStorage` 当持久层;不提交任何 key/secret 进仓库。
11. 为"还没设计的远期功能"提前选型/加重型依赖(过度设计)。

**如果某个功能看起来必须违反上面任何一条才能实现:停下,在 Kanban 卡上 `block` 并说明,不要硬做、不要悄悄违反。**

---

## 3. 不变量必须编码成测试(物理护栏)

因为人类不读代码,**红线不能只靠这份文档的君子协定,必须有测试在你违反时亮红灯。**

- **§1 每条不变量,至少有一个自动化测试,违反就 fail。**
- **实现任何碰到 状态/AI/事件/契约/渲染边界 的功能时,必须在同一次改动里新增或更新对应不变量测试。** 功能代码和它的护栏测试一起提交,不可分离。
- 这些测试进 CI(TeamCity),push 即跑。

不变量测试示例(照此思路写,具体实现自定):
- INV-2/5:`任意一次被接受的 mutation 之后,事件日志里必有对应事件`。
- INV-3:`apply 一个 AI 提议、在'接受'之前,模型状态不变`。
- INV-5:`从事件日志重建的 projection == 当前状态`(replay 一致性)。
- INV-4/7:`core crate 的依赖图里不含任何 LLM/HTTP/渲染 crate`(可用依赖检查/构建测试实现)。
- INV-8:`hook 产生的事件带 origin=hook 标记,且触发链有深度上限、必然终止`。

---

## 4. 技术栈(已冻结 · 细节见 TECH-SPEC.md)

- **core**:Rust。单一特权进程(即 Tauri 的 Rust 核心),对外暴露单一 typed 契约。
- **外壳**:Tauri 2.x,原生 only。
- **前端**:TypeScript + web。框架在第一次动手时选定 React **或** Svelte 之一并**从此固定**(它决定下面用 React Flow 还是 Svelte Flow)。
- **存储**:SQLite(`rusqlite` 或 `sqlx`)+ WAL。**手写**事件溯源逻辑(events 表 append-only + fold + projection 表)。**不自建存储引擎,不引入 `cqrs-es`**(除非将来 projection 查询/并发复杂到值得,届时另议)。
- **AI 接入**:`tauri-plugin-shell` sidecar + tokio spawn 外部 CLI agent;用 `--output-format json/stream-json` + JSON schema 契约,逐行解析 → 反序列化成 Rust typed proposal。**直接借鉴 open-design(nexu-io)** 的 PATH 检测 / spawn / 流解析 / 大 prompt 走 stdin / iframe 沙箱(见 TECH-SPEC §5)。
- **2D 节点图**(关卡拓扑):React Flow / Svelte Flow(MIT),与前端框架匹配。数据走事件溯源,图只是 projection 的可视层。
- **3D**:近期 Three.js 只读预览;远期交互式 3D 编辑**外包 Blender**(socket addon),Unity 当数据消费端;**永不自建交互式 3D 编辑器**。
- **发布**:`tauri build` + itch.io 的 `butler` 增量推送。

---

## 5. 许可证纪律(本项目开源,但要保持 license-clean)

每引入一个依赖,先过许可证:
- **MIT / Apache-2.0 / BSD**:自由用,保留版权声明。
- **弱 copyleft(LGPL/MPL)**:逐个评估用法,优先动态/独立。
- **强 copyleft(GPL/AGPL)**:**只能当外部独立程序调用**(如读其文件格式、socket 通信),**绝不进代码库**。Tiled 是 GPL → 只能读它的 TMX/JSON 格式或调独立进程。

---

## 6. 目录约定

```
/core        # Rust crate:确定性核心 + 事件溯源 + 单一契约接口(无 LLM/无渲染)
/src-tauri   # Tauri 外壳接线:sidecar 调 CLI、把契约暴露给前端
/ui          # 前端(TS + React/Svelte),所有渲染都在这
/tests       # 测试,含 /tests/invariants(§3 的护栏测试)
CLAUDE.md  SPEC-v1.md  TECH-SPEC.md  WORKFLOW.md   # 根目录契约文件
```
- **本仓库 = 工作台的代码**。用户用工作台创建的"游戏项目数据"是另一回事:一个项目 = 用户本地一个文件夹/库,与本仓库分离(见 SPEC-v1 §2 G5)。

---

## 7. 每次改动的自查清单(Definition of Done)

人类靠勾这个清单 + 看 CI + 跑验收 来验收你,**不靠读你的代码**。每次提交前自查:

- [ ] 只做了当前 SPEC 单元边界内的事(没碰 SPEC §4.2 的"不做"项)
- [ ] 没有违反 §1 任何一条红线
- [ ] 对应的**不变量测试**已新增/更新且通过(§3)
- [ ] 功能测试已写且通过
- [ ] 至少能编译;CI 负责跨平台/打包验证
- [ ] 新依赖的许可证已过(§5)
- [ ] 没有提交任何 key/secret
- [ ] commit 小而聚焦,对应一个单元/子任务
- [ ] PR 描述填了 handoff 模板(改了哪些文件 / 怎么验证的 / 残留风险)——见 WORKFLOW.md

---

## 8. 行为准则

- **一次一个单元,按 SPEC 顺序 U0→U5。** 不跳步、不并行铺开(并行是飞轮第三阶段的事,见 WORKFLOW)。
- **SPEC/TECH-SPEC 没覆盖的细节**:按"最小、可逆、不扩大范围"自行决定并在 commit/PR 里记一句。**不要就这类细节反复回头问人类。**
- **但架构级的岔路**(碰红线、碰核心设计、跨模块的不可逆决定):不要自己拍,`block` 卡片说明,等人类定。
- **永远不要为了让功能跑起来而悄悄违反不变量。** 看起来需要违反时,停 + flag。
- 完成一个单元 = 它的验收标准(SPEC)过了 + 不变量测试绿 + 自查清单全勾。
