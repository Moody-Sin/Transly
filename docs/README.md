# docs (设计与架构文档)

> Transly 的架构决策与跨端边界说明文档。

[![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg?style=flat-square)](https://github.com/RichardLitt/standard-readme)

本目录存放不属于「使用说明」范畴、但维护者必须了解的设计依据。使用说明见根目录 `README.md`，工程规范见 `PROJECT_RULES.md`，踩坑记录见 `Lessons Learned.md`。

## 目录

- [文档索引](#文档索引)
- [撰写约定](#撰写约定)
- [许可证](#许可证)

## 文档索引

- **[architecture-state-machines.md](architecture-state-machines.md)**：桌面端与浏览器扩展两套任务状态机的语义对比、为什么不跨进程合并、以及各自的代际隔离边界。

## 撰写约定

- 每新增一篇文档，必须在本索引中登记，否则无人能发现它。
- 文档必须回答「为什么这样设计」以及「什么情况下这个结论会失效」，只复述代码的文档不予收录。

## 许可证

[MIT](../LICENSE)
