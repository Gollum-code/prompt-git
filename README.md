# prompt-git

> **Prompt 也是代码——给它 git、diff、测试。**

把提示词当代码管理的版本控制工具：`git` 存储 prompt、**结构化 diff**（按块/变量/系统提示高亮）、**每次变更自动跑评测门禁**。让 prompt 迭代像代码一样有版本、有评审、有回归测试。

```
prompt-git init → 编辑 → commit → diff → test →（不行）checkout 回滚
```

## 为什么要做

- Prompt 迭代是 AI 团队最高频工作，但**没有版本管理**——改来改去不知道哪版好、回不去。
- 现有 "PromptHub / re_gent / vcprompt" 都是**个人资产整理**，没有「git 原生协作 + eval 门禁」的方案。
- 概念契合开发者直觉（git 世界观）：diff 引擎现成，容易做。
- 空白度最高：全网仅 ~358 个仓库，且多数是单文件收藏夹，不是协作 VCS。

## 功能（MVP）

| 命令 | 作用 |
|---|---|
| `prompt-git init` | 创建 `prompts/` 目录并纳入 git |
| `prompt-git commit -m` | 提交一次提示词变更（基于 git） |
| `prompt-git log` | 查看提示词版本历史 |
| `prompt-git checkout <version>` | 回滚到某个版本 |
| `prompt-git diff` | **结构化 diff**：按 system/user/vars/tests 分块、行内词级高亮 |
| `prompt-git render` | 渲染当前提示词（变量插值调试，不调用模型） |
| `prompt-git test` | **评测门禁**：变更后跑关联用例，返回通过率（可接 CI） |
| `prompt-git compare <v1> <v2>` | 相同输入下对比两个版本的输出 |

## 目录约定

```
prompts/
  system.md        # 系统提示
  user.md          # 用户提示模板（支持 {input} 等变量）
  vars.yaml        # 变量定义 / 默认值
  tests.yaml       # 关联测试用例 + 评测后端配置
```

## 快速开始

```bash
# 1. 安装（需要 Rust 工具链）
cargo install --path prompt-git

# 2. 初始化提示词项目
mkdir my-prompt && cd my-prompt
prompt-git init

# 3. 编辑 prompts/system.md 与 prompts/user.md
# 4. 提交第一版
prompt-git commit -m "初始化提示词"

# 5. 再次改动后，查看结构化 diff
prompt-git diff

# 6. 跑评测门禁（需要 API Key，见下）
prompt-git test

# 7. 不行就回滚
prompt-git checkout HEAD~1
```

### 本地零成本体验评测门禁

仓库内置了一个假 LLM 服务器，无需 API Key 即可看到完整的 `test` 流程：

```bash
python scripts/mock_llm.py &          # 起一个假的 chat/completions 服务 :18080
export OPENAI_API_KEY=dummy            # 只要非空即可
export PROMPT_GIT_BASE_URL=http://127.0.0.1:18080/v1
cd examples/customer-support
prompt-git test
```

## tests.yaml 配置

```yaml
backend:
  provider: openai          # openai | deepseek | custom（OpenAI 兼容）
  model: gpt-4o-mini
  temperature: 0.2
  max_tokens: 512
  base_url: ""              # custom 时必填；也可用环境变量 PROMPT_GIT_BASE_URL
  api_key_env: ""           # 自定义 key 环境变量名（留空按 provider 默认）

judge: keyword              # keyword（离线，不消耗 token）| llm（模型判定）
fail_under: 1.0             # 通过率门禁，低于该值 test 以非零码退出

cases:
  - name: 物流查询
    vars: { input: "我的订单发货了吗？", language: 中文 }
    expect_contains: [订单, 物流]        # 输出必须包含（大小写不敏感）
    expect_not_contains: [无法, 抱歉]    # 输出禁止包含
    # expect_exact: "..."               # 完全一致判定
    # min_length: 10 / max_length: 500  # 长度约束
    # judge_prompt: "..."               # judge: llm 时的判定指令
    # enabled: false                    # 跳过该用例
```

### API Key 解析

| provider | 默认环境变量 |
|---|---|
| `openai` | `OPENAI_API_KEY` |
| `deepseek` | `DEEPSEEK_API_KEY` |
| `custom` | `PROMPT_GIT_API_KEY`（或 `backend.api_key_env` 指定） |

统一覆盖：`PROMPT_GIT_MODEL`、`PROMPT_GIT_BASE_URL`（指向本地模型 / 代理时很有用）。

## diff 示例

```
━━━ system.md · 系统提示
文件规模: 6 → 7 行
  @@ -4,3 +4,4 @@
     4    4 │ - 始终使用{language}回答，语气保持{tone}。
     5    5 │ - 不确定的事情不要编造，先说明需要核实，并建议用户联系人工客服。
     6    6 │ - 回答控制在 150 字以内，先给结论再给步骤。
          7 │ +- 提供订单/物流信息时，主动给出查询入口链接：…
```

- 按 **块**（system / user / vars / tests）分组展示，而不是一整坨行 diff；
- 修改对自动做 **行内词级高亮**，一眼看出变量、措辞变了哪些词；
- diff 末尾提示「建议运行 `prompt-git test` 确认回归」。

## 技术架构

```
prompt-git CLI (Rust)
  ├─ store.rs     # git 接入：init / add / commit / log / checkout / show
  ├─ diff.rs      # Myers 行级 diff + 词级高亮，按块渲染
  ├─ template.rs  # 变量渲染 {name} / {{转义}}，vars.yaml 默认值
  ├─ eval.rs      # OpenAI 兼容 chat/completions 客户端 + keyword/llm judge
  ├─ test.rs      # 评测门禁：跑用例、通过率、fail_under 门禁（CI 友好）
  └─ compare.rs   # 双版本同输入对比 + 相似度
```

## 拿星策略（README 层面）

- 标签：`prompt` `prompt-engineering` `llm` `version-control` `cli`
- 卖点：「Prompt 也是代码——给它 git、diff、测试」
- 演示：一次 prompt 迭代的 diff + 回归测试通过
- 发布：r/LocalLLaMA、r/LLMDevs、中文 AI 社区

## Roadmap

| 阶段 | 交付 |
|---|---|
| M1 ✅ | git 接入 + commit/log/checkout + 结构化 diff |
| M2 ✅ | test 门禁（keyword / llm judge）+ 变量模板 |
| M3 | compare 双输出 + 团队 flow 文档 + tag 语义版本 |

## 风险与应对

- 团队协作习惯培养成本 → README 讲清「为什么 prompt 要版本管理」；`diff` 末尾主动提示 `test`
- 评测门禁要轻 → 默认 `keyword` 离线判定，不消耗 token；`llm` 判定按需开启
- 与真实 LLM 工作流贴合 → 支持 OpenAI / DeepSeek / 本地模型，模板渲染成真实请求

## License

MIT
