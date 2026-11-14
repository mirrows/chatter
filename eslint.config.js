// ESLint 9 扁平配置（v3）：TypeScript + Vue 3。
//
// 目标与 `rules.md` R3.1 一致：只做「正确性」检查，格式化交给 Prettier，
// 因此这里使用 `flat/essential` 而非 `flat/recommended`，避免格式类规则互相打架。
import js from "@eslint/js";
import pluginVue from "eslint-plugin-vue";
import globals from "globals";
import tseslint from "typescript-eslint";

export default tseslint.config(
  {
    // 构建产物与生成代码不参与检查
    ignores: [
      "dist/**",
      "node_modules/**",
      "src-tauri/target/**",
      "src-tauri/gen/**",
      "src-tauri/icons/**",
      "coverage/**",
    ],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  ...pluginVue.configs["flat/essential"],
  {
    files: ["**/*.{ts,tsx,vue}"],
    languageOptions: {
      ecmaVersion: 2022,
      sourceType: "module",
      globals: { ...globals.browser, ...globals.node },
    },
    rules: {
      // Vue 组件文件名沿用现有 PascalCase / 单文件视图约定
      "vue/multi-word-component-names": "off",
      // TS 自己负责未定义变量检查，避免对类型声明误报
      "no-undef": "off",
      // 允许以 _ 开头的占位参数（事件处理器等）
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_" },
      ],
    },
  },
  {
    // `.vue` 的 `<script setup lang="ts">` 需要 TS 解析器
    files: ["**/*.vue"],
    languageOptions: {
      parserOptions: {
        parser: tseslint.parser,
        extraFileExtensions: [".vue"],
        sourceType: "module",
      },
    },
  },
  {
    files: ["tests/**/*.ts"],
    rules: {
      // 测试里大量使用断言式非空访问与 mock 类型
      "@typescript-eslint/no-non-null-assertion": "off",
    },
  },
);
