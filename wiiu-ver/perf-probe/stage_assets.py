#!/usr/bin/env python3
"""Copy an owner's original C2 assets for the Wii U herbivore/map probe."""
import argparse
from pathlib import Path
import shutil


def assets():
    files = ['HUNTER1.CAR', '_RES.TXT', 'WEAPONS/PISTOL.CAR', 'WEAPONS/SHOTGUN.CAR', 'WEAPONS/DBSGUN.CAR', 'WEAPONS/X_BOW.CAR', 'WEAPONS/RIFLE.CAR', 'WEAPONS/SNIPER.CAR', 'MENU/MENUM.TGA']
    files += [f'{name}.CAR' for name in ['MOSH', 'GALL', 'DIMOR2', 'PTERA', 'DIMET', 'BRACHI1', 'PAR2', 'ANKYLO1', 'STEGO', 'ALLO', 'CHASMOSA', 'VELO2', 'SPINO', 'CERATO1', 'TIREX']]
    files += [f'AREAS/AREA{i}.{ext}' for i in range(1, 6) for ext in ['MAP', 'RSC']]
    files += [f'MENU/PICS/AREA{i}.TGA' for i in range(1, 6)]
    files += [f'MENU/PICS/{prefix}{i}.TGA' for prefix in ['DINO', 'CALL'] for i in range(1, 10)]
    files += [f'MENU/PICS/WEAPON{i}.TGA' for i in range(1,7)] + ['MENU/PICS/EQUIP2.TGA']
    files += [f'SOUNDFX/CALLS/CALL{i}_{v}.WAV' for i in range(1, 10) for v in 'ABC']
    files += [f'SOUNDFX/STEPS/{prefix}{i}.WAV' for prefix in ['HWALK', 'FOOTW'] for i in range(1, 4)]
    files += ['SOUNDFX/A_UNDERW.WAV'] + [f'SOUNDFX/HUM_DIE{i}.WAV' for i in range(1, 5)]
    return files


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('huntdat', type=Path)
    parser.add_argument('destination', type=Path)
    args = parser.parse_args()
    files = assets()
    missing = [name for name in files if not (args.huntdat / name).is_file()]
    if missing:
        parser.error('Missing source assets: ' + ', '.join(missing))
    args.destination.mkdir(parents=True, exist_ok=True)
    for name in files:
        shutil.copy2(args.huntdat / name, args.destination / Path(name).name)
    print(f'Copied {len(files)} original assets to {args.destination}')


if __name__ == '__main__':
    main()
