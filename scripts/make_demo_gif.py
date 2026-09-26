"""生成 README 演示 GIF：真实执行 prompt-git 命令，解析 ANSI 颜色，渲染打字动画。

用法：
    python scripts/make_demo_gif.py                 # 输出 docs/demo.gif
    python scripts/make_demo_gif.py --width 96      # 自定义列数

依赖：pillow（pip install pillow）
"""

import argparse
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

REPO_ROOT = Path(__file__).resolve().parents[1]
BIN = REPO_ROOT / "target" / "release" / "prompt-git.exe"
MOCK = REPO_ROOT / "scripts" / "mock_llm.py"
OUT = REPO_ROOT / "docs" / "demo.gif"

FONT_ASCII = r"C:\Windows\Fonts\consola.ttf"
FONT_ASCII_BOLD = r"C:\Windows\Fonts\consolab.ttf"
FONT_CJK = r"C:\Windows\Fonts\msyh.ttc"
FONT_CJK_BOLD = r"C:\Windows\Fonts\msyhbd.ttc"

# ANSI 16 色（标准 xterm 取值）
PALETTE = {
    "black": (0, 0, 0),
    "red": (197, 15, 31),
    "green": (19, 161, 14),
    "yellow": (193, 156, 0),
    "blue": (0, 55, 218),
    "magenta": (136, 23, 152),
    "cyan": (58, 150, 221),
    "white": (204, 204, 204),
}
BRIGHT = {k: tuple(min(255, c + 100) for c in v) for k, v in PALETTE.items()}
BRIGHT["black"] = (128, 128, 128)
BRIGHT["blue"] = (92, 92, 255)

BG = (10, 14, 20)
TITLE_BG = (30, 34, 42)
FG_DEFAULT = (204, 204, 204)
DIM = (110, 118, 129)

ANSI_NAMES = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"]


def parse_ansi(text: str):
    """把含 SGR 转义串解析成 [(fg, bg, bold, dim, underline, char)]。"""
    fg = None
    bg = None
    bold = dim = underline = False
    segs = []
    for part in text.split("\x1b["):
        if not part:
            continue
        if "m" not in part:
            segs.append((fg, bg, bold, dim, underline, part))
            continue
        params, rest = part.split("m", 1)
        for p in params.split(";"):
            if not p:
                continue
            try:
                n = int(p)
            except ValueError:
                continue
            if n == 0:
                fg = bg = None
                bold = dim = underline = False
            elif n == 1:
                bold = True
            elif n == 2:
                dim = True
            elif n == 4:
                underline = True
            elif n == 22:
                bold = dim = False
            elif n == 24:
                underline = False
            elif 30 <= n <= 37:
                fg = PALETTE[ANSI_NAMES[n - 30]]
            elif 40 <= n <= 47:
                bg = PALETTE[ANSI_NAMES[n - 40]]
            elif 90 <= n <= 97:
                fg = BRIGHT[ANSI_NAMES[n - 90]]
            elif 100 <= n <= 107:
                bg = BRIGHT[ANSI_NAMES[n - 100]]
            elif n == 39:
                fg = None
            elif n == 49:
                bg = None
        if rest:
            segs.append((fg, bg, bold, dim, underline, rest))
    return segs


def is_wide(ch: str) -> bool:
    return ord(ch) > 0x2E7F  # CJK / 全角字符按 2 格


class TermFonts:
    def __init__(self, size: int):
        self.size = size
        self.ascii = ImageFont.truetype(FONT_ASCII, size)
        self.ascii_b = ImageFont.truetype(FONT_ASCII_BOLD, size)
        self.cjk = ImageFont.truetype(FONT_CJK, size)
        self.cjk_b = ImageFont.truetype(FONT_CJK_BOLD, size)
        asc, desc = self.ascii.getmetrics()
        self.cell_w = round(self.ascii.getlength("M"))
        self.line_h = asc + desc + 4
        self.title_font = ImageFont.truetype(FONT_ASCII, 12)


def font_for(f: TermFonts, ch: str, bold: bool):
    if is_wide(ch):
        return f.cjk_b if bold else f.cjk
    return f.ascii_b if bold else f.ascii


class Session:
    """记录一次终端会话：命令 + 真实输出。"""

    def __init__(self):
        self.steps = []  # [(command, result_text, is_error)]

    def run(self, command: str, cwd: Path, env: dict | None = None):
        e = os.environ.copy()
        if env:
            e.update(env)
        p = subprocess.run(
            command, cwd=cwd, shell=True, capture_output=True, text=True,
            encoding="utf-8", errors="replace", env=e, timeout=180,
        )
        out = p.stdout
        if p.stderr:
            out = p.stderr + out
        self.steps.append((command, out.rstrip("\n"), p.returncode != 0))


def build_session(verbose=False) -> tuple[Session, Path, dict]:
    tmp = Path(tempfile.mkdtemp(prefix="pg-demo-"))
    sess = Session()
    mock_env = {"OPENAI_API_KEY": "dummy",
                "PROMPT_GIT_BASE_URL": "http://127.0.0.1:18080/v1"}
    mock = None

    # 在 PATH 里放一个 prompt-git shim，让演示命令干净简短
    shim_dir = tmp / "bin"
    shim_dir.mkdir()
    (shim_dir / "prompt-git.cmd").write_text(
        f'@echo off\r\n"{BIN}" %*\r\n', encoding="ascii")
    env = os.environ.copy()
    env["PATH"] = str(shim_dir) + os.pathsep + env.get("PATH", "")

    def g(args):
        subprocess.run(["git", *args], cwd=tmp, check=True,
                       capture_output=True, text=True, encoding="utf-8",
                       errors="replace")

    g(["init", "-q"])
    g(["config", "core.autocrlf", "false"])
    g(["config", "user.email", "demo@example.com"])
    g(["config", "user.name", "prompt-git"])

    try:
        sess.run("prompt-git init", tmp, env)

        # 用更贴近真实的产品提示词覆盖 init 默认模板
        example_prompts = REPO_ROOT / "examples" / "customer-support" / "prompts"
        for f in example_prompts.iterdir():
            if f.is_file():
                shutil.copy(f, tmp / "prompts" / f.name)

        # 回退到「v1」基线：示例里已含后续改动，这里先移除，
        # 让后面的 diff 展示一次干净的「+1 规则 +1 章节」迭代
        sys_p = tmp / "prompts" / "system.md"
        kept = [l for l in sys_p.read_text(encoding="utf-8").splitlines(True)
                if "xinghe.example/order" not in l]
        sys_p.write_text("".join(kept), encoding="utf-8")
        usr_p = tmp / "prompts" / "user.md"
        kept = [l for l in usr_p.read_text(encoding="utf-8").splitlines(True)
                if "3. 相关链接" not in l]
        usr_p.write_text("".join(kept), encoding="utf-8")

        sess.run('prompt-git commit -m "初始化客服提示词 v1"', tmp, env)
        sess.run('prompt-git render --input "我的订单发货了吗？"', tmp, env)

        # 模拟一次提示词迭代：system 加一条规则 + user 加输出章节
        prompts = tmp / "prompts"
        system = prompts / "system.md"
        text = system.read_text(encoding="utf-8")
        text += "- 提供订单/物流信息时，主动给出查询入口链接：https://www.xinghe.example/order\n"
        system.write_text(text, encoding="utf-8")

        user = prompts / "user.md"
        text = user.read_text(encoding="utf-8")
        text += "3. 相关链接（如有）\n"
        user.write_text(text, encoding="utf-8")

        sess.run("prompt-git diff", tmp, env)

        # 起假 LLM 跑评测门禁
        mock = subprocess.Popen([sys.executable, str(MOCK)], stdout=subprocess.DEVNULL,
                                stderr=subprocess.DEVNULL)
        for _ in range(15):
            if mock.poll() is not None:
                break
            try:
                subprocess.run(
                    ["python", "-c",
                     "import urllib.request,json;"
                     "urllib.request.urlopen(urllib.request.Request("
                     "'http://127.0.0.1:18080/v1/chat/completions',"
                     "data=json.dumps({}).encode(),method='POST'))"],
                    timeout=5, capture_output=True)
                break
            except Exception:
                time.sleep(0.5)

        sess.run("prompt-git test", tmp, env=mock_env | env)
        sess.run("prompt-git tag 1.0.0", tmp, env)
        sess.run("prompt-git log --oneline", tmp, env)
    finally:
        if mock is not None:
            mock.kill()

    if verbose:
        for cmd, out, err in sess.steps:
            print(f"$ {cmd}")
            print(out)
            print("-" * 60)
    return sess, tmp, env


class GifRenderer:
    def __init__(self, cols: int, rows: int, font_size: int):
        self.cols = cols
        self.rows = rows
        self.f = TermFonts(font_size)
        self.header_h = 30
        self.width = cols * self.f.cell_w
        self.height = self.header_h + rows * self.f.line_h
        self.title = "prompt-git demo — Prompt 也是代码，给它 git＋diff＋测试"

    def new_image(self):
        return Image.new("RGB", (self.width, self.height), BG)

    def draw_header(self, img):
        d = ImageDraw.Draw(img)
        d.rectangle([0, 0, self.width, self.header_h], fill=TITLE_BG)
        for i, c in enumerate([(255, 95, 86), (255, 189, 46), (39, 201, 63)]):
            d.ellipse([12 + i * 20, 11, 12 + i * 20 + 8, 19], fill=c)
        d.text((70, 8), self.title, font=self.f.title_font, fill=(160, 168, 180))

    def draw_line(self, img, row_y, segs, prompt_char=None, cursor_col=-1):
        """segs: [(fg,bg,bold,dim,underline,text)]。返回该行占用格数。"""
        d = ImageDraw.Draw(img)
        x = 0
        col = 0
        for fg, bg, bold, dim, underline, text in segs:
            for ch in text:
                if prompt_char is not None and col == cursor_col:
                    self._draw_cursor(d, x, row_y, 2)
                cell_w = self.f.cell_w if not is_wide(ch) else self.f.cell_w * 2
                if bg is not None:
                    d.rectangle([x, row_y, x + cell_w - 1, row_y + self.f.line_h - 1],
                                fill=bg)
                color = fg or FG_DEFAULT
                if dim and fg is None:
                    color = DIM
                elif dim and fg is not None:
                    color = tuple(int(c * 0.55) for c in fg)
                font = font_for(self.f, ch, bold)
                # 让宽字符与 ASCII 基线一致
                baseline = (self.f.line_h - self.f.ascii.getmetrics()[0] -
                            self.f.ascii.getmetrics()[1]) // 2 + 1
                d.text((x, row_y + baseline), ch, font=font, fill=color)
                if underline:
                    y = row_y + self.f.line_h - 4
                    d.line([x, y, x + cell_w - 1, y], fill=color, width=1)
                x += cell_w
                col += 1 if not is_wide(ch) else 2
        if prompt_char is not None and cursor_col >= col:
            self._draw_cursor(d, x, row_y, 2)
        return col

    def _draw_cursor(self, d, x, y, w):
        d.rectangle([x, y, x + w * self.f.cell_w - 1, y + self.f.line_h - 1],
                    fill=(233, 235, 238))

    def frame(self, view_lines):
        """view_lines: 至多 rows 行（每行是 segs 列表）。"""
        img = self.new_image()
        self.draw_header(img)
        start = max(0, len(view_lines) - self.rows)
        visible = view_lines[start:]
        for i, segs in enumerate(visible):
            self.draw_line(img, self.header_h + i * self.f.line_h, segs)
        return img


def parse_cmd_line(command: str, cursor: bool = False) -> list:
    """命令行按真实输入渲染：提示符 + 命令（浅色），可选末端方块光标。"""
    prompt = (None, None, True, False, False, "$ ")
    cmd = (None, None, True, False, False, command)
    segs = [prompt, cmd]
    if cursor:
        segs.append((FG_DEFAULT, None, False, False, False, "\u2588"))
    return segs


def main():
    try:
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    except Exception:
        pass
    ap = argparse.ArgumentParser()
    ap.add_argument("--width", type=int, default=96)
    ap.add_argument("--rows", type=int, default=20)
    ap.add_argument("--font-size", type=int, default=13)
    ap.add_argument("--type-ms", type=int, default=26)
    ap.add_argument("--type-step", type=int, default=2)
    ap.add_argument("--hold-ms", type=int, default=900)
    ap.add_argument("--colors", type=int, default=128)
    ap.add_argument("--verbose", action="store_true")
    ap.add_argument("--stills", type=str, default="",
                    help="额外把所有关键帧拼成 PNG 写到这里（目检用）")
    args = ap.parse_args()

    sess, tmp, _ = build_session(verbose=args.verbose)
    r = GifRenderer(args.width, args.rows, args.font_size)

    transcript: list[list] = []  # 每行是 segs 列表
    frames: list[Image.Image] = []
    delays: list[int] = []

    def emit_frames(image, ms):
        frames.append(image)
        delays.append(ms)

    for command, output, is_error in sess.steps:
        # 打字动画（每次 --type-step 个字符）
        typed = ""
        i = 0
        while i < len(command):
            typed += command[i:i + args.type_step]
            i += args.type_step
            line = parse_cmd_line(typed, cursor=True)
            img = r.frame(transcript + [line])
            emit_frames(img, args.type_ms)
        # 回车
        img = r.frame(transcript + [parse_cmd_line(command)])
        emit_frames(img, 120)
        # 输出：单帧长停留（GIF 单帧最长 65s，无需多帧重复）
        transcript.append(parse_cmd_line(command))
        for line in (output.split("\n") if output else ["（无输出）"]):
            transcript.append(parse_ansi(line))
        hold = args.hold_ms if not is_error else args.hold_ms + 400
        emit_frames(r.frame(transcript), hold)

    # 收尾光标
    emit_frames(r.frame(transcript), 400)

    # 目检用：把每个 hold 帧拼成一张横向 PNG
    if args.stills:
        holds = [frames[i] for i in range(len(frames)) if delays[i] >= 500]
        w = r.width
        h = r.height
        sheet = Image.new("RGB", (w * len(holds), h), (20, 20, 20))
        for idx, im in enumerate(holds):
            sheet.paste(im, (idx * w, 0))
        Path(args.stills).parent.mkdir(parents=True, exist_ok=True)
        sheet.save(args.stills)
        print(f"关键帧拼接: {args.stills}  ({len(holds)} 帧)")

    OUT.parent.mkdir(exist_ok=True)
    # 全局调色板 + 关闭抖动：GIF 体积可降一个数量级（抖动噪声极难压缩）
    master = frames[len(frames) // 2].quantize(
        colors=args.colors, method=Image.Quantize.MEDIANCUT
    )
    qframes = [
        f.quantize(palette=master, dither=Image.Dither.NONE) for f in frames
    ]
    qframes[0].save(
        OUT,
        save_all=True,
        append_images=qframes[1:],
        duration=delays,
        loop=0,
        optimize=True,
        disposal=2,
    )
    size_kb = OUT.stat().st_size / 1024
    print(f"写出 {OUT}  {len(frames)} 帧  {size_kb:.0f} KB  "
          f"({r.width}x{r.height})")
    shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    main()
