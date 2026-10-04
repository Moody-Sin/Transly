# extension (浏览器扩展)

> 基于 Manifest V3 的 Transly 沉浸式整页翻译插件。

[![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg?style=flat-square)](https://github.com/RichardLitt/standard-readme)

本插件是与桌面端同等地位的一等公民，**核心定位是“整页翻译仅译文”**——极简、低干扰、高保真。类似原生浏览器翻译，用目标语言译文原地直接替换网页原始文本（仅译文模式），绝不做破坏排版的双语混排、不堆砌词典与花哨分析。

## 目录

- [架构原则](#架构原则)
- [关键技术](#关键技术)
- [构建与调试](#构建与调试)
- [许可证](#许可证)

## 架构原则

1. **瘦客户端**：插件端**不存 API 密钥**，**不直连外网翻译接口**。
2. **本地协作**：所有翻译请求通过本地回环地址（`127.0.0.1:39281`）路由至 Transly 桌面端处理，共享凭据与 SQLite 缓存。

## 关键技术

- **原地 DOM 替换**：精确定位文本节点直接替换，保留原有 CSS 样式与页面排版，并记录原始文本属性支持一键无损还原。
- **批量合并翻译 (Batch Translation)**：按可视区域文本块聚合，批量交由桌面端处理，避免碎片化网络请求。
- **增量监听 (MutationObserver)**：自适应 SPA 页面动态加载与瀑布流内容。

## 构建与调试

```sh
cd extension
pnpm build
```

在 Chrome 扩展管理界面中开启开发者模式，点击“加载已解压的扩展程序”，选择 `extension/dist` 即可。

## 许可证

[MIT](../LICENSE)
