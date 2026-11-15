Optional 1 — CAN-to-USB Bidirectional Bridge — Han Jiayi 韩嘉仪

1. 简介
基于 DM-MC02（STM32H723VG），使用 FDCAN1 + USB CDC 实现 Classical CAN 双向桥接。

CAN1 RX: PD0，TX: PD1
CAN Bitrate: 1 Mbit/s，Sample Point: 75%
支持 11-bit Standard CAN ID，0–8 bytes 数据
保留原有 SWD、UART 功能

2. 数据路径
Host → CAN: Host → USB CDC → Command Parser → CAN TX Queue → FDCAN1 TX → CAN Bus
主机发送 TX 命令后，usb_reader 调用 parse_bridge_tx() 检查 ID、长度和数据，并生成 CAN Frame；随后由 can_writer 异步发送至 CAN 总线。

CAN → Host: CAN Bus → FDCAN1 RX → Frame Formatter → USB TX Queue → USB CDC → Host
当 CAN1 收到数据帧后，can_reader 自动调用 format_bridge_rx()，将 CAN ID、数据长度和载荷转换为文本，并通过 USB 发送给主机，无需主机主动轮询。

3. 主机协议
使用 ASCII text，以 \n 作为帧边界。

格式：DIRECTION ID LENGTH DATA
发送请求：TX 123 4 01020304
接收通知：RX 321 3 A1B2C3
空数据帧：TX 123 0 -

程序逐字节解析 USB 数据，以 \n 判断消息结束，支持一条命令跨多个 USB Packet，以及一个 USB Packet 包含多条命令。过长或格式错误的请求会被拒绝。

4. 并发与缓冲
使用 Embassy async 实现四个并发流程：
USB RX：接收并解析主机命令。
CAN TX：从队列读取 CAN 帧并发送。
CAN RX：持续接收 CAN 帧并主动转发。
USB TX：向主机发送消息。

FDCAN1 使用 FDCAN1_IT0 / FDCAN1_IT1 中断，通过 Embassy 异步驱动处理 CAN 收发，无需轮询等待。

缓冲:
CAN TX Queue：8 frames
USB TX Queue：16 messages（每条最多 63 bytes）

使用 try_send() 非阻塞入队；队列满时丢弃新消息，CAN TX 队列满时尝试返回错误。
CAN TX timeout：100 ms；USB TX timeout：200 ms。
USB 发送失败时清理输出队列并等待重连。
USB 接收端检测断连后，清理待发送的 CAN 请求。
采用 best-effort 策略，不保证完全无丢帧。错误通知可能因 USB 输出队列已满而丢失；断线后不保证所有旧消息均被清除。

5. 代码修改和检验
src/main.rs：四个异步流程、双向转发、有界队列和断连处理
src/protocol.rs：parse_bridge_tx()、format_bridge_rx()
can_bridge.py：主机端持续读取 CAN 通知及发送请求
Cargo.toml / Cargo.lock：添加 embassy-sync

cargo build --locked — Passed
python -m py_compile can_bridge.py — Passed
尚未上板测试
