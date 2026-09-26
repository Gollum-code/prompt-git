# 团队协作 Flow（prompt-git × git flow）

prompt-git 基于 git，团队协作天然复用 git 的分支 / PR / 评审能力。
这份文档定义一套轻量的「提示词迭代」工作流：**改 → diff → 提交 → 门禁 → 评审 → 发布**。

## 分支约定

| 分支 | 用途 |
|---|---|
| `main` | 稳定版提示词，只接受通过评审 + 评测门禁的合并 |
| `feat/<name>` | 每个提示词改动一个分支（如 `feat/order-link`） |
| `fix/<name>` | 回归修复（如 `fix/toxic-output`） |

> 单一文件项目的提示词通常规模小，一条功能分支对应一次提交即可，不必分太细。

## 一次完整的提示词迭代

```bash
# 1. 从最新 main 拉分支
git checkout main && git pull
git checkout -b feat/order-link

# 2. 改提示词（system.md / user.md / vars.yaml / tests.yaml）
#    同时补/改 tests.yaml 里对应的用例
prompt-git diff          # 结构化看看改了什么，确认符合预期

# 3. 本地先过一遍门禁（CI 也会跑同样的命令）
prompt-git test

# 4. 提交并推送
prompt-git commit -m "feat: 订单查询返回中给出查询入口链接"
git push -u origin feat/order-link

# 5. 开 PR 到 main（GitHub PR）
#    评审人关注：diff 是否合理、tests.yaml 用例是否覆盖改动、test 是否通过
```

## PR 评审清单（评审人）

- [ ] `prompt-git diff` 的变更范围是否如描述所说
- [ ] 新加的测试用例是否覆盖这次改动的行为
- [ ] 是否触碰了 `vars.yaml` 变量定义（会导致其他用例受影响）
- [ ] CI 的 `lint-test` 与 `eval-gate` 是否通过

## 回归 / 回滚

```bash
# 线上提示词出问题 → 立即回滚到上一个稳定 tag
prompt-git tag               # 看有哪些稳定版本
prompt-git checkout v1.2.1   # 恢复该版本的 prompts/ 到工作区
prompt-git commit -m "revert: 回滚到 v1.2.1"
git push
```

## 版本发布（语义化版本）

```bash
# 通过评审合并进 main 后：
prompt-git test                  # 最后确认一次门禁
prompt-git tag 1.3.0             # 自动规范为 v1.3.0（annotated tag）
git push --tags
# 然后基于 tag 创建 GitHub Release，填写变更说明
```

版本规则（SemVer）：
- `MAJOR`：破坏性变更（例如改变输出格式约定、换底层模型策略）
- `MINOR`：新增行为（新规则、新变量、新用例维度）
- `PATCH`：回归修复、措辞调整

## 评测门禁进 CI

仓库自带 `.github/workflows/ci.yml`：

- `lint-test`：三平台 `cargo fmt / clippy / test`
- `eval-gate`：用本地假 LLM（`scripts/mock_llm.py`）跑 `prompt-git test`，验证链路可用

真实模型门禁建议在 PR 合并前由维护者在本地执行一次：

```bash
# 本地接真实 key 跑完整门禁
export OPENAI_API_KEY=sk-...
prompt-git test
```

## 常见问题

| 问题 | 做法 |
|---|---|
| 改了 system.md 忘了补用例 | `test` 跑出 0/0 或报错时补上 |
| 两个分支同时改了同一段提示词 | git 合并冲突照常处理；先解冲突再过门禁 |
| 想对比两个候选方案 | `prompt-git compare <候选A> <候选B>` |
| tag 打错了 | `prompt-git tag -d v1.0.0` 删除后重打 |
