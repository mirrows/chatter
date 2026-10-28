# Chatters 文档索引

> 版本：v1.0　日期：2026-10-28　说明：本文件编号 `03`，用于记录 `requirements/` 的文档清单与当前状态。

## 1. 文档清单

| 编号 | 文件 | 内容 | 状态 |
|---|---|---|---|
| 01 | `01-project-overview.md` | 项目概述、功能/非功能需求、已确认事项（Q1–Q15）、里程碑与 v2 路线图 | v0.5（2026-10-28） |
| 02 | `02-technical-design.md` | 架构、协议（含 v2 块级续传/能力协商）、模块划分、命令/事件、存储、跨平台适配 | v0.2（随实现迭代） |
| 03 | `03-doc-index.md` | 本索引（历史编号空缺，现补齐为索引页） | v1.0 |
| 04 | `04-milestone-m2-report.md` | M2 报告（设备发现 + 文字消息） | 已完成 |
| 05 | `05-milestone-m3a-report.md` | M3-a 报告（图片/文件传输主流程） | 已完成 |
| 06 | `06-milestone-m3b-report.md` | M3-b 报告（续传/超时/重试） | 已完成 |
| 07 | `07-milestone-m4-report.md` | M4 报告（设置/历史/托盘/自启） | 已完成 |
| 08 | `08-milestone-m5-report.md` | M5 报告（打包与交付） | 安装包待联网 |
| 09 | `09-user-guide.md` | 使用说明（安装、收发、设置、FAQ） | 随功能更新 |
| 10 | `10-milestone-p0-report.md` | P0 收尾报告（F3/F4/F21/F22/F31/F51、单实例、图标） | 已完成 |
| 11 | `11-acceptance-checklist.md` | v1.0.0-rc.1 验收清单（自动化 + F1–F53） | 待实机执行 |
| 12 | `12-milestone-p1-p2-report.md` | P1/P2 报告（文件操作、历史清理、托盘、发布工程） | 已完成 |
| 13 | `13-v2-plan.md` | v2 迭代计划（多平台 M6–M9、网络与性能 M10、完整续传 M11） | 已批准，待启动 |

## 2. 约定

1. `requirements/` 是需求与范围的唯一基准（`rules.md` R6.1）；代码行为偏离文档按 bug 处理。
2. 协议或范围变更必须与文档更新同批提交（R6.2）。
3. 每完成一个里程碑新增 `0N-milestone-*.md` 报告（R6.3），并在本索引登记。
4. 未完成项统一记录在 `01` §3.5，避免散落各报告。

## 3. 当前未完成项入口

- 功能/非功能缺口：`01-project-overview.md` §3.5
- 待执行验收：`11-acceptance-checklist.md`
- 发布阻塞项（NSIS、签名、ESLint、E2E）：`12-milestone-p1-p2-report.md` §5
