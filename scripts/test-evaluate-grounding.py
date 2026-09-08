#!/usr/bin/env python3
"""本地验证实验调用上限与断点续跑，不连接模型服务。"""

import contextlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


SPEC = importlib.util.spec_from_file_location(
    "evaluate_grounding", Path(__file__).with_name("evaluate-grounding.py")
)
RUNNER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RUNNER)


class GroundingBudgetTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.settings = self.root / "settings.json"
        self.settings.write_text(json.dumps({
            "endpoint": "https://api.deepseek.com/chat/completions",
            "model": "deepseek-v4-flash", "api_key": "synthetic-test-key",
        }))
        self.cases = self.root / "cases.json"
        self.output = self.root / "results"

    def write_cases(self, count):
        self.cases.write_text(json.dumps([
            {"id": str(i), "question": "测试问题", "facts": {"count": i},
             "topics": [], "expected_review": "do-not-send-this-gold"}
            for i in range(count)
        ]))

    def run_script(self, *extra):
        args = ["evaluate-grounding.py", str(self.cases), str(self.output),
                "--settings", str(self.settings), *extra]
        with patch("sys.argv", args), contextlib.redirect_stdout(io.StringIO()), \
                contextlib.redirect_stderr(io.StringIO()):
            RUNNER.main()

    def test_budget_stops_before_network_but_dry_run_can_preview_all(self):
        self.write_cases(3)
        with patch.object(RUNNER, "build_opener") as network:
            with self.assertRaises(SystemExit) as error:
                self.run_script()
            self.assertEqual(error.exception.code, 2)
            network.assert_not_called()
            self.assertEqual(list(self.output.iterdir()), [])
            self.run_script("--dry-run")
            network.assert_not_called()
        previews = list(self.output.glob("*.request.json"))
        self.assertEqual(len(previews), 6)
        for path in previews:
            text = path.read_text()
            request = json.loads(text)["request"]
            self.assertEqual(request["thinking"], {"type": "disabled"})
            self.assertEqual(request["max_tokens"], 4096)
            self.assertNotIn("synthetic-test-key", text)
            self.assertNotIn("do-not-send-this-gold", text)

    def test_resume_skips_completed_calls_and_rejects_changed_requests(self):
        self.write_cases(1)
        response = {"choices": [{"finish_reason": "stop", "message": {"content": "本地模拟回答"}}]}
        with patch.object(RUNNER, "build_opener") as network:
            network.return_value.open.side_effect = lambda *a, **k: io.BytesIO(json.dumps(response).encode())
            self.run_script()
            self.assertEqual(network.return_value.open.call_count, 2)
            for call in network.return_value.open.call_args_list:
                payload = call.args[0].data.decode()
                self.assertNotIn("synthetic-test-key", payload)
                self.assertNotIn("do-not-send-this-gold", payload)
            saved = {p.name: p.read_bytes() for p in self.output.glob("*-facts.json")}
            self.run_script()
            self.assertEqual(network.return_value.open.call_count, 2)
            cases = json.loads(self.cases.read_text())
            cases[0]["question"] = "改变的问题"
            self.cases.write_text(json.dumps(cases))
            with self.assertRaises(SystemExit):
                self.run_script()
            self.assertEqual(network.return_value.open.call_count, 2)
            self.assertEqual(saved, {p.name: p.read_bytes() for p in self.output.glob("*-facts.json")})


if __name__ == "__main__":
    unittest.main()
