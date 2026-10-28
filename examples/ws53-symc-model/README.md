# WS53 SYMC/KM CPU 模型测试

本程序只用于显式启用SYMC/KM配置的QEMU模型，**不得部署到实板**。它通过生产HAL
装载合成密钥、准备整块传输、提交和等待。测试命令通过普通RAM邮箱传入；主机不
配置MMIO、不预填结果、不替CPU访问PAC。测试镜像使用独立workspace和锁文件。

在主工程根目录运行：

```sh
cargo build --locked -Zbuild-std=core,alloc \
  --manifest-path examples/ws53-symc-model/Cargo.toml \
  --features model-handoff --release --target riscv32imfc-unknown-none-elf
uv run --script scripts/test-ws53-symc-model.py \
  --qemu ../hisi-riscv-qemu/qemu/build/qemu-system-riscv32 \
  --reference ../hisi-riscv-qemu/scripts/ws53_symc_reference.py \
  --elf examples/ws53-symc-model/target/riscv32imfc-unknown-none-elf/release/symc-model \
  --output target/ws53-symc-cpu
```

QEMU须包含本次SYMC模型；主工作区旧版本可能尚未切换，应将`--qemu`与`--reference`
指向经过构建核对的隔离工作区。输出目录必须不存在，以保留每轮原始记录。运行器
检查RAM边界、ABI、栈及原子指令，并保存ELF、QEMU和参考脚本校验值。

154个场景包含两路通道、四种算法/长度、五种模式、双向/even/odd及多种长度的
120次运算和34次拒绝/故障/取消/丢弃测试。完整输入、输出、IV、保护字节及描述符
逐一核对，终态和取消后的MMIO计数不得增加，KM槽不得被隐式释放。使用公开生成的
测试字节，AES与SM4分别对照PyCryptodomex与gmssl；不读取真实板卡密钥。

CPU从不调用`release_after_external_quiescence`，输出内存始终留在传输资源中；
只有模型主机在已知总线状态下观察RAM。软件超时、取消、Drop和forget不停止模型
已经接受的请求。本测试不建立真实芯片的缓存一致性、跨核交接或物理排空条件，
也不替代RAM、Flash及冷启动验收。
