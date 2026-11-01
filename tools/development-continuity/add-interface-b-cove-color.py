#!/usr/bin/env python3
"""Closed-app v1 -> v2 cutover: independent saved Interface B cove color.

The app reads only v2. This converter seeds the CURRENT resolved lighting color;
it cannot reconstruct a historical independent color that was never stored.
"""
import argparse
import copy
import hashlib
import json
import math
from pathlib import Path
import plistlib
import subprocess

KEY = 'csMailInterfaceBAppearanceDocument'

def preferences(domain):
    return plistlib.loads(subprocess.check_output(['defaults', 'export', domain, '-']))

def fingerprint(data):
    return hashlib.sha256(data).hexdigest()

def add_color(material):
    assert 'coveColor' not in material
    texture = material['texture']
    color = (texture['coloredFlecks']['pigment'] if 'coloredFlecks' in texture
             else {'hue': 0, 'saturation': 0, 'brightness': 0.53})
    assert set(color) == {'hue', 'saturation', 'brightness'}
    assert all(isinstance(v, (int, float)) and math.isfinite(v) and 0 <= v <= 1 for v in color.values())
    material['coveColor'] = copy.deepcopy(color)

def convert(document):
    assert document['version'] == 1
    result = copy.deepcopy(document)
    for profile in result['profiles']:
        add_color(profile['appearance']['material'])
    for kind in ['user', 'unsaved']:
        if kind in result['selection']:
            add_color(result['selection'][kind]['working'])
    result['version'] = 2
    # Removing only the new field/version must reproduce every old value.
    projection = copy.deepcopy(result)
    for profile in projection['profiles']:
        del profile['appearance']['material']['coveColor']
    for kind in ['user', 'unsaved']:
        if kind in projection['selection']:
            del projection['selection'][kind]['working']['coveColor']
    projection['version'] = 1
    assert projection == document
    return result

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--domain', required=True)
    parser.add_argument('--app', type=Path, required=True)
    parser.add_argument('--record', type=Path, required=True)
    parser.add_argument('--apply', action='store_true')
    args = parser.parse_args()
    process = subprocess.run(['pgrep', '-f', '^' + str(args.app.resolve() / 'Contents/MacOS/CsMail') + '$'], stdout=subprocess.PIPE)
    assert process.returncode == 1, 'Close the member app before preparing/applying conversion'
    prefs = preferences(args.domain)
    old = prefs[KEY]
    if not args.apply:
        assert not args.record.exists(), 'Use a fresh conversion record'
        document = json.loads(old)
        args.record.write_text(json.dumps({'before': document, 'beforeSHA256': fingerprint(old),
            'document': convert(document)}, indent=2) + '\n')
        print('Prepared v2: existing material, profiles and working selection preserved; independent cove colors seeded explicitly')
        return
    record = json.loads(args.record.read_text())
    assert fingerprint(old) == record['beforeSHA256'], 'Appearance changed after preparation'
    assert convert(json.loads(old)) == record['document']
    encoded = json.dumps(record['document'], separators=(',', ':')).encode()
    subprocess.run(['defaults', 'write', args.domain, KEY, '-data', encoded.hex()], check=True)
    after = preferences(args.domain)
    assert json.loads(after[KEY]) == record['document']
    assert {k: v for k, v in after.items() if k != KEY} == {k: v for k, v in prefs.items() if k != KEY}
    print('PASS v2 conversion; every previous appearance value and other preference preserved')

if __name__ == '__main__':
    main()
