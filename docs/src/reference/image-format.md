# 应用镜像格式与签名

WS63 app 镜像（flashboot 从 flash `0x230000` 加载的对象）的字段布局。事实逐字段取自 [`hisi-fwpkg/crates/hisi-fwpkg/src/image.rs`](https://github.com/hispark-rs/hisi-fwpkg) 与 [`src/fwpkg.rs`](https://github.com/hispark-rs/hisi-fwpkg)。所有多字节字段为**小端**。

构建命令见 [打包成可启动镜像](../how-to/package-image.md)；安全启动原理见 [安全启动与签名](../explanation/secure-boot.md)。

## 整体布局

```text
+--------------------------------------+ 0x000
| image_key_area_t   (0x100 字节)      |  magic = 0x4B0F2D1E
+--------------------------------------+ 0x100
| image_code_info_t  (0x200 字节)      |  magic = 0x4B0F2D2D
+--------------------------------------+ 0x300  = APP_IMAGE_HEADER_LEN
| code body（原始 .text/.rodata/...）  |  链接到 0x230300 运行
+--------------------------------------+
```

0x300 字节前缀为**定长**镜像头。secure boot 关闭（efuse `SEC_VERIFY_ENABLE == 0`）时，flashboot 的 `verify_image_*` 在检查任何签名/body hash **之前**短路成功，故签名字段从不被读。关键只有两点：头恰好 0x300 字节、其后是链接到 `0x230300` 的真实代码。

## 常量

| 常量 | 值 |
|------|----|
| `APP_KEY_AREA_IMAGE_ID` | `0x4B0F_2D1E` |
| `APP_CODE_INFO_IMAGE_ID` | `0x4B0F_2D2D` |
| `KEY_AREA_LEN` | `0x100` |
| `CODE_INFO_LEN` | `0x200` |
| `IMAGE_HEADER_LEN` | `0x300` |
| `STRUCTURE_VERSION` | `0x0001_0000` |
| `SIG_LEN`（`BOOT_SIG_LEN`） | `0x40` |
| `KEY_ALG_ECC256` | `0x2A13_C812`（ECC256 / brainpoolP256r1） |
| `ECC_CURVE_BP256R1` | `0x2A13_C812` |
| `PUB_KEY_LEN`（`BOOT_PUBLIC_KEY_LEN`） | `0x40` |
| `FLASH_NO_ENCRY_FLAG` | `0x3C78_96E1` |
| `HASH_LEN` | `32`（SHA-256） |

## 密钥区 `image_key_area_t`（偏移 0x000，长度 0x100）

| 偏移 | 字段 | 默认值 |
|------|------|--------|
| `0x00` | `image_id`（magic） | `0x4B0F_2D1E` |
| `0x04` | `structure_version` | `0x0001_0000` |
| `0x08` | `structure_length` | `0x100` |
| `0x0C` | `signature_length` | `0x40` |
| `0x10` | `key_owner_id` | `1`（默认） |
| `0x14` | `key_id` | `1`（默认） |
| `0x18` | `key_alg` | `0x2A13_C812` |
| `0x1C` | `ecc_curve_type` | `0x2A13_C812` |
| `0x20` | `key_length` | `0x40` |
| `0x24` | `key_version_ext` | `0`（disabled 板） |
| `0x28` | `mask_key_version_ext` | `0` |
| `0x2C` | `msid_ext` | `0` |
| `0x30` | `mask_msid_ext` | `0` |
| `0x34` | `maintenance_mode` | `0`（关闭） |
| `0x38`..`0x48` | `die_id[16]` | dummy 0（仅维护模式检查） |
| `0x48` | `code_info_addr` | `0`（紧随其后） |
| — | `ext_public_key_area[0x40]`、`sig_key_area[0x40]` | **dummy 0**（ECC 签名/公钥 blob） |

## 代码信息区 `image_code_info_t`（偏移 0x100，长度 0x200；下表偏移相对区起点）

| 偏移 | 字段 | 默认值 |
|------|------|--------|
| `0x00` | `image_id`（magic） | `0x4B0F_2D2D` |
| `0x04` | `structure_version` | `0x0001_0000` |
| `0x08` | `structure_length` | `0x200` |
| `0x0C` | `signature_length` | `0x40` |
| `0x10` | `version_ext` | `0` |
| `0x14` | `mask_version_ext` | `0` |
| `0x18` | `msid_ext` | `0` |
| `0x1C` | `mask_msid_ext` | `0` |
| `0x20` | `code_area_addr` | `0`（紧随头之后） |
| `0x24` | `code_area_len` | `body.len()` |
| `0x28`..`0x48` | `code_area_hash[32]` | **body 的真实 SHA-256** |
| `0x48` | `code_enc_flag` | `0x3C78_96E1`（`FLASH_NO_ENCRY_FLAG`，未加密） |
| `0x4C`..`0x5C` | `protection_key_l1[16]` | 0（加密关闭） |
| `0x5C`..`0x6C` | `protection_key_l2[16]` | 0 |
| `0x6C`..`0x7C` | `iv[16]` | 0 |
| `0x7C` | `code_compress_flag` | `0`（未压缩） |
| `0x80` | `code_uncompress_len` | `= code_area_len` |
| `0x84` | `text_segment_size` | `0x0001_0000`（默认，仅信息） |
| — | `sig_code_info[0x40]` + `sig_code_info_ext[0x40]` | **dummy 0** |

> `code_area_hash`（区内偏移 `0x28`，即镜像绝对偏移 `0x128`）是 body 的**真实 SHA-256**，与厂商 `sign_tool` 一致。
>
> `code_enc_flag`（区内 `0x48`，绝对 `0x148`）= `0x3C7896E1` 是非零哨兵：flashboot `ws63_flash_encrypt_config()` 做 `if (code_enc_flag == FLASH_NO_ENCRY_FLAG) return;`，故**零值反而**会令 flashboot 尝试配置 on-the-fly 解密而启动失败。明文镜像必须用此值。
>
> dummy-zero 字段：两个区的 ECC 签名 blob（`sig_key_area`、`sig_code_info`、`sig_code_info_ext`）与公钥（`ext_public_key_area`、`die_id`、protection key、iv）。

## fwpkg V1 容器（`fwpkg.rs`）

"all-in-one" 固件包：小头 + 每分区描述符表 + 串接的分区负载。布局是 `hisiflash` 解析器的逆，匹配厂商 `packet_create.py` `create_allinone()`。

```text
+----------------------------------+ 0x000
| FWPKG_HEAD (12 字节)             |  flag(4) crc(2) cnt(2) total_len(4)
+----------------------------------+ 0x00C
| IMAGE_INFO[0] (52 字节)          |  name[32] off(4) len(4) burn_addr(4)
| IMAGE_INFO[1] ...                |           burn_size(4) type(4)
+----------------------------------+
| payload[0] || 16 个 0 字节       |
| payload[1] || 16 个 0 字节       |
+----------------------------------+
```

### 常量

| 常量 | 值 |
|------|----|
| `FWPKG_MAGIC_V1`（`flag`） | `0xEFBE_ADDF` |
| `HEADER_SIZE`（`FWPKG_HEAD`） | `12` |
| `BIN_INFO_SIZE`（`IMAGE_INFO`） | `52` |
| `NAME_SIZE`（名字字段宽） | `32`（名字须 < 32 字节） |
| `PAYLOAD_SEPARATOR` | `16`（每负载后补的 0 字节数） |

### `FWPKG_HEAD`（12 字节）

| 偏移 | 字段 | 大小 | 值 |
|------|------|------|----|
| `0x00` | `flag`（magic） | 4 | `0xEFBE_ADDF` |
| `0x04` | `crc` | 2 | CRC16/XMODEM |
| `0x06` | `cnt`（分区数） | 2 | `parts.len()` |
| `0x08` | `total_len` | 4 | 头 + 全部负载 + 全部分隔 |

### `IMAGE_INFO`（每分区 52 字节）

| 偏移 | 字段 | 大小 |
|------|------|------|
| `0x00` | `name[32]` | 32 |
| `0x20` | `offset` | 4 |
| `0x24` | `length`（负载字节数，**不含** 16 字节分隔） | 4 |
| `0x28` | `burn_addr` | 4 |
| `0x2C` | `burn_size` | 4 |
| `0x30` | `type` | 4 |

### 分区 `type`

| 类型 | 值 | 说明 |
|------|----|----|
| `Loader` | `0` | LoaderBoot（一级加载器） |
| `Normal` | `1` | ssb / flashboot / nv / params / app … |
| `KvNv` | `2` | Key-Value NV |
| `Efuse` | `3` | eFuse 配置 |
| `Other(v)` | `v` | 其它原始类型码 |

### CRC

`crc` = **CRC16/XMODEM**（poly `0x1021`，init `0x0000`），覆盖范围为从偏移 6（`cnt` 字段）到描述符表末尾的字节（即 `out[6..head_len]`，`head_len = 12 + cnt*52`）。已知向量：`crc16_xmodem("123456789") == 0x31C3`。

> 每个负载后跟 **16 个 0 字节**分隔，计入 `total_len` 但**不计入**描述符 `length`。
