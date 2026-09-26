# r/LLMDevs 帖子草稿

> 标题：**We treat prompt changes like code diffs now — open-sourced the CLI (git + eval gate)**

---

We keep a lot of production prompts across our LLM apps, and the biggest pain
was: every tweak was un-reviewable, and nothing proved a change didn't break
earlier behavior.

So I built **`prompt-git`**, a small CLI that gives prompts a git-native
workflow:

- `prompts/` holds `system.md`, `user.md`, `vars.yaml` (`{var}` templating with
  defaults) and `tests.yaml` (cases + eval backend). All plain files, all in git.
- **`prompt-git diff`** — structured, per-block diff with word-level highlight.
  "which system line / variable changed" is a one-glance answer.
- **`prompt-git test`** — renders the prompt and runs your regression cases
  against any OpenAI-compatible endpoint. Free offline keyword judge by
  default; opt-in LLM judge. Fails with a non-zero exit so you can wire it into
  CI / pre-merge hooks.
- **`prompt-git compare v1 v2`** — same input through two versions, side-by-side
  outputs + a similarity read.
- **`prompt-git tag 1.0.0`** — SemVer for prompt versions; `checkout v1.2.1` to
  hot-rollback.

Model-agnostic: OpenAI, DeepSeek, or local/hosted models via
`PROMPT_GIT_BASE_URL` — no provider lock-in.

Demo GIF of a real iteration (diff + green eval gate + release tag):
**GIF**（上传到附件：repo 内 `docs/demo.gif`）

Quick start:

```bash
prompt-git init
prompt-git commit -m "initial prompts"
# tweak prompts/system.md + add a case to prompts/tests.yaml
prompt-git diff
prompt-git test
prompt-git tag 1.0.0
```

CI: the repo's workflow runs fmt/clippy/test plus a mock-LLM eval-gate self
check, so the gate itself is verified on every push.

Repo: https://github.com/Gollum-code/prompt-git
Release v0.1.0: https://github.com/Gollum-code/prompt-git/releases/tag/v0.1.0

I mainly want feedback from folks shipping LLM features: where does a
keyword/LLM eval gate break down in practice, and what assertion types are you
missing (regex? semantic similarity? LLM-as-judge rubrics)?

---

发帖备注：
- 强调「非 provider 锁定」+「回归测试」，这是 LLMDevs 关心点。
- 结尾用具体问题收尾，引导讨论而非纯推广。
- 附 release 链接显得成熟、可追溯。
