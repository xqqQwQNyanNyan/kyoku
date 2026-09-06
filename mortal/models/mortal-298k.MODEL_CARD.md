---
license: agpl-3.0
datasets:
- VoidShine/mortal-dataset
tags:
- mahjong
- riichi-mahjong
- reinforcement-learning
- game-ai
---

# Mortal Training Checkpoint — 298k Steps

> **⚠️ WARNING: This model is NOT recommended for use in ranked matches.**

---

### Overview

A trained checkpoint of [Mortal](https://github.com/Equim-chan/Mortal), an AI for Japanese Riichi Mahjong powered by deep reinforcement learning. This is a **hanchan (south round) model**.

### Model

- **Base framework**: [Mortal](https://github.com/Equim-chan/Mortal) (by Equim)
- **Checkpoint**: `mortal_298k.pth` (298,000 steps)
- **Architecture**: ResNet with 192 channels, 40 blocks
- **Game type**: 4-player hanchan (south round)

### Dataset

Training data sourced from [Tenhou](https://tenhou.net/) high-level 4-player hanchan games (2025–2026), stored under `dataset/4p_hanchan/`.

> Note: `dataset/4p_tonpuu/` (east round data) is included in the repository but was **not** used for training this model.

### Performance

- **AI similarity**: 83%–92% on [mjai.ekyu.moe](https://mjai.ekyu.moe/zh-cn.html)
- **Mahjong Soul**: Average S+ level on MAKA test

### Usage

1. Clone the [Mortal](https://github.com/Equim-chan/Mortal) repository and follow its setup instructions.
2. Place `mortal_298k.pth` at the path specified by `state_file` in `config.toml`.
3. Adjust paths in `config.toml` to match your local environment.
4. Refer to Mortal's documentation for inference and training details.

### Configuration

See `config.toml` for the full training configuration. All paths are set to `/path/to/` placeholders — replace them with your actual local paths before use.

### License

This project is licensed under [AGPL-3.0](LICENSE), consistent with [Mortal](https://github.com/Equim-chan/Mortal).

The dataset consists of game logs from [Tenhou](https://tenhou.net/). Please respect Tenhou's terms of service when using this data.
