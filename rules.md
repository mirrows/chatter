# Chatters 项目规则（对 AI 代码生成助手的约束）

> 版本：v1.0　日期：2026-10-27　适用范围：本仓库内所有代码、文档、测试的生成行为

## 0. 约束生效说明

本文件是 AI 助手在本仓库内工作的强制约束。AI 助手在开始任何任务前必须先读取本文件，并严格遵守；每次会话、每轮代码生成前都应自查是否符合。若与用户口头指令冲突，以用户指令为准，但需在回复中说明冲突点。

## 1. 文件与路径命名

1. **R1.1 所有文件名和目录名必须使用英文**（ASCII 字母、数字、`-`、`_`、`.`）。禁止使用中文或其他非 ASCII 字符——避免命令执行报错。
2. R1.2 目录名用小写或 kebab-case；源码文件遵循对应语言的社区惯例（Rust 用 `snake_case.rs`，Vue 组件用 `PascalCase.vue`，TS 模块用 `camelCase.ts`）。
3. R1.3 文档统一放在 `requirements/` 目录，文件名用数字前缀（如 `01-project-overview.md`）。
4. R1.4 任何文件名、目录名不得包含空格。
5. R1.5 **文档正文内容以中文为主**；文件名英文只是命名要求，不代表正文语言要求。

## 2. 功能开发与测试流程（强制）

1. **R2.1 一次只做一个功能点。** 功能是最小可独立交付的单元（如"UDP 发现广播"、"TEXT 帧收发"）。当前功能未完成前，不得开始下一个功能。
2. **R2.2 每个功能必须配单元测试。** 功能代码完成时，必须在统一测试目录生成对应测试文件：
   - Rust 后端：`src-tauri/tests/<module>/<feature>.rs`（集成测试）和/或模块内 `#[cfg(test)] mod tests`（单元测试）。
   - 前端：`tests/<module>/<feature>.spec.ts`（Vitest），与被测组件/存储对应。
   - 所有测试文件统一放在上述位置，不允许散落。
3. **R2.3 测试通过门禁。** 功能只有在其**全部测试用例通过**（`cargo test` 与 `npm run test` 退出码为 0）后才算完成，才能开始下一个功能。若有用例失败，应修复代码或经用户同意后调整测试——严禁带着失败的用例宣布功能完成。
4. R2.4 每个功能至少覆盖：一个正常路径用例、一个边界用例、一个失败/异常用例（如非法帧、传输中断）。
5. R2.5 完成前必须运行**全量**既有测试套件做回归，而不只跑新测试。
6. R2.6 测试文件与用例命名必须与所验证功能对应（如 `discovery_announce_test`），失败时可直接定位功能。

## 3. 代码质量

1. R3.1 工具链门禁：Rust 侧 `cargo fmt` + `cargo clippy -- -D warnings`；前端 `eslint` + `prettier` + `vue-tsc --noEmit`。功能完成前必须全部通过。
2. R3.2 Rust 网络与 IO 代码路径禁止 `unwrap()`；用 `Result` 传播并记录错误。仅测试代码和真正不可能失败的场景（需注释说明）允许 `unwrap()`。
3. R3.3 所有用户可见文案走 i18n 资源（首版中文），组件内不得硬编码 UI 字符串。
4. R3.4 公开函数/类型必须写文档注释（`///`）；复杂逻辑加英文行内注释。
5. R3.5 TypeScript 禁用 `any`；Rust↔前端共享类型两侧各定义一次并保持同步（见 `requirements/02-technical-design.md` 第 8 节）。
6. R3.6 不得提交死代码、无引用的 TODO、被注释掉的代码。

## 4. Git 与版本

1. R4.1 提交信息遵循 Conventional Commits（`feat:`、`fix:`、`test:`、`docs:`、`refactor:`、`chore:`），scope 为模块名（如 `feat(transfer): simple resume`）。
2. R4.2 每个逻辑变更一个提交；功能与其测试放在同一提交。
3. R4.3 `main` 分支保持全绿（全部测试通过）。功能开发在短期分支 `feat/<功能名>` 上进行，仅在 R2.3 门禁通过后合并。
4. R4.4 发布版本使用语义化版本（`v1.0.0`）；预发布用 `v1.0.0-rc.1`。

## 5. 安全与数据

1. R5.1 调试日志默认不输出消息明文与文件路径，用 ID 代替。
2. R5.2 接收文件必须落入配置的下载目录；代码中必须拒绝路径穿越（如文件名中的 `../`），且必须有对应测试用例。
3. R5.3 网络输入（帧）按协议限制长度；超长帧必须拒绝，并有测试用例。
4. R5.4 禁止遥测、禁止任何外网连接；代码生成时自查不得出现非局域网端点。

## 6. 文档

1. R6.1 `requirements/` 是需求范围的唯一基准；代码行为偏离文档即为 bug，不是新功能。
2. R6.2 协议变更必须先更新 `02-technical-design.md`，同一提交内完成。
3. R6.3 每完成一个里程碑，生成 `requirements/0N-milestone-report.md`，总结交付内容与测试状态。

## 7. 环境与构建

1. R7.1 构建运行不得依赖网络（依赖安装除外）。
2. R7.2 依赖版本锁定（`Cargo.lock`、`package-lock.json` 提交入库）。
3. R7.3 项目须能按 `README.md` 步骤在干净机器上构建。
4. R7.4 构建生成物不入库：`target/`、`dist/`、`src-tauri/gen/schemas/`（`tauri_build::build()` 按当前依赖重建，仅供编辑器校验 `capabilities/*.json` 的 `$schema`；新克隆首次构建前编辑器缺 schema 属正常）。`src-tauri/gen/android/` 是源码工程（Manifest、Kotlin 插件、Gradle），必须入库，不得被忽略规则误伤。
5. R7.5 签名材料不入库：Android keystore（`*.jks`/`*.keystore`）、Windows 证书（`*.pfx`）、`keystore.properties`、`.secrets/` 一律由 `.gitignore` 排除；CI 通过 GitHub Secrets 注入（名称见 `02` §11.3），禁止把密钥或密码写入代码、文档或提交历史。

## 8. 交互与提示规范（v3）

1. **R8.1 轻量级提示统一使用 `message` 组件。** 成功 / 失败 / 信息类的一次性提示必须调用
   `src/stores/message.ts` 导出的 `message.success` / `message.error` / `message.info`
   （渲染由 `MessageHost.vue` 负责），**默认 1.5s 后自动消失**（`DEFAULT_MESSAGE_DURATION`），
   同屏最多 3 条、超出排队。
2. R8.2 禁止使用 `window.alert` / `window.confirm`。需要用户确认用 `confirmDialog`（`stores/confirm.ts`），
   需要单行输入用 `promptDialog`（`stores/prompt.ts`）。
3. R8.3 平台能力缺失（例如 Android 无法修改下载目录）用 `message.info` 说明，不得写入页面级 `error` 状态。
4. R8.4 页面级常驻信息（保存成功徽标、需重启提示、清空历史结果）保留就地展示，不属于轻量提示。
5. R8.5 提示文案同样走 i18n 资源（见 R3.3），不得在组件内硬编码。
