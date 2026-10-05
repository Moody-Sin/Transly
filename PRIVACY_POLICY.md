# Transly 隐私权政策 (Privacy Policy)

**生效日期：2026年10月6日**

Transly（以下简称“我们”）高度重视用户的隐私安全与个人数据保护。本隐私政策旨在向您说明 Transly 桌面客户端及浏览器扩展程序在您使用过程中如何处理、收集与保护您的信息。

## 1. 核心隐私承诺：无数据收集与完全离线/本地处理

Transly 坚持**“本地优先，用户主权”**的架构设计原则：
- **不收集个人身份信息 (PII)**：我们不要求用户注册账号，不收集您的姓名、电子邮箱、手机号或任何身份信息。
- **不进行用户行为追踪**：Transly 没有集成任何第三方数据分析 SDK、广告追踪器或遥测（Telemetry）工具。
- **零数据上报**：除用户在本地主动配置并触发的第三方翻译服务（如 DeepL、OpenAI 等）外，Transly 不会向任何服务器上传您的翻译内容、浏览历史或使用统计。

## 2. 数据处理机制

### 2.1 翻译文本与剪贴板数据
- **桌面端输入与剪贴板**：当您使用快捷键翻译或开启剪贴板监听时，文本仅在本地内存中流转并用于请求翻译；在识别剪贴板图片 OCR 时，图片数据仅在系统内存中即时处理，识别完成后立即释放，绝不在磁盘产生任何临时文件或图像缓存。
- **浏览器扩展**：浏览器扩展在您主动触发整页翻译时，仅提取当前网页中可见的 DOM 文本节点，通过本地回环地址（`127.0.0.1`）发送至您本地运行的 Transly 桌面端处理，不采集当前页面的 Cookie、表单输入、密码或浏览凭据。

### 2.2 本地翻译缓存与术语库
- **SQLite 缓存**：为了加速重复翻译并减少网络开销，Transly 默认在您本地磁盘建立加密/受保护的 SQLite 缓存。您随时可以在设置中彻底关闭缓存，关闭时系统将自动清空所有已存储的翻译历史，并保证在途请求零落盘。
- **术语库**：您自定义的术语规则全部保存在本地，绝不向外同步。

### 2.3 API 密钥与通信安全
- **系统原生凭据托管**：您所配置的第三方 API 密钥（如 DeepL、OpenAI、DeepSeek Key）以及桌面端与扩展之间的配对 Bearer Token，均直接托管于操作系统原生安全存储（Windows Credential Manager / macOS Keychain / Linux Secret Service），严禁明文落盘。
- **本地回环保护**：桌面端仅在 `127.0.0.1:39281` 监听本地通信，强制进行 Bearer Token 强鉴权与严格的扩展 CORS 白名单校验，彻底防范恶意外部网页嗅探。

## 3. 第三方翻译服务说明

当您在设置中选择并启用了第三方商业翻译引擎（如 DeepL、OpenAI 或 DeepSeek）时，待翻译文本将直接从您的客户端发往相应提供商的官方 API 节点。相关数据处理受相应第三方服务商的隐私政策约束：
- DeepL 隐私政策：[https://www.deepl.com/privacy](https://www.deepl.com/privacy)
- OpenAI 隐私政策：[https://openai.com/privacy](https://openai.com/privacy)
- DeepSeek 隐私政策：[https://www.deepseek.com/privacy](https://www.deepseek.com/privacy)

若您使用内置的 Google 网页翻译，请求仅通过加密 HTTPS 协议直连官方接口获取译文。

## 4. 权限使用说明（浏览器扩展）

Transly 浏览器扩展仅申请最小必要权限：
- `activeTab` / `scripting`：仅在您点击扩展图标或点击“翻译此网页”时，用于向当前活动标签页注入原地 DOM 替换脚本。
- `storage`：仅用于在本地保存扩展与本地桌面端的配对 Bearer Token 及界面偏好设置。

## 5. 联系方式

如果您对本隐私政策有任何疑问、意见或建议，请通过 GitHub 仓库向我们提交反馈：
- GitHub: [https://github.com/Moody-Sin/Transly](https://github.com/Moody-Sin/Transly)
