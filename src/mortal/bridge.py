"""将官方 Mortal Bot 包装成一条输入对应一条输出的本地推理进程。"""

import hashlib
import json
from pathlib import Path
import sys


def main():
    runtime, checkpoint, player = sys.argv[1:]
    sys.path.insert(0, str(Path(runtime).resolve() / "mortal"))

    import torch
    from engine import MortalEngine
    from model import Brain, DQN
    from libriichi.mjai import Bot

    # 小批量 CPU 推理避免线程争用；完整候选评价不能使用 quick_eval。
    torch.set_num_threads(1)
    state = torch.load(checkpoint, weights_only=True, map_location="cpu")
    config = state["config"]
    version = config["control"].get("version", 1)
    if version != 4:
        raise ValueError(f"only four-player Mortal V4 is supported, got V{version}")
    brain = Brain(version=version, **config["resnet"]).eval()
    dqn = DQN(version=version).eval()
    brain.load_state_dict(state["mortal"])
    dqn.load_state_dict(state["current_dqn"])
    engine = MortalEngine(
        brain, dqn, is_oracle=False, version=version,
        device=torch.device("cpu"), enable_amp=False,
        enable_quick_eval=False,
        # 规则保护可能改变最终动作，返回的 Q 值仍是原始评价；以 Bot 的动作输出为准。
        enable_rule_based_agari_guard=True,
        name="kyoku-mortal",
    )
    bot = Bot(engine, int(player))
    with open(checkpoint, "rb") as source:
        digest = hashlib.file_digest(source, "sha256").hexdigest()
    print(json.dumps({
        "version": version,
        "tag": str(state.get("tag", Path(checkpoint).name)),
        "sha256": digest,
    }), flush=True)

    # 只送入真实牌谱事件；推荐动作不会反过来改变下一步的牌谱分支。
    for line in sys.stdin:
        reaction = bot.react(line)
        print(reaction or '{"type":"none","meta":{"mask_bits":0}}', flush=True)


if __name__ == "__main__":
    main()
