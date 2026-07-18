#!/usr/bin/env python3
"""Test Phase G - run QEMU and send commands."""
import subprocess
import time
import sys

proc = subprocess.Popen(
    [
        "qemu-system-x86_64",
        "-kernel", "build/arcadia-kernel.elf",
        "-m", "256M",
        "-nographic",
        "-drive", "file=build/test-fat32.img,format=raw,if=ide",
        "-no-reboot",
    ],
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.STDOUT,
    text=False,
)

output = b""
start = time.time()

def read_output(timeout=5):
    global output
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            chunk = os.read(proc.stdout.fileno(), 4096)
            if not chunk:
                break
            output += chunk
        except:
            time.sleep(0.1)
    return output.decode('utf-8', errors='replace')

import os

try:
    time.sleep(3)
    text = read_output(2)

    # Send 'run'
    print("=== SENDING: run ===")
    proc.stdin.write(b'run\n')
    proc.stdin.flush()
    time.sleep(8)  # Wait longer for process to run
    text = read_output(2)
    print(text[-2000:])

finally:
    proc.terminate()
    proc.wait(timeout=5)
