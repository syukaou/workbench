# WORKFLOW.md — 〈WORKBENCH〉 开发飞轮

> 本项目的第一产出物不是工作台,是**验证"你这套 人 + 工具 的开发循环能不能自己转起来"**。工作台是飞轮的第一个负载测试件。
> 工具链:Hermes Kanban(任务源 + 派发)· Claude Code / OpenCode(实现)· git worktree · GitHub(代码真源 + PR 评审闸)· GitHub Actions(CI/构建/测试/打包)· itch.io(发布)。

---

## 1. 飞轮长什么样

```
 Hermes Kanban ──卡片──► worker(Claude Code/OpenCode) ──代码──► git worktree ──► GitHub PR
   (要做什么)              (vibe coding 实现)                              │
       ▲                                                                  ▼
       │                                                          GitHub Actions CI
       └──── 你:验收 / unblock / 建新卡 ◄──── 构建+不变量测试+打包 ◄──── push 触发
```

| 工具 | 角色 | 你熟不熟 |
|---|---|---|
| **Hermes Kanban** | 任务源 + 派发 + 持久状态 + human-in-the-loop + 审计(`~/.hermes/kanban.db`) | 要学 |
| **Claude Code / OpenCode** | worker:把卡片实现成代码 | 熟 |
| **git worktree** | 每张编码卡一个隔离工作区(Kanban 原生支持 `worktree:` 工作区) | 熟 |
| **GitHub** | 代码唯一真源 + PR 评审闸(你的人工验收点) | 熟 |
| **GitHub Actions** | CI:push → 构建 + 跑测试(含不变量测试)+ 打包 | 要学 |
| **itch.io** | 发布(`butler` 增量推送) | 要学 |

**飞轮的"飞"不在工具多,在于一张卡能否无人工搬运地从 Kanban 流到可运行/可发布产物。** 中间任何一步要你手动复制/手动跑/手动传,飞轮就卡住。**飞轮验证的本质 = 把这条链上的人工搬运降到只剩"决策"和"验收"两件事。**

---

## 2. 飞轮分三档点亮(别一次全接通)

你同时在验证三件没验证过的事——纯 vibe coding 靠不靠谱、飞轮转不转、多模型分层划不划算。**三件一起上会无法归因**(产出烂了分不清是谁的锅)。所以一次只新增一个不熟的变量:

| 档 | 闭合到哪 | 这一档验证什么 | Kanban / 模型怎么配 |
|---|---|---|---|
| **v0** | Kanban + Claude Code + 本地构建。CI 先不急着上 | **纯 vibe coding + CLAUDE.md 护栏本身靠不靠谱**(最核心的未知数) | 见 §3 第一阶段 |
| **v1** | 加 GitHub Actions:push → 构建 + 跑**不变量测试** + 出可执行包 | 自动化链路 + **质量闸能不能自动拦住跑偏** | 同上 + handoff 证据(§4) |
| **v2** | 加 Kanban 自动分解/并行 + 自动发 itch.io | 完整链路从"要做什么"一路自动到"用户能下载" | 见 §3 第三阶段 |

**先点 v0**:最大的未知数是"纯 vibe coding 到底靠不靠谱",这个不验证,后面接再多自动化都是建在流沙上。靠谱了再 v1 上 CI 把护栏和打包自动化,最后 v2 接通两端。

---

## 3. Hermes Kanban 怎么用(关键:用它的骨,分阶段开它的编排魔法)

Hermes Kanban 远不止看板 UI——它是 dispatcher + 持久队列 + 状态机,worker 是有身份的完整 OS 进程,通过 `kanban_*` 工具读写板子,原生支持 git worktree 工作区、human-in-the-loop、审计。**但它也自带"自动分解 + 多 agent 并行 + swarm"能力,那恰好是我们否决掉的"编排式多 agent"。所以按阶段开启:**

### 第一阶段(对应飞轮 v0/v1)——把它当"带持久状态和 human-in-the-loop 的任务队列",不是自动编排器

配置:
```yaml
kanban:
  auto_decompose: false        # 关掉自动分解魔法。你来当 orchestrator
  max_in_progress: 1           # 串行:一次只跑一张卡,先验证单 worker 质量
```
- **单个 worker profile**,后面跑 **单个 SOTA**(Claude Code,模型 = Claude 或 Codex)。
- **你**手动建卡(`hermes kanban create`)、决定拆什么、什么顺序、验收谁。**你是 orchestrator,Kanban 提供的是基础设施价值**(持久化、worktree 隔离、审计、手机上 `/kanban unblock`),不是它的自动分解。
- 编码卡用 worktree 工作区(完成后保留):
  ```
  hermes kanban create "实现 U1:事件日志 append + fold" \
      --assignee dev --workspace worktree --branch u1-event-log
  ```
- worker 卡住 → `block`;你 `comment` 给上下文、`unblock`;它下次 `kanban_show()` 会读到。

### 第二阶段(对应飞轮 v1)——加质量闸

- 同上,但 worker 完成时按 handoff 约定交证据(§4),GitHub Actions 在 PR 上跑不变量测试。
- 此时**物理护栏(不变量测试)就位**——这是放并行/小模型进来的前提。

### 第三阶段(对应飞轮 v2)——才开并行与小模型分层

只有当不变量测试这道闸已就位、且单 SOTA 的质量基准已建立,才安全地开:
- `auto_decompose: true` / swarm / 多 profile。
- **模型分层**:架构敏感卡(碰红线/核心设计/跨模块)→ SOTA(Claude/Codex);架构无关卡(UI 组件、CRUD 样板、测试、文档、glue)→ 小模型(DeepSeek/OpenCode,或你的 Gemini/Grok CLI)。两者**不实时互相指挥**,各领自己性质的卡,都对着同一份 CLAUDE.md 和同一套不变量测试。
- **模型只是 worker 背后可换的引擎,换谁是配置问题、不是架构问题。** 用不变量测试当客观裁判选哪个小模型最划算。

> ⚠️ **无法归因警告**:不要在第一阶段就开自动分解/并行/多模型。那会在你看不见代码的基础上,再叠"任务拆得对不对""并行 worker 有没有跑偏"两层看不见的委托。先单 SOTA 把"vibe coding 靠不靠谱"这一个变量单独验证。

---

## 4. Handoff 证据约定(worker 完成卡时必须留)

worker `kanban_complete` 时,`summary` 是人读的收尾,`metadata` 是机器可读的交接。工程/评审卡按这个形状留(键是约定不是强制 schema):
```json
{
  "changed_files": ["core/src/log.rs", "core/src/invariant_tests.rs"],
  "verification": ["cargo test --workspace"],
  "dependencies": ["父卡 id 或外部 issue"],
  "residual_risk": ["没测到的 / 仍需人类复核的"]
}
```
让下一个读者(人或下游 worker)能快速答四问:改了什么 / 怎么验证的 / 失败了怎么 unblock 或重试 / 还故意留了什么风险。**secret/token/原始日志不要进 metadata,放指针和摘要。**

---

## 5. 人类怎么不读代码也能验收(这是 vibe coding 的核心动作)

你的两件事只有**决策**和**验收**。验收不靠读每一行,靠四样东西叠加:

1. **CI 里不变量测试全绿**(GitHub Actions)——红线没被违反的物理证据。
2. **SPEC 单元的验收 checklist 过了**(SPEC-v1 §5.7)——功能做没做到。
3. **PR 的 handoff metadata**(§4)——改了什么、怎么验的、残留风险。
4. **跑一遍 app,对照该单元的"完成标准"**(SPEC-v1)——亲眼确认闭环。

四样都过 → 合 PR。任何一样不过 → 在卡上 `comment` 退回,worker 下轮读到再改。**你永远不需要逐行读 diff。**

---

## 6. 安全纪律(写死,别踩)

- **Hermes Kanban 是单机的**(`~/.hermes/kanban.db` 是本地 SQLite,worker spawn 在同机)。跨主机共享板不支持;你的多机 homelab 别指望用它跨机协作。
- **dashboard 默认不鉴权、绑 localhost**。**绝不在共享主机上 `hermes dashboard --host 0.0.0.0`**——那会把整个协作面(任务体、评论、工作区路径)暴露到网络,且任何人能建/改/归档卡。
- **第一阶段你是 orchestrator,不是自动分解器**——别因为 auto_decompose 诱人就提前开。
- worker 长任务要定时 `kanban_heartbeat`(>1 小时至少一次),否则 dispatcher 会当它崩了回收(丢当前进度,但不算失败)。

---

## 7. 一句话总览

**Hermes Kanban 当任务主导(第一阶段用骨、关编排魔法,你当 orchestrator)→ 单 SOTA worker 在 worktree 里 vibe coding → PR 上 GitHub Actions 跑不变量测试当物理闸 → 你靠测试+checklist+handoff+试跑验收,不读代码 → 飞轮三档点亮,第三阶段才放并行和小模型分层。**
