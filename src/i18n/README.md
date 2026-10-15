# src/i18n (前端国际化语言包)

> Transly 桌面端前端国际化语言字典、强类型约束与对称性保障。

[![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg?style=flat-square)](https://github.com/RichardLitt/standard-readme)

本目录承载 Transly 桌面端渲染层的所有 UI 文案。遵循「禁止在代码与组件中硬编码显示文案」的工程原则，所有界面提示、错误说明、按钮文案均通过语言包集中管理。

## 目录

- [结构](#结构)
- [类型安全与约束](#类型安全与约束)
- [测试与验证](#测试与验证)
- [许可证](#许可证)

## 结构

- **zh-CN.json**：简体中文语言包（主基准字典）。
- **en.json**：英文语言包（完整对称翻译）。
- **index.ts**：语言类型导出（`Locale`、`I18nSchema`）与当前语言字典获取函数 `getTranslation`。

## 类型安全与约束

1. **编译期严格键值推导**：`index.ts` 将 `en.json` 显式标注为 `I18nSchema`（基于 `zh-CN.json` 与后端 OCR 错误绑定生成）。当英文语言包缺失任何键或层级结构不匹配时，TypeScript 编译期（`tsc`）直接报错阻断，杜绝运行时 `undefined` 渲染。
2. **禁止硬编码文案**：所有组件均应通过 `t = getTranslation(locale)` 获取文案，绝不直接在 JSX 中编写未经抽取的文本。

## 测试与验证

语言包的对称性与嵌套结构完整性纳入自动化测试门禁：

```sh
pnpm run test:task
```

底层执行 `src/task/i18n.test.ts`，自动递归遍历并深度比对 `zh-CN.json` 与 `en.json`（以及扩展端语言包）的所有叶子键路径，在 pre-commit 与 CI 阶段杜绝漏翻或多余键。

## 许可证

[商业专有许可](../../LICENSE)
