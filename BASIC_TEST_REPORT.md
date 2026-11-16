HerKules L4 — Basic Hardware Test Report

1. Pin Mapping（引脚映射）
MCU: STM32H723VG (DM-MC02)
根据 src/wiring.rs 和 DM-MC02 官方原理图，使用以下引脚配置：
Signal    |  MCU GPIO  |  Connector Pin
SWCLK     |  PE9       |  P11.3
SWDIO     |  PE13      |  P11.4
UART7 TX  |  PE8       |  P13.4
UART7 RX  |  PE7       |  P13.3

SWD Self-test Connections：
PE9 (P11 Channel 3 Signal) → P1.3 (SWCLK)
PE13 (P11 Channel 4 Signal) → P1.4 (SWDIO)

UART7 Configuration：
Peripheral: UART7
Baud Rate: 115200
Data Format: 8N1
Interrupt: UART7
TX/RX Buffers: 128 bytes each

2. Hardware Test Procedure（硬件测试流程）
断开 SWD 回接跳线。
使用外置 probe 执行 cargo build --locked 和 cargo run --locked，编译并烧录 DM-MC02。
烧录完成后断电，拔掉外置 probe。
根据 DM-MC02 管脚标注图，使用两根信号线完成同板 SWD 自测接线。
重新通过 USB-C 连接电脑，使用 host.py 测试 USB CDC 和 SWD。
对 UART7 TX/RX 进行回接，运行 UART loopback test。

3. Test Commands & Results（测试命令及结果）
Command                              |  Recorded / Reported Output      |  Status
probe-rs list                        |  ExpecteDM-MC02 CMSIS-DAP Demo   |  PASS
python host.py COM6 ping             |  OK DM-MC02                      |  PASS
python host.py COM6 swd              |  OK DPIDR 6BA02477               |  PASS
python host.py COM6 swd              |  OK DPIDR 6BA02477               |  PASS
python host.py COM6 ping             |  OK DM-MC02                      |  PASS
python host.py COM7 uart-loopback    |  OK UART loopback                |  PASS

4. Verified Configuration（实测确认的配置）
SWD GPIO Mapping
根据原始 wiring.rs：
SWCLK: PE9，GPIO Output，initially Low
SWDIO: PE13，GPIO Flex，bidirectional
连接至同板 SWD 接口后，连续两次运行 SWD ID，参考测试记录均返回：OK DPIDR 6BA02477
两次结果一致，且测试前后的 PING 均返回 OK DM-MC02。
这与 SWD 引脚配置正常工作、USB 控制接口在 SWD 操作后仍可通信的结果一致。

5. Failed / Untested Items（失败及未确认项目）
NONE
