/*
 * Fail-closed ABI endpoints for AP code paths that the vendor combined with
 * its STA supplicant. The wifi-wpa2-personal Rust API cannot construct an AP
 * interface, so reaching one of these functions is an unsupported-mode fault.
 * Keep this file header-free: these are private vendor ABI symbols, not a new
 * public C API.
 */

void *g_hapd;

int hostapd_main(const char *ifname)
{
    (void)ifname;
    return -1;
}

void *hostapd_get_interfaces(void)
{
    return (void *)0;
}

void hostapd_pre_quit(void *interfaces)
{
    (void)interfaces;
}

void hostapd_global_deinit(void)
{
}

void hostapd_global_interfaces_deinit(void)
{
}

void hostapd_event(void *context, int event, void *data)
{
    (void)context;
    (void)event;
    (void)data;
}

/* WPA1/TKIP is outside the WPA2-Personal/CCMP profile. */
int hmac_md5(const unsigned char *key, unsigned int key_len,
    const unsigned char *data, unsigned int data_len, unsigned char *mac)
{
    (void)key;
    (void)key_len;
    (void)data;
    (void)data_len;
    (void)mac;
    return -1;
}
