# Facebook 登录检测验收

2026-10-16 在专用 Chrome 身份 `jga2zn4u` 中完成真实页面验收。

Facebook 的 `/me` 请求跳转到首页的已保存账号登录页，页面显示 `Continue`、`Use another profile`，并包含提交到同源 `/login/device-based/login/caa/` 的表单。此时账号尚未完成登录，页面没有邮箱或密码输入框。

登录检测现已识别这类页面：同时要求首页、可见的继续和切换账号控件，以及指定同源登录表单。账号身份不会随检测结果返回。

实际 OpenCLI 完整检测从 `Current-account identity could not be verified` 变为 `not_logged_in`，且错误字段为空。源码在真实页面中的直接执行也返回 `authenticated: false`。没有使用模拟平台结果。

Facebook 修复首次通过验收的版本为 `.scratch/mac-arm-candidate-05/onecrew_lite`，可执行文件 SHA-256 为 `96de629d9f6cf570f5e48d50bcfd45667968618e929b902f5100e82e233b35fb`。账号数据目录继续为 `.scratch/mac-arm-live-data`，已通过工作台备份并更新配套只读适配器。当前运行版本及后续升级验证见 [Rust 搜索行为同步验收](18-rust-search-alignment.md)。

真实 SEA 启动、两个界面、SQLite/PGlite 与缓存完整性验收通过，1 项通过、0 项失败、0 项跳过。日志位于 `.scratch/mac-arm-facebook-sea-05.log`，完整登录检测结果位于 `.scratch/facebook-auth-fixed-live.log`。

工作台的本地会话在服务重启后需要通过新的启动票据恢复。当前断开提示未区分会话失效和服务不可用，此问题尚未修复。
