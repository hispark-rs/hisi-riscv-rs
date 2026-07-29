#!/usr/bin/env bash

# Load a local HIL credential file without evaluating it as shell code.
# CI should continue to inject the two variables through its secret store.
load_ws63_wifi_credentials() {
    local file="${WS63_WIFI_ENV_FILE:-}"
    local disposition="${WS63_WIFI_ENV_FILE_DISPOSITION:-keep}"
    local mode line ssid="" passphrase=""
    local ssid_seen=0 passphrase_seen=0

    [ -n "$file" ] || return 0

    case "$disposition" in
        keep|delete) ;;
        *)
            echo "ERROR: WS63_WIFI_ENV_FILE_DISPOSITION must be keep or delete" >&2
            return 1
            ;;
    esac

    if [ -n "${WS63_WIFI_SSID:-}" ] || [ -n "${WS63_WIFI_PASSPHRASE:-}" ]; then
        echo "ERROR: credential file and direct Wi-Fi environment variables are mutually exclusive" >&2
        return 1
    fi
    if [ ! -f "$file" ] || [ -L "$file" ]; then
        echo "ERROR: Wi-Fi credential input must be a regular, non-symlink file" >&2
        return 1
    fi
    if [ ! -O "$file" ]; then
        echo "ERROR: Wi-Fi credential input must be owned by the current user" >&2
        return 1
    fi

    if mode="$(stat -f '%Lp' "$file" 2>/dev/null)"; then
        :
    elif mode="$(stat -c '%a' -- "$file" 2>/dev/null)"; then
        :
    else
        echo "ERROR: cannot inspect Wi-Fi credential file permissions" >&2
        return 1
    fi
    if [ "$mode" != 600 ]; then
        echo "ERROR: Wi-Fi credential file permissions must be 0600" >&2
        return 1
    fi

    while IFS= read -r line || [ -n "$line" ]; do
        case "$line" in
            ''|'#'*)
                ;;
            WS63_WIFI_SSID=*)
                if [ "$ssid_seen" -ne 0 ]; then
                    echo "ERROR: duplicate WS63_WIFI_SSID in credential file" >&2
                    return 1
                fi
                ssid="${line#WS63_WIFI_SSID=}"
                ssid_seen=1
                ;;
            WS63_WIFI_PASSPHRASE=*)
                if [ "$passphrase_seen" -ne 0 ]; then
                    echo "ERROR: duplicate WS63_WIFI_PASSPHRASE in credential file" >&2
                    return 1
                fi
                passphrase="${line#WS63_WIFI_PASSPHRASE=}"
                passphrase_seen=1
                ;;
            *)
                echo "ERROR: unsupported entry in Wi-Fi credential file" >&2
                return 1
                ;;
        esac
    done < "$file"

    if [ "$ssid_seen" -ne 1 ] || [ -z "$ssid" ]; then
        echo "ERROR: credential file is missing WS63_WIFI_SSID" >&2
        return 1
    fi
    if [ "$passphrase_seen" -ne 1 ] || [ -z "$passphrase" ]; then
        echo "ERROR: credential file is missing WS63_WIFI_PASSPHRASE" >&2
        return 1
    fi

    WS63_WIFI_SSID="$ssid"
    WS63_WIFI_PASSPHRASE="$passphrase"
    if [ "$disposition" = delete ]; then
        rm -f -- "$file"
    fi
    unset WS63_WIFI_ENV_FILE
    unset WS63_WIFI_ENV_FILE_DISPOSITION
}
