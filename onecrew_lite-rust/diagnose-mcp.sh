#!/usr/bin/env bash
# 在 Mac 上跑:  bash diagnose-mcp.sh
# 作用:抓「启动本地连接一直转圈」的真凶(panic / keychain 阻塞 / 端口占用)
set -u

APP="onecrew-local-desktop"
PORT=39393
OUT="/tmp/onecrew-diag"
mkdir -p "$OUT"
LOG="$OUT/stderr.log"
SAMPLE="$OUT/sample.txt"
: > "$LOG"

line() { printf '\n============ %s ============\n' "$1"; }

# 1) 定位 .app 里的可执行文件
line "1. 定位二进制"
BIN=""
# 优先用脚本所在目录推断 target
HERE="$(cd "$(dirname "$0")" && pwd)"
for base in "$HERE/target" "$HOME/mutagen-projects/ubuntu-home/home/code/kolx/onecrew-local/target" "$HERE"; do
  [ -z "$BIN" ] || break
  BIN="$(find "$base" -type f -name "$APP" -path '*MacOS*' 2>/dev/null | head -n1)"
done
if [ -z "$BIN" ]; then
  echo "没找到 $APP 的可执行文件。请确认已经 build 过(cargo tauri build / npm run tauri build)。"
  exit 1
fi
echo "BIN = $BIN"
echo "签名信息:"
codesign -dvvv "$BIN" 2>&1 | grep -Ei 'Identifier|Signature|TeamIdentifier|Authority|flags=' | sed 's/^/  /'

# 2) 退掉正在跑的实例 + 查端口
line "2. 清场 & 端口检查"
pkill -x "$APP" 2>/dev/null && echo "已退掉旧实例" || echo "没有在跑的旧实例"
sleep 1
echo "端口 $PORT 占用情况(启动前):"
lsof -nP -iTCP:"$PORT" 2>/dev/null | sed 's/^/  /' || true
[ -z "$(lsof -nP -iTCP:$PORT 2>/dev/null)" ] && echo "  (空,端口未被占用)"

# 3) 带 backtrace 从终端启动,stderr 全程录进日志
line "3. 启动 App(stderr → $LOG)"
RUST_BACKTRACE=full RUST_LOG=debug "$BIN" >>"$LOG" 2>&1 &
PID=$!
echo "已启动,PID = $PID"
sleep 2
if ! kill -0 "$PID" 2>/dev/null; then
  echo "!!! 进程启动后立刻退出了,直接看日志:"
  cat "$LOG"
  exit 0
fi

# 4) 等你去点「启动本地连接 → 继续」
line "4. 现在请操作"
echo ">>> 切到 App 窗口,点『启动本地连接』→『继续并打开系统授权』。"
echo ">>> 如果弹出 Keychain 授权框,照常点『始终允许 / 允许』。"
echo ">>> 等它开始转圈(或报错、或成功)之后,回到这里按【回车】继续采样。"
read -r _

# 5) 采样(抓转圈现场)+ 端口复查
line "5. 采样中(3 秒)"
if kill -0 "$PID" 2>/dev/null; then
  sample "$PID" 3 -mayDie -file "$SAMPLE" >/dev/null 2>&1
  echo "采样写入 $SAMPLE"
else
  echo "!!! 进程已经不在了(可能整个 crash 退出)。看日志即可。"
fi
echo "端口 $PORT 占用情况(启动后 —— 若已 LISTEN 说明 bind 成功、卡在更后面):"
lsof -nP -iTCP:"$PORT" 2>/dev/null | sed 's/^/  /' || true
[ -z "$(lsof -nP -iTCP:$PORT 2>/dev/null)" ] && echo "  (空 —— bind 没成功 / 没走到 bind)"

# 6) 汇总输出
line "6-A. STDERR / panic 日志(最重要)"
if grep -qiE 'panic|backtrace|error|thread .* panicked' "$LOG"; then
  grep -niE 'panic|backtrace|thread .* panicked|error|unwrap|expected|left:|right:' "$LOG" | sed 's/^/  /'
  echo "  --- 完整日志见: $LOG ---"
else
  echo "  (日志里没有 panic/error 关键字)"
  echo "  完整日志尾部:"
  tail -n 40 "$LOG" | sed 's/^/  /'
fi

line "6-B. 关键线程栈(是否卡在 keychain / bind / 我方代码)"
if [ -f "$SAMPLE" ]; then
  echo "-- 命中 Security/Keychain/securityd 的帧:"
  grep -niE 'SecItem|securityd|Keychain|generic_password|SecKeychain' "$SAMPLE" | grep -viE 'KeychainCircle' | sed 's/^/  /' || echo "  (无 —— 说明没卡在 keychain 调用)"
  echo "-- 命中 bind/accept/listen 的帧:"
  grep -niE 'TcpListener|::bind|tcp_bind|accept' "$SAMPLE" | sed 's/^/  /' || echo "  (无)"
  echo "-- 我方 crate 正在执行的帧(agent_platform / local_mcp / platform_runtime / local_vault):"
  grep -niE 'agent_platform|local_mcp|platform_runtime|local_vault|start_mcp|LocalMcp|LocalVault|VaultCipher' "$SAMPLE" | sed 's/^/  /' || echo "  (无 —— 我方代码没有活跃栈,倾向 panic 后任务已消失)"
  echo "-- blocking pool 线程是否存在(spawn_blocking 是否还挂着):"
  grep -niE 'blocking|spawn_blocking' "$SAMPLE" | head -n 20 | sed 's/^/  /' || echo "  (无 blocking 线程)"
else
  echo "  (没有采样文件)"
fi

line "完成"
echo "把上面 6-A 和 6-B 两段整段贴回给我。"
echo "原始文件: 日志=$LOG  采样=$SAMPLE"
