/* Shared STA helper that the vendor source tree places in wifi_softap_api.c. */

int is_hex_string(const char *data, unsigned int len)
{
    unsigned int index;

    if (data == (void *)0 || len == 0) {
        return -1;
    }
    for (index = 0; index < len; index++) {
        const char value = data[index];
        if (!((value >= '0' && value <= '9') ||
              (value >= 'A' && value <= 'F') ||
              (value >= 'a' && value <= 'f'))) {
            return -1;
        }
    }
    return 0;
}
