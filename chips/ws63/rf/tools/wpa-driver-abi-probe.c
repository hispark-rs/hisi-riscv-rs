#include <stddef.h>

#include "utils/common.h"
#include "driver_soc_common.h"

#define ABI_VALUE(name, value) const unsigned int name = (value)

ABI_VALUE(abi_scan_ssid_size, sizeof(ext_driver_scan_ssid_stru));
ABI_VALUE(abi_scan_size, sizeof(ext_scan_stru));
ABI_VALUE(abi_scan_freqs, offsetof(ext_scan_stru, freqs));
ABI_VALUE(abi_scan_num_ssids, offsetof(ext_scan_stru, num_ssids));
ABI_VALUE(abi_scan_extra_ies_len, offsetof(ext_scan_stru, extra_ies_len));

ABI_VALUE(abi_crypto_size, sizeof(ext_crypto_settings_stru));
ABI_VALUE(abi_crypto_pairwise, offsetof(ext_crypto_settings_stru, ciphers_pairwise));
ABI_VALUE(abi_crypto_akm_count, offsetof(ext_crypto_settings_stru, n_akm_suites));
ABI_VALUE(abi_crypto_akm, offsetof(ext_crypto_settings_stru, akm_suites));
ABI_VALUE(abi_crypto_sae_pwe, offsetof(ext_crypto_settings_stru, sae_pwe));

ABI_VALUE(abi_driver_assoc_size, sizeof(ext_associate_params_stru));
ABI_VALUE(abi_driver_assoc_auth, offsetof(ext_associate_params_stru, auth_type));
ABI_VALUE(abi_driver_assoc_freq, offsetof(ext_associate_params_stru, freq));
ABI_VALUE(abi_driver_assoc_crypto, offsetof(ext_associate_params_stru, crypto));

ABI_VALUE(abi_connect_result_size, sizeof(ext_connect_result_stru));
ABI_VALUE(abi_connect_result_bssid, offsetof(ext_connect_result_stru, bssid));
ABI_VALUE(abi_connect_result_status, offsetof(ext_connect_result_stru, status));
ABI_VALUE(abi_connect_result_freq, offsetof(ext_connect_result_stru, freq));

ABI_VALUE(abi_disconnect_size, sizeof(ext_disconnect_stru));
ABI_VALUE(abi_disconnect_reason, offsetof(ext_disconnect_stru, reason));
ABI_VALUE(abi_disconnect_ie_len, offsetof(ext_disconnect_stru, ie_len));
