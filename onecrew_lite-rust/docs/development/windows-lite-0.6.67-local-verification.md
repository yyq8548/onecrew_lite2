# Windows Lite 0.6.67 本机交付验证

日期：2026-10-07（America/Los_Angeles）。远端 main：709833f73a326114040680dc6830ebeea81366d6。
原工作目录的六项未提交改动保留；独立 worktree 接入其中 Windows 首次使用、说明、NSIS 快捷方式及对应测试改动。业务发现和评分源码未修改。未提交或推送。

## 构建

CLI 0.6.67 使用正式 development/scripts/build.sh，目标 x86_64-pc-windows-gnu；本机 Rust 1.98.0、WinLibs GCC 16.1、LLVM 23。Web 和 MCP UI 均从当前源码构建。
MSVC 路径缺 cargo-xwin，使用仓库支持的原生 GNU 路径。Bindgen 需明确 GNU target、LLVM resource dir 及 MinGW include；只设置当前构建进程环境，不修改系统配置。

成功构建环境：PATH 添加 ~/.cargo/bin 和 C:/Program Files/LLVM/bin；LIBCLANG_PATH=C:/Program Files/LLVM/bin；BINDGEN_EXTRA_CLANG_ARGS 为 --target=x86_64-w64-windows-gnu、LLVM 23 resource-dir 和 WinLibs MinGW include。CARGO_TARGET_DIR 复用原 checkout 的 target，串行运行 Cargo。

CLI manifest：110301184 字节，SHA256 9c0613b7abbc37c1d70d5c0cc74db7fcd853c6370ab36b982038d39fd1b49965。
Library 0.1.4：pnpm build 后使用原生 scripts/build-binary.mjs，Node v26.7.0；Node SHA256 与 nodejs.org SHASUMS256.txt 一致。Mac-only build-release 包装器没有运行，也没有修改。
Library companion SHA256：5211b8fa4b6098ec55f1e618d9da161df6efbd17aa46a35a72143c88ad77bc5d。

## 验证和安装

- JS 频道/登录/打包相关23项通过、1项Mac测试跳过；新增Windows首次入口打包及companion安装测试2项通过、1项Mac测试跳过。无Windows symlink权限，相关lane未运行。
- Library五列导出、隐藏邮箱/账号后备优先级测试通过；HTTP/MCP、重启持久化、表格原子操作集成通过。
- NSIS安装包25项文件哈希通过；仅Windows系统PATH下两个程序可运行；中文和空格安装路径、拒绝重复覆盖、两页面与前端资源、注册/卸载通过，测试数据和无关文件保留。
- 本机使用 install-local-build.sh 和 __bootstrap-update 升级，再通过 manifest 校验安装 companion。CLI status 返回0.6.67、对应hash、startup/data ready、login active。
- 安装前后30张discovery表行摘要完全一致，没有恢复客户搜索或修改审批/预算。摘要没有导出行内容。
- 浏览器实际显示0.6.67/hash工作台、已连接扩展/已选择身份，以及Library中原有虚拟货币_1007_1表。
- opencli repair：1.8.8、15组addon、daemon/extension/connection均true；未执行平台搜索或登录检测。
- 插件显式安装 fresh/enabled：0.6.67+codex.51e3817c61054d3f86beafbaa156389dc66c8e85cd4c1b40b12dcc708b0cdd5f。

插件安装回执要求完整退出并重启Codex。当前聊天未证明加载新插件；没有启动替代stdio或新增MCP条目。后续重启后通过正式Lite MCP读回版本/hash，再在工作台确认测试条件。真实搜索全链与同事干净电脑验收仍待完成。

## 同事测试包

Downloads/onecrew_lite-0.6.67-Windows-x64-同事测试包-20261007.zip，89085685字节。
ZIP SHA256：bffa7b73bc6ac44d7ed5f8fbee526357c546a1651ecc056acf3de3beefffd72b。
仅包含安装程序、安装程序SHA256文件及先看我.txt；ZIP CRC和包内安装器hash通过。没有客户凭据、数据库、浏览器资料或开发缓存。
安装器SHA256：4317721d5d33afd9a47d5d662c143bb1e23b0e82cfd4381d3bf552bf50b871d0。
未签名内部测试，不是签名生产发行；已存在Lite时拒绝覆盖。首次使用联网准备组件及连接Codex，同事自行完成扩展、平台账号登录和计划确认。

构建及安装证据日志位于本机 C:/Temp/onecrew-067-* 与 C:/Temp/onecrew-014-*，不包含在同事包。
