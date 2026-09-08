#!/usr/bin/env python3
"""对已核对的局面摘要做解释实验；每条请求独立、无工具、无自动重试。"""

import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
import hashlib
import json
from pathlib import Path
import time
from urllib.error import HTTPError, URLError
from urllib.parse import urlparse
from urllib.request import HTTPRedirectHandler, Request, build_opener


SYSTEM = """你是日麻复盘助手。根据给定事实，用简明中文直接回答问题，可以使用Markdown。
先比较相关选择的收益和代价，再说明已提供的Mortal偏好。不要替推荐寻找必然理由。
区分已知事实、条件推论与尚不能判断的内容；缺少信息时具体说明，但仍回答证据足够的部分。
不要自行补造牌河、暗牌、数值或对手未来动作。无需固定格式或引用标签。"""


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


def save(path, value):
    temporary = path.with_suffix(".tmp")
    temporary.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")
    temporary.replace(path)


def topics(path):
    result = {}
    for section in path.read_text().split("\n## ")[1:]:
        name = section.split("：", 1)[0]
        if name in result:
            raise ValueError(f"重复规则主题：{name}")
        result[name] = section.strip()
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("cases", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--models", nargs="+")
    parser.add_argument("--conditions", nargs="+", choices=["facts", "rules"], default=["facts", "rules"])
    parser.add_argument("--workers", type=int, choices=range(1, 5), default=4)
    parser.add_argument("--thinking", choices=["default", "disabled"], default="disabled")
    parser.add_argument("--max-requests", type=int, default=2, help="本次最多发出的请求数，默认2；dry-run不受此限制")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--settings", type=Path, default=Path.home() / "Library/Application Support/dev.kyoku.desktop/settings.json")
    args = parser.parse_args()
    if args.max_requests < 1:
        parser.error("max-requests必须为正整数")
    settings = json.loads(args.settings.read_text())
    endpoint = settings["endpoint"]
    parsed = urlparse(endpoint)
    if parsed.scheme != "https" or parsed.hostname != "api.deepseek.com" or parsed.path != "/chat/completions" or parsed.query or parsed.username:
        parser.error("本实验只支持已配置的DeepSeek官方chat/completions地址")
    models = args.models or [settings["model"]]
    if len(set(models)) != len(models) or any(m not in {"deepseek-v4-flash", "deepseek-v4-pro"} for m in models):
        parser.error("模型必须是互不重复的deepseek-v4-flash或deepseek-v4-pro")
    if len(set(args.conditions)) != len(args.conditions):
        parser.error("实验条件不能重复")
    cases = json.loads(args.cases.read_text())
    if not cases or len({c["id"] for c in cases}) != len(cases):
        parser.error("案例不能为空且id不能重复")
    knowledge = topics(Path(__file__).resolve().parents[1] / "docs/analysis/mahjong-knowledge.md")
    args.output.mkdir(parents=True, exist_ok=True)
    jobs = []
    for case in cases:
        if not case["id"].isascii() or not case["id"].replace("-", "").isalnum():
            parser.error("案例id只允许ASCII字母、数字与连字符")
        for model in models:
            for condition in args.conditions:
                content = "已核对的当前局面事实：\n" + json.dumps(case["facts"], ensure_ascii=False, indent=2)
                if condition == "rules":
                    content += "\n\n相关规则与定义：\n" + "\n\n".join(knowledge[t] for t in case["topics"])
                content += "\n\n用户问题：\n" + case["question"]
                request = {"model": model, "messages": [{"role": "system", "content": SYSTEM}, {"role": "user", "content": content}], "max_tokens": 8192, "reasoning_effort": "low"}
                if args.thinking == "disabled":
                    request["thinking"] = {"type": "disabled"}
                    request["max_tokens"] = 4096
                digest = hashlib.sha256(json.dumps(request, ensure_ascii=False, sort_keys=True).encode()).hexdigest()
                stem = f'{case["id"]}-{model}-{condition}'
                result_path = args.output / f"{stem}.json"
                if result_path.exists():
                    previous = json.loads(result_path.read_text())
                    if previous["request_sha256"] != digest:
                        parser.error(f"请求已改变，请使用新输出目录：{stem}")
                    continue
                jobs.append((stem, request, {"case_id": case["id"], "model": model, "condition": condition, "request_sha256": digest, "input_characters": len(content)}))
    if not args.dry_run and len(jobs) > args.max_requests:
        parser.error(f"待运行{len(jobs)}条请求，超过本次上限{args.max_requests}；未发出任何请求。先dry-run检查，确需扩大时显式设置--max-requests。")
    save(args.output / "run.json", {"endpoint": endpoint, "models": models, "conditions": args.conditions, "thinking": args.thinking, "max_requests": args.max_requests, "case_sha256": hashlib.sha256(args.cases.read_bytes()).hexdigest(), "dry_run": args.dry_run, "pending": len(jobs), "workers": args.workers, "scope": "人工摘要与相关规则的单轮解释；未测试生产工具选择或历史追问"})
    if args.dry_run:
        for stem, request, metadata in jobs:
            save(args.output / f"{stem}.request.json", {**metadata, "request": request})
        print(f"已生成{len(jobs)}条请求预览；未联网", flush=True)
        return
    if not settings.get("api_key"):
        parser.error("配置缺少API Key")

    def run(job):
        stem, request, metadata = job
        started = time.monotonic()
        record = dict(metadata)
        try:
            req = Request(endpoint, data=json.dumps(request, ensure_ascii=False).encode(), headers={"Content-Type": "application/json", "Authorization": "Bearer " + settings["api_key"]})
            with build_opener(NoRedirect()).open(req, timeout=120) as response:
                body = json.load(response)
            choice = body["choices"][0]
            answer = choice["message"].get("content") or ""
            record.update(answer=answer, finish_reason=choice.get("finish_reason"), usage=body.get("usage"), served_model=body.get("model"))
            record["status"] = "delivered" if choice.get("finish_reason") == "stop" and answer.strip() else "incomplete"
        except HTTPError as error:
            record.update(status="request_failed", http_status=error.code)
        except (URLError, TimeoutError, OSError, ValueError, KeyError, IndexError, TypeError) as error:
            # 只保存错误类别，避免服务错误正文或连接信息泄漏配置。
            record.update(status="request_failed", error_type=type(error).__name__)
        record["elapsed_seconds"] = round(time.monotonic() - started, 3)
        save(args.output / f"{stem}.json", record)
        return stem, record

    with ThreadPoolExecutor(max_workers=args.workers) as pool:
        for future in as_completed([pool.submit(run, job) for job in jobs]):
            stem, record = future.result()
            print(f'{stem}: {record["status"]}, {record["elapsed_seconds"]}s', flush=True)
    records = []
    for case in cases:
        for model in models:
            for condition in args.conditions:
                records.append(json.loads((args.output / f'{case["id"]}-{model}-{condition}.json').read_text()))
    save(args.output / "summary.json", {"total": len(records), "delivered": sum(r["status"] == "delivered" for r in records), "semantic_review": "pending_manual_review", "records": [{k: v for k, v in r.items() if k != "answer"} for r in records]})


if __name__ == "__main__":
    main()
