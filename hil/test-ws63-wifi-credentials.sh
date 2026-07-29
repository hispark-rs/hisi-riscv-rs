#!/usr/bin/env bash
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=lib/ws63-wifi-credentials.sh
source "$HERE/hil/lib/ws63-wifi-credentials.sh"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

fail() {
    echo "FAIL: $*" >&2
    exit 1
}

write_file() {
    local path="$1" body="$2" mode="${3:-600}"
    printf '%s' "$body" > "$path"
    chmod "$mode" "$path"
}

valid="$TMP/valid.env"
write_file "$valid" $'WS63_WIFI_SSID=test network\nWS63_WIFI_PASSPHRASE=value=with spaces\n'
WS63_WIFI_ENV_FILE="$valid"
load_ws63_wifi_credentials
[ "$WS63_WIFI_SSID" = "test network" ] || fail "SSID was not preserved"
[ "$WS63_WIFI_PASSPHRASE" = "value=with spaces" ] || fail "passphrase was not preserved"
[ -e "$valid" ] || fail "persistent credential file was consumed"
[ -z "${WS63_WIFI_ENV_FILE:-}" ] || fail "credential file path remained exported"
unset WS63_WIFI_SSID WS63_WIFI_PASSPHRASE

one_shot="$TMP/one-shot.env"
write_file "$one_shot" $'WS63_WIFI_SSID=test\nWS63_WIFI_PASSPHRASE=delete-me\n'
WS63_WIFI_ENV_FILE="$one_shot"
WS63_WIFI_ENV_FILE_DISPOSITION=delete
load_ws63_wifi_credentials
[ ! -e "$one_shot" ] || fail "one-shot credential file was retained"
[ -z "${WS63_WIFI_ENV_FILE_DISPOSITION:-}" ] ||
    fail "credential disposition remained exported"
unset WS63_WIFI_SSID WS63_WIFI_PASSPHRASE

invalid_disposition="$TMP/invalid-disposition.env"
write_file "$invalid_disposition" $'WS63_WIFI_SSID=test\nWS63_WIFI_PASSPHRASE=keep-me\n'
WS63_WIFI_ENV_FILE="$invalid_disposition"
WS63_WIFI_ENV_FILE_DISPOSITION=invalid
if load_ws63_wifi_credentials 2>"$TMP/invalid-disposition.err"; then
    fail "invalid credential disposition was accepted"
fi
[ -e "$invalid_disposition" ] || fail "invalid-disposition credential file was consumed"
grep -q 'must be keep or delete' "$TMP/invalid-disposition.err" ||
    fail "invalid-disposition error was not actionable"
unset WS63_WIFI_ENV_FILE WS63_WIFI_ENV_FILE_DISPOSITION

insecure="$TMP/insecure.env"
write_file "$insecure" $'WS63_WIFI_SSID=test\nWS63_WIFI_PASSPHRASE=not-a-secret\n' 644
WS63_WIFI_ENV_FILE="$insecure"
if load_ws63_wifi_credentials 2>"$TMP/insecure.err"; then
    fail "insecure permissions were accepted"
fi
[ -e "$insecure" ] || fail "rejected credential file was consumed"
grep -q 'permissions must be 0600' "$TMP/insecure.err" || fail "permission error was not actionable"
unset WS63_WIFI_ENV_FILE

target="$TMP/target.env"
link="$TMP/link.env"
write_file "$target" $'WS63_WIFI_SSID=test\nWS63_WIFI_PASSPHRASE=not-a-secret\n'
ln -s "$target" "$link"
WS63_WIFI_ENV_FILE="$link"
if load_ws63_wifi_credentials 2>"$TMP/link.err"; then
    fail "symlink credential input was accepted"
fi
grep -q 'regular, non-symlink file' "$TMP/link.err" || fail "symlink error was not actionable"
unset WS63_WIFI_ENV_FILE

duplicate="$TMP/duplicate.env"
write_file "$duplicate" $'WS63_WIFI_SSID=one\nWS63_WIFI_SSID=two\nWS63_WIFI_PASSPHRASE=not-a-secret\n'
WS63_WIFI_ENV_FILE="$duplicate"
if load_ws63_wifi_credentials 2>"$TMP/duplicate.err"; then
    fail "duplicate field was accepted"
fi
grep -q 'duplicate WS63_WIFI_SSID' "$TMP/duplicate.err" || fail "duplicate error was not actionable"
unset WS63_WIFI_ENV_FILE

ambiguous="$TMP/ambiguous.env"
write_file "$ambiguous" $'WS63_WIFI_SSID=file-value\nWS63_WIFI_PASSPHRASE=file-value\n'
WS63_WIFI_ENV_FILE="$ambiguous"
WS63_WIFI_SSID="direct-value"
if load_ws63_wifi_credentials 2>"$TMP/ambiguous.err"; then
    fail "ambiguous credential sources were accepted"
fi
grep -q 'mutually exclusive' "$TMP/ambiguous.err" || fail "ambiguity error was not actionable"
unset WS63_WIFI_ENV_FILE WS63_WIFI_SSID

unknown="$TMP/unknown.env"
write_file "$unknown" $'WS63_WIFI_SSID=test\nWS63_WIFI_PASSPHRASE=redacted-fixture\nSHELL_COMMAND=do-not-run\n'
WS63_WIFI_ENV_FILE="$unknown"
if load_ws63_wifi_credentials 2>"$TMP/unknown.err"; then
    fail "unsupported entry was accepted"
fi
grep -q 'unsupported entry' "$TMP/unknown.err" || fail "unsupported-entry error was not actionable"
if grep -qE 'redacted-fixture|unknown\.env' "$TMP/unknown.err"; then
    fail "credential value or path leaked into diagnostics"
fi

help_file="$TMP/help.env"
write_file "$help_file" $'WS63_WIFI_SSID=test\nWS63_WIFI_PASSPHRASE=not-a-secret\n'
WS63_WIFI_ENV_FILE="$help_file" "$HERE/hil/ws63-connectivity-smoke.sh" --help >"$TMP/help.out"
[ -e "$help_file" ] || fail "--help consumed the credential file"

WS63_WIFI_ENV_FILE="$help_file" "$HERE/hil/ws63-a5b-response-bound.sh" --help \
    >"$TMP/a5b-help.out"
[ -e "$help_file" ] || fail "A5B --help consumed the credential file"

echo "WS63 Wi-Fi credential contract: PASS"
