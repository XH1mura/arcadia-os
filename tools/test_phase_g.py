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

def read_until_prompt(timeout=15):
    global output
    while time.time() - start < timeout:
        chunk = proc.stdout.read1(4096) if hasattr(proc.stdout, 'read1') else proc.stdout.read(4096)
        if chunk:
            output += chunk
            text = output.decode('utf-8', errors='replace')
            if 'arcadia>' in text.split('\n')[-1] if text else False:
                return text
            if 'arcadia>' in text:
                return text
    return output.decode('utf-8', errors='replace')

try:
    # Wait for boot
    time.sleep(3)
    # Read whatever output is available
    text = read_until_prompt(10)
    print("=== BOOT OUTPUT ===")
    for line in text.split('\n'):
        if 'arcadia>' in line or 'error' in line.lower() or 'panic' in line.lower() or 'PROCESS' in line or 'Hello' in line or 'Ring 3' in line or 'init' in line.lower():
            print(line)
    
    # Send 'ps' command
    print("\n=== SENDING: ps ===")
    proc.stdin.write(b'ps\n')
    proc.stdin.flush()
    time.sleep(1)
    text = read_until_prompt(5)
    for line in text.split('\n'):
        if 'PID' in line or 'STATE' in line or 'running' in line or 'unused' in line or 'arcadia>' in line or 'active' in line.lower():
            print(line)

    # Send 'run' command
    print("\n=== SENDING: run ===")
    proc.stdin.write(b'run\n')
    proc.stdin.flush()
    time.sleep(3)
    text = read_until_prompt(8)
    for line in text.split('\n'):
        l = line.strip()
        if l and ('init' in l.lower() or 'ring' in l.lower() or 'hello' in l.lower() or 'process' in l.lower() or 'error' in l.lower() or 'panic' in l.lower() or 'entry' in l.lower() or 'loaded' in l.lower() or 'creating' in l.lower() or '3' in l):
            print(line)

    print("\n=== RAW OUTPUT (last 500 chars) ===")
    text = output.decode('utf-8', errors='replace')
    print(text[-500:])

finally:
    proc.terminate()
    proc.wait(timeout=5)
