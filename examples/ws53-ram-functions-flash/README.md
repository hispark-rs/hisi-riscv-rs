# 候选19：专用 ITCM 的自主 Flash 启动验收

独立 workspace，启用运行时 `ws53-ram-functions`。FlashBoot 入口初始化 memory
mode、DTCM 及专用 ITCM 代码/表；main 不等待调试器，先核对源/副本，再等待30秒
串口重连/分析仪观察窗口。保留控制核复位，夹具显式取得 UART2、TIMER、WDT 和
MGPIO17 的应用侧测试所有权，不宣称通用控制核并行协作。

三个不内联 Rust 函数（run → root → leaf）和4字只读表全部在专用 ITCM。
1024/4096/16384次迭代由 RAM 中的 run 完成，不在每次迭代跳回 Flash。独立的
Flash 参考算法及宿主算法核对结果；64状态字同时经串口输出并可只读核对。
DSLogic D5=MGPIO17 的三个脉冲包围三次运行，TIMER1计数提供独立区间对照。
结束释放/恢复pad、停timer，复核代码/常量副本和缓存配置未变，然后静默。

构建与检查在父工程根目录运行：

```sh
uv run --script scripts/ws53_ram19_build.py --output target/ram19-build
```

先运行同一ELF的源绑定模型和离线扇区规划，实板必须先复核探针23121302、串口
5CB7019921及SWD健康。实际烧录须保存现场4MiB、保留NV、只写规划的应用页和
组合头、整片回读、分析仪READY后才BOOT，并备好同页原厂恢复及回读。不得修改
eFuse、安全配置、控制核固件或校准数据。冷启动另验，nRESET不能替代真正断电。

这不是SFC擦写函数闭包：正常陷阱/NMI、日志、测量和启动仍可执行Flash代码。
不关闭Flash/XIP，不配置缓存/PMP，不以本测试授予DMA一致性或Flash排空资格。
UART2 AGPIO1/2无板端测点，串口日志不是其电气波形证明。当前结果以实施记录
中的具名QEMU、Flash、复位及冷启动记录为准，不继承旧布局的验收。
