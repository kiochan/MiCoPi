"""Compile the SDK's x64 C11/C++17 layout and callback-signature fixtures."""
import argparse
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]
PORTABLE_CC = ROOT / 'target/tools/llvm-mingw-20240619-ucrt-x86_64/bin/x86_64-w64-mingw32-clang.exe'


def main():
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument('--cc', default=str(PORTABLE_CC), help='Existing x64 Clang-compatible C/C++ compiler')
    args = parser.parse_args()
    candidate = Path(args.cc).expanduser()
    compiler = str(candidate.resolve()) if candidate.is_file() else shutil.which(args.cc)
    if compiler is None:
        parser.error('Compiler not found; initialize the documented portable environment or pass --cc.')
    fixture = ROOT / 'tests/fixtures/native-layout.c'
    for language, standard in [('c', 'c11'), ('c++', 'c++17')]:
        subprocess.run([compiler, '-x', language, f'-std={standard}', '-Wall', '-Wextra', '-Werror',
                        '-fsyntax-only', '-I', str(ROOT / 'crates/plugin-api/include'), str(fixture)],
                       cwd=ROOT, check=True)
    print('Native SDK x64 C11/C++17 layouts and callback signatures passed; no output artifacts created.')


if __name__ == '__main__':
    main()
