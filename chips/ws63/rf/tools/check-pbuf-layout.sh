#!/usr/bin/env bash
# Compile layout assertions with the exact WS63 SDK RISC-V compiler/headers.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
FBB_WS63="${FBB_WS63:-/Users/sanchuan/Documents/hispark/fbb_ws63/src}"
CC="${WS63_GCC:-$FBB_WS63/tools/bin/compiler/riscv/cc_riscv32_musl_105/cc_riscv32_musl_fp/bin/riscv32-linux-musl-gcc}"
OUT="$ROOT/target/ws63-layout-oracle"
mkdir -p "$OUT/include/asm"

# Only the type width is relevant to these layouts. Avoid pulling the entire
# LiteOS kernel configuration into this narrow header oracle.
cat > "$OUT/include/asm/atomic.h" <<'EOF'
typedef volatile int atomic_t;
EOF
cat > "$OUT/include/securec.h" <<'EOF'
/* layout oracle: declarations are not needed */
EOF
cat > "$OUT/layout.c" <<'EOF'
#include <stddef.h>
typedef unsigned int sys_prot_t;
#include "lwip/opt.h"
#include "lwip/pbuf.h"
#include "lwip/netif.h"

#define CHECK(type, field, value) _Static_assert(offsetof(type, field) == value, #type "." #field)
_Static_assert(sizeof(struct pbuf) == 32, "struct pbuf size");
CHECK(struct pbuf, payload, 4);
CHECK(struct pbuf, len, 10);
CHECK(struct pbuf, malloc_len, 16);
CHECK(struct pbuf, type_internal, 18);
CHECK(struct pbuf, flags, 20);
CHECK(struct pbuf, ref, 24);
CHECK(struct pbuf, if_idx, 28);
CHECK(struct pbuf, priority, 29);
CHECK(struct netif, state, 240);
CHECK(struct netif, drv_send, 244);
CHECK(struct netif, drv_set_hwaddr, 248);
CHECK(struct netif, hwaddr, 268);
CHECK(struct netif, hwaddr_len, 274);
CHECK(struct netif, name, 284);
int main(void) { return 0; }
EOF

ARGS=(
  -c -Os -DLWIP_CONFIG_FILE='"lwip/lwipopts_default.h"'
  -I"$OUT/include"
  -I"$FBB_WS63/open_source/lwip/lwip_adapter/liteos_207/src/include"
  -I"$FBB_WS63/open_source/lwip/lwip_v2.1.3/src/include"
  -I"$FBB_WS63/kernel/liteos/liteos_v208.5.0/Huawei_LiteOS/compat/linux/include"
  -o "$OUT/layout.o" "$OUT/layout.c"
)
if [[ "$(uname -s)" == Darwin ]]; then
  orb -m "${ORB_MACHINE:-dev}" "$CC" "${ARGS[@]}"
else
  "$CC" "${ARGS[@]}"
fi
echo "verified WS63 pbuf/netif SDK layout"
