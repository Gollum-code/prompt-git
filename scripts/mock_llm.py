"""假 LLM 服务器：用于零成本体验 `prompt-git test` / `compare`。

用法：
    python scripts/mock_llm.py            # 监听 127.0.0.1:18080
    export OPENAI_API_KEY=dummy
    export PROMPT_GIT_BASE_URL=http://127.0.0.1:18080/v1
    prompt-git test

按 user 消息内容返回固定的回答，用于验证关键词判定逻辑。
"""

import json
from http.server import BaseHTTPRequestHandler, HTTPServer


class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        n = int(self.headers.get("Content-Length", 0))
        body = json.loads(self.rfile.read(n).decode("utf-8"))
        user = ""
        for m in body.get("messages", []):
            if m.get("role") == "user":
                user += m.get("content", "")

        if "订单号" in user:
            text = "您可以在 App 的“我的 - 全部订单”中查看订单号。路径：我的 > 全部订单。"
        elif "发货" in user:
            text = "您的订单 888666 正在运输中，物流单号 SF1234567890。"
        else:
            text = "我无法从描述中直接确认，建议联系人工客服进一步核实。"

        resp = {
            "id": "chatcmpl-mock",
            "choices": [{"index": 0, "message": {"role": "assistant", "content": text}}],
            "usage": {"total_tokens": 12},
        }
        data = json.dumps(resp).encode("utf-8")
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def log_message(self, *args):
        pass


if __name__ == "__main__":
    print("mock-llm listening on 127.0.0.1:18080")
    HTTPServer(("127.0.0.1", 18080), Handler).serve_forever()
