#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Verify exact-source CI and downloaded NET0 origin report bytes."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import subprocess
import zipfile

SOURCE = 'f04b701bc0b859e1370c14d65c429152a87b2fcc'
RUN = 34438207077


def gh(*args):
    return subprocess.check_output(['gh', *args])


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if not __debug__:
        parser.error('Evidence assertions must remain enabled')
    ci = json.loads(gh('run', 'view', str(RUN), '--repo', 'hispark-rs/hisi-rf-ws63',
                       '--json', 'headSha,status,conclusion,url,jobs'))
    assert ci['headSha'] == SOURCE
    if ci['status'] != 'completed' or ci['conclusion'] != 'success':
        raise SystemExit('Exact-source CI has not passed; no acceptance receipt written')
    assert len(ci['jobs']) == 23 and all(job['conclusion'] == 'success' for job in ci['jobs'])
    listing = json.loads(gh('api', f'repos/hispark-rs/hisi-rf-ws63/actions/runs/{RUN}/artifacts?per_page=100'))
    assert len(listing['artifacts']) == listing['total_count']
    artifacts = {item['name']: item for item in listing['artifacts']}
    expected = {'consumer.json', 'cleanup.json', 'storage.json', 'host-tx.json', 'rx-stop.json',
                'rx-mode.json', 'rx-origin.json', 'consumer.Cargo.toml', 'consumer.Cargo.lock'}
    records, reference = [], None
    for host in ('ubuntu-latest', 'macos-14', 'windows-latest'):
        item = artifacts['net0-external-consumer-' + host]
        assert not item['expired'] and item['workflow_run']['head_sha'] == SOURCE
        raw = gh('api', f"repos/hispark-rs/hisi-rf-ws63/actions/artifacts/{item['id']}/zip")
        assert 'sha256:' + sha(raw) == item['digest']
        with zipfile.ZipFile(io.BytesIO(raw)) as archive:
            assert set(archive.namelist()) == expected and len(archive.namelist()) == len(expected)
            files = {name: archive.read(name) for name in expected}
        consumer = json.loads(files['consumer.json'])
        origin = json.loads(files['rx-origin.json'])
        assert consumer['status'] == origin['status'] == 'pass'
        assert all(consumer[key] is True for key in ('clean_offline', 'incremental_unchanged',
            'restored_build', 'space_unicode_path', 'pinned_dependencies_unchanged',
            'rx_origin_link_verified', 'missing_rx_origin_metadata_rejected', 'missing_free_registration_metadata_rejected'))
        assert not consumer['consumer_build_script'] and not consumer['consumer_wrap_flags']
        assert consumer['consumer_lock_sha256'] == sha(files['consumer.Cargo.lock'])
        assert origin['schema'] == 'net0-rx-origin-link/v2' and origin['rejected_mutations'] == 16
        assert origin['metadata']['bytes'] == 384 and origin['metadata']['slots'] == 16
        assert origin['metadata']['packet_payload_bytes'] == 0
        assert origin['metadata']['original_pointer_bytes'] == 4
        assert origin['free_registration']['callback'] == 249
        assert origin['registration_veneer']['rom_address'] == 0x128d4a
        assert len(origin['edges']) == 6 and len(origin['patches']) == 3
        parity = {'edges': [(e['source'], e['target']) for e in origin['edges']],
                  'patches': [(p['original'], p['original_address'], p['index']) for p in origin['patches']],
                  'veneer': origin['veneer'], 'metadata_bytes': origin['metadata']['bytes']}
        if reference is not None:
            assert reference == parity
        reference = parity
        records.append({'host': host, 'id': item['id'], 'expires_at': item['expires_at'],
            'downloaded_zip_sha256': sha(raw), 'github_digest': item['digest'],
            'origin_elf_sha256': origin['elf_sha256'],
            'files': [{'name': name, 'sha256': sha(data), 'bytes': len(data)} for name, data in sorted(files.items())]})
    result = {'schema': 'net0-rx-origin-free-downloads/v1', 'source': SOURCE, 'ci_url': ci['url'],
              'successful_jobs': 23, 'artifacts': records, 'semantic_parity': reference,
              'boundary': 'Downloaded report ZIPs, not firmware or registry-only facade acceptance; finite Actions retention'}
    args.output.write_text(json.dumps(result, indent=2) + '\n')
    (args.output.parent / 'SHA256SUMS').write_text(''.join(
        f'{sha(path.read_bytes())}  {path.name}\n'
        for path in sorted(args.output.parent.iterdir())
        if path.is_file() and path.name != 'SHA256SUMS'))
    print(json.dumps({'verified_zips': len(records), 'member_digests': sum(len(r['files']) for r in records)}))


if __name__ == '__main__':
    main()
