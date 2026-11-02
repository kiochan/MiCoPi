"""Project-local modern Rust toolchain for the egui build (no system changes)."""
from pathlib import Path
import concurrent.futures
import hashlib
import shutil
import tarfile
import urllib.request

ROOT = Path(__file__).resolve().parents[1] / 'target/tools'
VERSION = '1.90.0'
HOST = 'x86_64-pc-windows-gnu'
DEST = ROOT / f'rust-{VERSION}'
PARTS = ['rustc', 'cargo', 'rust-std', 'rustfmt']

def install(part):
    name = f'{part}-{VERSION}-{HOST}'
    url = f'https://static.rust-lang.org/dist/{name}.tar.xz'
    archive = ROOT / f'{name}.tar.xz'
    if not archive.exists():
        print(f'Downloading {part}...', flush=True)
        urllib.request.urlretrieve(url, archive)
    expected = urllib.request.urlopen(url + '.sha256', timeout=60).read().decode().split()[0]
    if hashlib.sha256(archive.read_bytes()).hexdigest() != expected:
        raise RuntimeError(f'Checksum mismatch: {archive}')
    with tarfile.open(archive) as package:
        package.extractall(ROOT, filter='data')
    source = ROOT / name
    components = (source / 'components').read_text().splitlines()
    for component in components:
        shutil.copytree(source / component, DEST, dirs_exist_ok=True)
    print(f'Installed {part}', flush=True)

if __name__ == '__main__':
    ROOT.mkdir(parents=True, exist_ok=True)
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        list(pool.map(install, PARTS))
    print(DEST)
