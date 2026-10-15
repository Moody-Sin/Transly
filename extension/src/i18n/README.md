# extension/src/i18n (浏览器扩展国际化辅助)

> 基于 Chrome 原生 i18n 规范的文案查询封装与静态 DOM 属性注入工具。

[![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg?style=flat-square)](https://github.com/RichardLitt/standard-readme)

本模块封装 `chrome.i18n.getMessage` API，为扩展各上下文（Popup、Selection 等）提供统一且健壮的文案读取能力，并支持对 HTML 静态 DOM 节点进行批量本地化属性扫描与注入。

## 目录

- [功能说明](#功能说明)
- [使用方法](#使用方法)
- [测试与验证](#测试与验证)
- [许可证](#许可证)

## 功能说明

- **t(key, ...substitutions)**：安全查询 `chrome.i18n.getMessage`，当键值不存在或处于非扩展环境时，安全回退至键名本身，杜绝运行时报错。
- **applyI18n(root)**：遍历目标 DOM 容器，自动扫描并替换带有 `data-i18n`、`data-i18n-placeholder`、`data-i18n-title` 的元素属性。

## 使用方法

在 HTML 中声明属性：
```html
<button data-i18n="popupTranslateBtn"></button>
<input data-i18n-placeholder="popupTokenPlaceholder" />
```

在 TS 中调用：
```ts
import { applyI18n, t } from '../i18n'

applyI18n(document)
const alertMsg = t('commonCopied')
```

## 测试与验证

扩展语言包字典（`_locales/zh_CN/messages.json` 与 `_locales/en/messages.json`）的键对称性由自动化单测守卫：

```sh
pnpm run test:task
```

## 许可证

[商业专有许可](../../../LICENSE)
