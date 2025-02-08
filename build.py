#!/usr/bin/env python3

import os.path
import shutil
import subprocess
import sys

LIBRARY_NAME = 'wikiplugin_internal'

match sys.platform:
    case 'linux':
        LIB_SOURCE_NAME = f'lib{LIBRARY_NAME}.so'
        LIB_TARGET_NAME = f'{LIBRARY_NAME}.so'
    case 'darwin':
        LIB_SOURCE_NAME = f'lib{LIBRARY_NAME}.dylib'
        LIB_TARGET_NAME = f'{LIBRARY_NAME}.so'
    case 'win32':
        LIB_SOURCE_NAME = f'{LIBRARY_NAME}.dll'
        LIB_TARGET_NAME = f'{LIBRARY_NAME}.dll'
    case _:
        raise Exception(f'unsupported platform {sys.platform}')

def main():
    cargo = subprocess.run(['cargo', 'build', '--release'])
    if cargo.returncode != 0:
        raise Exception('cargo build failed')
    shutil.copy(os.path.join('target', 'release', LIB_SOURCE_NAME), os.path.join('lua', LIB_TARGET_NAME))

if __name__ == '__main__':
    main()
