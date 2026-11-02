# MiCoPi

[English](README.md) · [简体中文](README.zh.md) · [日本語](README.ja.md) · [한국어](README.ko.md)

使用 Rust 编写的轻量原生绘画软件，优先保证绘画响应，同时提供可扩展、面向 AI 的执行核心。

目前支持 **Windows 10/11 x64**。桌面工作台使用 **egui + egui_dock** 和 OpenGL，数位笔输入使用 Windows Ink。项目仍处于早期开发阶段。

**0.21.0：** 新增[可验证的事件回放](docs/replay.md)。记录笔触压感、图层和插件操作，支持时间轴播放、SHA-256 完整性校验、工程内保存历史，以及导出回放文件和 PNG 帧序列。

新增[原子操作组](docs/atomic-operations.md)：失败时回滚整组画布修改，只提交一条组级事件。MCRPLY02 使用元数据差分、像素块引用和 XOR 压缩，仍支持旧版回放。

![MiCoPi 实际工作台，截图为英语界面](docs/screenshots/workbench.png)

## 主要理念

**绘画优先，行为由插件承载，人与自动化共用同一个执行核心。**

逐点输入采用强类型接口，保留压感、防抖和局部更新。插件、MCP 与 headless 通过带 schema 的 action 和公开状态操作同一个 Session。图像节点非破坏式合成画布；操作节点只有明确运行时才执行。

“AI-ready”代表可发现、可检查、可验证的集成契约，不代表必须联网或内置云端助手。配图与详细说明见英文[设计理念](docs/philosophy.md)、[文档索引](docs/README.md)、[开发路线图](ROADMAP.md)和[目录规划](docs/project-structure.md)。技术文档以英语为准。

## 当前功能

- 独立 DLL 实现画笔、橡皮、吸管、抓手和缩放。
- Windows Ink 压感、压力曲线、防抖和笔刷轮廓光标。
- HSV 色环、RGB 控件、色板和笔刷预设。
- 图层显隐、透明度、混合模式和分块撤销/重做。
- 可停靠、浮动、分组的标签面板；间距、尺寸、主题和快捷键可配置。
- 集中搜索设置、JSON 编辑、命令面板和快捷键录制。
- PNG 透明导出、PNG/JPEG/BMP 导入和 `.mcp` 工程文件。
- 非破坏式图像节点与插件注册的 action/状态流程节点。
- JSONL 批处理、MCP stdio，以及连接正在运行的 GUI。

图层支持重命名、复制（**Ctrl+J**）、重排和删除，以及精确翻转、旋转 180° 和整数偏移。详见[图层编辑](docs/layer-editing.md)。

当前每个 Session 只支持一份文档；尚无 Wintab、选区、自由变换、图层组/合并或水彩引擎。原生 DLL 与宿主在同一进程中执行。

## 构建与运行

需要 Rust 1.88+、Visual Studio C++ Build Tools 和 Windows SDK。

```powershell
.\scripts\build.ps1 -PluginsOnly
cargo run --release --locked
.\scripts\build.ps1 -Test
```

运行 `dist/MiCoPi.exe`，保留同目录的 `plugins/` 和 `licenses/`。最终用户无需安装 Rust。项目本地便携工具链的用法见英文[开发指南](docs/development.md)。

```powershell
New-Item -ItemType Directory -Force target/example-output | Out-Null
.\dist\micopi-cli.exe run examples/automation/draw.jsonl
.\dist\micopi-cli.exe actions
.\dist\micopi-cli.exe mcp
```

默认 MCP 使用独立画布；通过 `mcp --attach <profile>/session.json` 可连接现有 GUI。示例按用途归类，见 [examples](examples/README.md)。

## 界面语言

默认语言为英语，支持简体中文、日语和韩语。按 **Ctrl+,** 打开设置，搜索 `ui.language`，选择语言并保存。切换会立即预览；关闭未保存的设置会恢复上次保存的值。

```json
{"version":1,"ui":{"language":"zh-CN"}}
```

语言不会改变 action ID、配置键、节点端口、文件路径或画布像素。详见英文[多语言说明](docs/localization.md)。

## 目录与贡献

`src/` 保存宿主与界面，`plugins/` 保存可安装插件包和独立算法，`crates/plugin-api/` 保存 ABI SDK。文档、截图、示例和工具分别位于 `docs/`、`docs/screenshots/`、`examples/` 和 `scripts/`。构建、发行产物与个人作品不进入 Git。

请阅读英文[贡献指南](CONTRIBUTING.md)、[架构说明](docs/architecture.md)和[变更记录](CHANGELOG.md)。英语 README 是项目概述的基准，三个翻译版本与其保持一致。

## 许可证

应用源码采用 [MIT](LICENSE)。内嵌 Google Material 图标采用 [Apache 2.0](assets/material-symbols/LICENSE)，详见[许可说明](assets/material-symbols/NOTICE.md)。发行目录包含第三方依赖许可。
