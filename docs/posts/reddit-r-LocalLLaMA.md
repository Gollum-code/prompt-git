# r/LocalLLaMA 帖子草稿

> 标题：**Prompt is code. I built it a git — with structured diff + automated eval gate**

---

Hey folks,

Most of us iterate prompts 10+ times a day and have zero version history. I
finally did something about it: a small CLI that treats prompts like code.

**`prompt-git`** — prompts live in `prompts/` (system.md / user.md / vars.yaml /
tests.yaml), everything is plain git under the hood, so branches, PRs, review
and rollback all work like you already know.

What makes it different from a prompt library / notebook:

- **Structured diff** — not line soup. Per-block (system / user / vars / tests)
  with intra-line word highlighting, so you instantly see which variable or
  wording changed between versions.
- **Eval gate** — `prompt-git test` renders the template + runs your cases
  against any OpenAI-compatible model (local models included via
  `PROMPT_GIT_BASE_URL`). Keyword judging is offline & free; LLM judging is
  opt-in. Pass-rate below a threshold → non-zero exit, so it can gate a CI job.
- **Compare** — same input, two versions, side-by-side outputs + similarity.
- **SemVer tags** — `prompt-git tag 1.0.0`.

Works with OpenAI / DeepSeek / anything exposing `/v1/chat/completions`, so
your local llama.cpp / Ollama models fit right in.

Demo (iteration → diff → eval gate → tag):

**GIF**（建议直接上传到帖子附件：repo 内 `docs/demo.gif`）

```bash
cargo install --path prompt-git   # or build from source
prompt-git init
prompt-git commit -m "v1"
# edit prompts/system.md ...
prompt-git diff
prompt-git test
prompt-git tag 1.0.0
```

Repo: https://github.com/Gollum-code/prompt-git

Would love feedback on the eval-gate ergonomics and diff granularity. If you
work on a team that maintains shared prompts, does a git-native + eval-gate
flow actually fit your workflow?

---

发帖备注：
- 先看子版规是否允许工具帖（一般允许，用 ShowHN/工具 flair 更稳）。
- GIF 直接作为帖子媒体上传（Reddit 不支持外链 hotlink GitHub 图）。
- 保留「本地模型可用」卖点，这是 r/LocalLLaMA 最关心的点。
