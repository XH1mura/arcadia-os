#!/usr/bin/env python3
"""Add a file to an existing FAT32 image."""
import struct
import sys
import os

def read_le16(data, off):
    return struct.unpack_from('<H', data, off)[0]

def read_le32(data, off):
    return struct.unpack_from('<I', data, off)[0]

def write_le16(data, off, val):
    struct.pack_into('<H', data, off, val)

def write_le32(data, off, val):
    struct.pack_into('<I', data, off, val)

def main():
    if len(sys.argv) < 4:
        print("Usage: add_to_fat32.py <image> <local_file> <fat_name>")
        sys.exit(1)

    img_path = sys.argv[1]
    file_path = sys.argv[2]
    fat_name = sys.argv[3].upper()

    with open(img_path, 'r+b') as f:
        img = bytearray(f.read())

    # Read BPB
    bytes_per_sector = read_le16(img, 11)
    sectors_per_cluster = img[13]
    reserved_sectors = read_le16(img, 14)
    num_fats = img[16]
    root_entry_count = read_le16(img, 17)  # 0 for FAT32
    total_sectors_16 = read_le16(img, 19)
    fat_size_sectors = read_le32(img, 36)  # FAT32 uses 32-bit FAT size at offset 36
    total_sectors_32 = read_le32(img, 32)
    root_cluster = read_le32(img, 44)

    cluster_size = bytes_per_sector * sectors_per_cluster
    fat_start = reserved_sectors
    data_start = reserved_sectors + num_fats * fat_size_sectors

    print(f"FAT32: {bytes_per_sector} B/sector, {sectors_per_cluster} S/cluster, cluster={cluster_size}B")
    print(f"  Reserved: {reserved_sectors}, FATs: {num_fats}, FAT size: {fat_size_sectors} sectors")
    print(f"  Data start: sector {data_start} (offset {data_start * bytes_per_sector})")
    print(f"  Root cluster: {root_cluster}")

    # Read file to add
    with open(file_path, 'rb') as f:
        file_data = f.read()
    print(f"  File to add: {file_path} ({len(file_data)} bytes)")

    # Calculate clusters needed
    clusters_needed = (len(file_data) + cluster_size - 1) // cluster_size
    if clusters_needed == 0:
        clusters_needed = 1
    print(f"  Clusters needed: {clusters_needed}")

    # Read FAT to find free clusters
    fat_offset = fat_start * bytes_per_sector
    fat_size_bytes = fat_size_sectors * bytes_per_sector
    fat_data = img[fat_offset:fat_offset + fat_size_bytes]

    # Find free clusters (FAT32 entry == 0)
    free_clusters = []
    # FAT32 entries are 28 bits, packed 2 per 4 bytes (but easier to read as u32)
    for cluster in range(2, (total_sectors_32 - data_start) // sectors_per_cluster + 2):
        entry_offset = cluster * 4
        if entry_offset + 4 > fat_size_bytes:
            break
        entry = read_le32(fat_data, entry_offset) & 0x0FFFFFFF
        if entry == 0:
            free_clusters.append(cluster)
        if len(free_clusters) >= clusters_needed:
            break

    if len(free_clusters) < clusters_needed:
        print(f"  ERROR: Not enough free clusters ({len(free_clusters)} < {clusters_needed})")
        sys.exit(1)

    alloc_clusters = free_clusters[:clusters_needed]
    print(f"  Allocated clusters: {alloc_clusters}")

    # Write file data to clusters
    for i, cluster in enumerate(alloc_clusters):
        data_offset = i * cluster_size
        data_end = min(data_offset + cluster_size, len(file_data))
        chunk = file_data[data_offset:data_end]
        # Pad to cluster size
        chunk = chunk + b'\x00' * (cluster_size - len(chunk))

        cluster_sector = data_start + (cluster - 2) * sectors_per_cluster
        img_offset = cluster_sector * bytes_per_sector
        img[img_offset:img_offset + cluster_size] = chunk

    # Update FAT: chain clusters, last one = end-of-chain (0x0FFFFFF8)
    for i, cluster in enumerate(alloc_clusters):
        entry_offset = cluster * 4
        if i < clusters_needed - 1:
            next_cluster = alloc_clusters[i + 1]
            write_le32(img, fat_offset + entry_offset, next_cluster & 0x0FFFFFFF)
        else:
            write_le32(img, fat_offset + entry_offset, 0x0FFFFFF8)

    # Mirror FAT to second FAT table
    fat2_offset = (fat_start + fat_size_sectors) * bytes_per_sector
    img[fat2_offset:fat2_offset + fat_size_bytes] = img[fat_offset:fat_offset + fat_size_bytes]

    # Add directory entry in root directory
    root_sector = data_start + (root_cluster - 2) * sectors_per_cluster
    root_offset = root_sector * bytes_per_sector

    # Find first free directory entry (0x00 or 0xE5)
    dir_entry_size = 32
    dir_offset = root_offset
    found_free = False
    for i in range(cluster_size // dir_entry_size):
        entry = img[dir_offset + i * dir_entry_size:dir_offset + (i + 1) * dir_entry_size]
        if entry[0] == 0x00 or entry[0] == 0xE5:
            dir_offset = root_offset + i * dir_entry_size
            found_free = True
            break

    if not found_free:
        print("  ERROR: No free directory entry in root")
        sys.exit(1)

    print(f"  Dir entry at offset: 0x{dir_offset:X}")

    # Create 8.3 filename
    # Pad name to 8 chars, extension to 3 chars
    name_parts = fat_name.split('.')
    if len(name_parts) == 1:
        short_name = name_parts[0].ljust(8).encode('ascii')[:8]
        short_ext = b'    '
    else:
        short_name = name_parts[0].ljust(8).encode('ascii')[:8]
        short_ext = name_parts[1].ljust(3).encode('ascii')[:3]

    # Build directory entry (32 bytes)
    dir_entry = bytearray(32)
    dir_entry[0:8] = short_name       # Name
    dir_entry[8:11] = short_ext       # Extension
    dir_entry[11] = 0x20              # Attribute: Archive
    dir_entry[12] = 0x00              # Reserved
    dir_entry[13] = 0x00              # Create time fine
    dir_entry[14:16] = b'\x00\x00'   # Create time
    dir_entry[16:18] = b'\x00\x00'   # Create date
    dir_entry[18:20] = b'\x00\x00'   # Access date
    dir_entry[20:22] = b'\x00\x00'   # First cluster high (16 bits)
    dir_entry[22:24] = b'\x00\x00'   # Modify time
    dir_entry[24:26] = b'\x00\x00'   # Modify date
    write_le16(dir_entry, 26, alloc_clusters[0])  # First cluster low (16 bits)
    write_le32(dir_entry, 28, len(file_data))      # File size

    img[dir_offset:dir_offset + 32] = dir_entry

    # Write back
    with open(img_path, 'wb') as f:
        f.write(img)

    print(f"  SUCCESS: Added '{fat_name}' ({len(file_data)} bytes, {clusters_needed} clusters)")

if __name__ == '__main__':
    main()
