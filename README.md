# Chatters — 局域网点对点传输工具（文字 / 图片 / 文件）

> 无需服务器、免登录的局域网点对点传输工具，支持文字、图片与文件传输。
> 技术栈：Tauri 2.x + Vue 3 + Rust 后端。当前状态：需求与技术方案已完成，开发未开始。

## 目录结构

- `requirements/` — 需求与设计的唯一基准（文件名英文，正文中文）
  - `01-project-overview.md` — 需求文档（v0.3）
  - `02-technical-design.md` — 架构、协议、模块划分（v0.1）
- `tests/` — 统一测试目录（布局见 rules.md R2.2）
- `rules.md` — 对 AI 代码生成助手的强制工程约束

## 当前状态

- 需求：已确认（Q1–Q8）— 见 `requirements/01-project-overview.md` 第 4 节
- 技术栈：Tauri 2.x + Vue 3 + Rust（已确认）
- 范围排除项：不做消息撤回、不支持传输记录导出；仅保留传输终止（取消）
- 下一步：M2-a 工程脚手架
