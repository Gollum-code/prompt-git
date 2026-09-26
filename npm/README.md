# prompt-git

> **Prompt 也是代码——给它 git、diff、测试。**

把提示词当代码管理的版本控制工具：git 存储 + **结构化 diff**（按块/词级高亮）+
**每次变更自动跑评测门禁**。安装即得预编译二进制（win / mac-arm / linux），无需编译。

## 安装

```bash
npm install -g prompt-git
```

或本地安装后整目录使用：postinstall 会自动按平台下载 GitHub Release 的二进制。

```bash
prompt-git init
prompt-git commit -m "初始化提示词"
prompt-git diff          # 结构化 diff
prompt-git test          # 评测门禁（需 API Key，详见下）
prompt-git tag 1.0.0     # 语义化版本
```

## 目录约定

```
prompts/
  system.md        # 系统提示
  user.md          # 用户提示模板（{var} 插值）
  vars.yaml        # 变量定义/默认值
  tests.yaml       # 关联测试用例 + 评测后端配置
```

## 评测门禁

`tests.yaml` 配置 provider（openai / deepseek / 本地模型）+ 判例，`prompt-git test`
返回通过率，低于 `fail_under` 以非零码退出（可进 CI）。

```bash
export OPENAI_API_KEY=sk-...     # 或 DEEPSEEK_API_KEY / PROMPT_GIT_API_KEY
export PROMPT_GIT_BASE_URL=...   # 本地模型时指向 /v1/chat/completions
prompt-git test
```

## 完整文档

- GitHub 仓库（源码 / demo GIF / 团队协作 flow）：https://github.com/Gollum-code/prompt-git
- 说明与配置示例：见仓库 README 与 `docs/team-flow.md`

## License

MIT