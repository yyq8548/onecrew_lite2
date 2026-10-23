# OneCrew Lite · Windows 使用说明

适用于 Windows x64。电脑需要安装 Codex 桌面应用及 Google Chrome，使用客户自己的 OneCrew、平台和 Gmail 账号。

通过 Codex 安装：将 ZIP 交给 Codex，并要求它读取 INSTALL-CODEX.md，依次直接执行安装命令。该流程不需要运行 CMD 或 PowerShell 脚本文件。通过 Access 下载时，先核对该发布记录提供的 ZIP 大小和 SHA-256。

## 首次安装

1. 将整个安装包解压到固定目录，完整退出 Codex。
2. 双击 Start-OneCrew.cmd，等待窗口确认本地服务启动及配套插件安装成功。日常使用采用普通权限；管理员账户也可使用。
3. 打开 Codex，输入“打开 OneCrew Lite 工作台和资料库”。在工作台登录自己的账号，在平时使用的 Chrome 安装并连接官方 OpenCLI 扩展，然后在工作台选择并授权该浏览器。已有网站登录继续使用。
4. 确认搜索条件后开始搜索，核对工作台、状态卡及动态表格的实际结果。

首次安装的数据目录为当前用户的 `%LOCALAPPDATA%\OneCrewLite\TypeScript`。安装入口会自动检查当前用户的服务登记、Codex 插件和历史默认目录，升级时沿用唯一旧目录。多个候选会明确提示选择，避免误用空账号。账号库、主密钥、表格和浏览器身份保留。安装包不包含开发者账号或浏览器数据。

## 升级与停止

将新包解压到新的程序目录，运行 Start-OneCrew.cmd 即可自动识别安装或升级。同构建正在运行时复用实例；不同构建则确认停止后升级；已停止则重新启动。Upgrade-OneCrew.cmd 可明确请求重启升级。状态检查失败会明确报错。插件提示需要重启 Codex 时，完整退出后重新打开。

Stop-OneCrew.cmd 只停止本地服务，保留账号和搜索数据。再次运行 Start-OneCrew.cmd 可继续使用。

存在多个候选目录或自定义目录没有服务及插件登记时，核实后明确指定原目录。例如在 PowerShell 中执行：

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\OneCrew.ps1 -Action Upgrade -DataDirectory "原数据目录的完整路径"
```

旧内部测试包使用的目录为 `%LOCALAPPDATA%\OneCrewLite\TypeScript-Test`。不要把更换目录后出现的空白界面当成历史数据丢失，也不要只复制数据库而遗漏主密钥或浏览器身份。

## 功能与故障信息

对外发送只保留邮件页的 Gmail 批量发送。真实发送需要客户确认。后台表格同步继续保留，使用客户账号已有的授权记录。

出现程序校验失败、目录归属错误、连接中断或插件安装失败时，保留完整错误文字。不要删除数据目录或修改 Windows 权限设置来掩盖错误。

Mac ARM 与 Mac Intel 的原生验收尚未完成；本目录提供 Windows 安装入口。
