"""Optional local build tools for this machine's Rust 1.59 (no system changes)."""
import concurrent.futures
import hashlib
from pathlib import Path
import shutil
import tarfile
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[1] / "target" / "tools"
LLVM = "llvm-mingw-20240619-ucrt-x86_64"
RUST = "rust-std-1.59.0-x86_64-pc-windows-gnu"
PACKAGES = [
    (f"https://github.com/mstorsjo/llvm-mingw/releases/download/20240619/{LLVM}.zip", "llvm-mingw.zip",
     "810703594a7e3eea03385b5329c7ea3bd65f5e496b44cf1b68c17ff436d265e7"),
    (f"https://static.rust-lang.org/dist/2022-02-24/{RUST}.tar.xz", "rust-std.tar.xz",
     "8064a7415a582b05014d333ca0a2566817930a005ec34176bde9b438c3c996a4"),
]

def fetch(item):
    url, name, digest = item
    path = ROOT / name
    if not path.exists():
        print(f"Downloading {name}...", flush=True)
        urllib.request.urlretrieve(url, path)
    if hashlib.sha256(path.read_bytes()).hexdigest() != digest:
        raise RuntimeError(f"Checksum mismatch: {path}; remove it and retry")
    return path

def main():
    ROOT.mkdir(parents=True, exist_ok=True)
    with concurrent.futures.ThreadPoolExecutor(2) as pool:
        archives = list(pool.map(fetch, PACKAGES))
    if not (ROOT / LLVM / "bin" / "clang.exe").exists():
        with zipfile.ZipFile(archives[0]) as z:
            for name in z.namelist():
                if not (ROOT / name).resolve().is_relative_to(ROOT.resolve()):
                    raise RuntimeError("Unsafe archive path")
            z.extractall(ROOT)
    if not (ROOT / RUST).exists():
        with tarfile.open(archives[1]) as t:
            t.extractall(ROOT, filter="data")
    # Rust 1.59's GNU target names GCC runtime archives; provide LLVM equivalents.
    lib = ROOT / LLVM / "x86_64-w64-mingw32" / "lib"
    shutil.copyfile(lib / "libunwind.a", lib / "libgcc_eh.a")
    shutil.copyfile(ROOT / LLVM / "lib/clang/18/lib/windows/libclang_rt.builtins-x86_64.a", lib / "libgcc.a")
    print("Ready. Run: powershell -File scripts/build.ps1 -Portable")

if __name__ == "__main__":
    main()
