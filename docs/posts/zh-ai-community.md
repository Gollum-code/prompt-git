# 中文 AI 社区帖子草稿（V2EX / 掘金 / 思否 / 微信公众号）

> 标题（V2EX）：Prompt 也是代码——我给提示词写了个 git，带结构化 diff 和评测门禁
> 标题（掘金）：开源 | prompt-git：把提示词当代码管理（git 存储 + diff + 每次变更自动评测）

---

## 背景

做 LLM 应用的人每天迭代 prompt 十几次：改参数、改措辞、加 few-shot……
但绝大部分团队**没有提示词版本管理**——改来改去不知道哪版好、回不去，
更没人知道某次改动有没有把之前修好的问题改回去。

## 我做了什么

`prompt-git`：一个 Rust 写的小 CLI，把提示词当代码管。

```
prompts/
  system.md        # 系统提示
  user.md          # 用户提示模板（{var} 插值，vars.yaml 给默认值）
  vars.yaml        # 变量定义 / 默认值
  tests.yaml       # 回归用例 + 评测后端
```

底层就是 git，分支 / PR / 评审 / 回滚全部天然复用，团队不用学新工具。

## 三个核心能力

**1. 结构化 diff**
不是整坨行 diff，而是按 system / user / vars / tests 分块展示，
修改对自动做行内词级高亮——一眼看出「哪个变量、哪句措辞」变了。

**2. 评测门禁 `prompt-git test`**
渲染当前模板 + 跑 tests.yaml 里的回归用例，返回通过率。
默认 `keyword` 判定完全离线、不花 token；需要时切 `llm` 判定。
通过率低于阈值就以非零码退出，**可以直接进 CI / 合并前检查**。

**3. 双版本对比 + 语义化版本**
`compare v1 v2`：相同输入下两版输出并排对比 + 相似度；
`tag 1.0.0`：SemVer 规范版本，`checkout v1.2.1` 一键热回滚。

## 模型无关

OpenAI / DeepSeek / 本地模型都可以（只要暴露
`/v1/chat/completions`，通过 `PROMPT_GIT_BASE_URL` 指向本地服务）。
支持中文环境，报错提示和文档都是中文。

## 演示

> GIF（`docs/demo.gif`，一次完整迭代：init → commit → render → diff → test → tag）

```bash
git clone https://github.com/Gollum-code/prompt-git && cd prompt-git
cargo install --path prompt-git   # 或 cargo build --release

mkdir my-prompt && cd my-prompt
prompt-git init
prompt-git commit -m "初始化提示词"
# 改 prompts/system.md …
prompt-git diff
prompt-git test
prompt-git tag 1.0.0
```

仓库：https://github.com/Gollum-code/prompt-git
Release：https://github.com/Gollum-code/prompt-git/releases/tag/v0.1.0

## 想讨论

1. 你团队现在怎么管 prompt？Excel / Notion / 直接改代码？
2. 「评测门禁」轻到什么程度你才愿意用？keyword 判定够不够？
3. 有没有人愿意一起把它往协作（多人评审、用例共享）方向推？

欢迎 star / issue / PR。觉得方向有意思的，转给你们的 AI 团队。

---

发帖备注：
- V2EX 用「分享创造」节点；掘金选「开源」「LLM」标签。
- 正文带一张 GIF + 一张 tests.yaml 截图最有说服力。
- 中文明确定位（报错提示、文档全中文），切中中文团队需求。
