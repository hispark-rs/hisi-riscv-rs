#include <stddef.h>
#include "soc_wifi_api.h"

#define ABI_VALUE(name, value) const unsigned int name = (value)

ABI_VALUE(abi_ap_size, sizeof(ext_wifi_ap_info));
ABI_VALUE(abi_ap_ssid, offsetof(ext_wifi_ap_info, ssid));
ABI_VALUE(abi_ap_bssid, offsetof(ext_wifi_ap_info, bssid));
ABI_VALUE(abi_ap_auth, offsetof(ext_wifi_ap_info, auth));
ABI_VALUE(abi_ap_channel, offsetof(ext_wifi_ap_info, channel));
ABI_VALUE(abi_ap_rssi, offsetof(ext_wifi_ap_info, rssi));
ABI_VALUE(abi_ap_pairwise, offsetof(ext_wifi_ap_info, pairwise));

ABI_VALUE(abi_assoc_size, sizeof(ext_wifi_assoc_request));
ABI_VALUE(abi_assoc_ssid, offsetof(ext_wifi_assoc_request, ssid));
ABI_VALUE(abi_assoc_auth, offsetof(ext_wifi_assoc_request, auth));
ABI_VALUE(abi_assoc_key, offsetof(ext_wifi_assoc_request, key));
ABI_VALUE(abi_assoc_bssid, offsetof(ext_wifi_assoc_request, bssid));
ABI_VALUE(abi_assoc_pairwise, offsetof(ext_wifi_assoc_request, pairwise));
ABI_VALUE(abi_assoc_hex_flag, offsetof(ext_wifi_assoc_request, hex_flag));
ABI_VALUE(abi_assoc_channel, offsetof(ext_wifi_assoc_request, channel));

ABI_VALUE(abi_status_size, sizeof(ext_wifi_status));
ABI_VALUE(abi_status_ssid, offsetof(ext_wifi_status, ssid));
ABI_VALUE(abi_status_bssid, offsetof(ext_wifi_status, bssid));
ABI_VALUE(abi_status_channel, offsetof(ext_wifi_status, channel));
ABI_VALUE(abi_status_status, offsetof(ext_wifi_status, status));

ABI_VALUE(abi_event_size, sizeof(ext_wifi_event));
ABI_VALUE(abi_event_kind, offsetof(ext_wifi_event, event));
ABI_VALUE(abi_event_info, offsetof(ext_wifi_event, info));
ABI_VALUE(abi_connected_size, sizeof(event_wifi_connected));
ABI_VALUE(abi_connected_bssid, offsetof(event_wifi_connected, bssid));
ABI_VALUE(abi_connected_ssid_len, offsetof(event_wifi_connected, ssid_len));
ABI_VALUE(abi_connected_ifname, offsetof(event_wifi_connected, ifname));
ABI_VALUE(abi_disconnected_size, sizeof(event_wifi_disconnected));
ABI_VALUE(abi_disconnected_reason, offsetof(event_wifi_disconnected, reason_code));
