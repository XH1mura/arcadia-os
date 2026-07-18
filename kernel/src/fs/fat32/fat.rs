use alloc::vec::Vec;
use crate::fs::fat32::bpb::{FAT32_BAD, FAT32_EOC, FAT32_FREE};
use crate::fs::fat32::Fat32Fs;

pub fn read_fat_entry(fs: &Fat32Fs, cluster: u32) -> Result<u32, &'static str> {
    let eps = fs.bpb.bytes_per_sector() / 4;
    let fat_sector = (cluster / eps as u32) as u32;
    let fat_offset = (cluster % eps as u32) as usize * 4;

    let mut buf = alloc::vec![0u8; fs.bpb.bytes_per_sector()];
    fs.read_sector_buf(fs.bpb.fat_start_sector() + fat_sector, &mut buf)?;

    let value = u32::from_le_bytes([buf[fat_offset], buf[fat_offset + 1], buf[fat_offset + 2], buf[fat_offset + 3]]);
    Ok(value & 0x0FFFFFFF)
}

pub fn write_fat_entry(fs: &Fat32Fs, cluster: u32, value: u32) -> Result<(), &'static str> {
    let eps = fs.bpb.bytes_per_sector() / 4;
    let fat_sector = (cluster / eps as u32) as u32;
    let fat_offset = (cluster % eps as u32) as usize * 4;

    let mut buf = alloc::vec![0u8; fs.bpb.bytes_per_sector()];
    fs.read_sector_buf(fs.bpb.fat_start_sector() + fat_sector, &mut buf)?;

    let existing = u32::from_le_bytes([buf[fat_offset], buf[fat_offset + 1], buf[fat_offset + 2], buf[fat_offset + 3]]);
    let new_val = (existing & 0xF0000000) | (value & 0x0FFFFFFF);
    let bytes = new_val.to_le_bytes();
    buf[fat_offset..fat_offset + 4].copy_from_slice(&bytes);

    for fat_copy in 0..fs.bpb.num_fats {
        let sector = fs.bpb.fat_start_sector() + fat_sector
            + fat_copy as u32 * fs.bpb.fat_size_32;
        fs.write_sector_buf(sector, &buf)?;
    }

    Ok(())
}

pub fn read_chain(fs: &Fat32Fs, start_cluster: u32) -> Result<Vec<u32>, &'static str> {
    let mut chain = Vec::new();
    let mut current = start_cluster;

    loop {
        if current < 2 {
            break;
        }
        chain.push(current);
        let next = read_fat_entry(fs, current)?;
        if next >= FAT32_EOC || next == FAT32_BAD || next == 0 {
            break;
        }
        if next < 2 || next == current {
            break;
        }
        if chain.contains(&next) {
            break;
        }
        current = next;
        if chain.len() > 100000 {
            break;
        }
    }

    Ok(chain)
}

pub fn free_chain(fs: &Fat32Fs, start_cluster: u32) -> Result<(), &'static str> {
    let chain = read_chain(fs, start_cluster)?;
    for &cluster in &chain {
        write_fat_entry(fs, cluster, FAT32_FREE)?;
    }
    Ok(())
}

pub fn alloc_cluster(fs: &Fat32Fs) -> Result<u32, &'static str> {
    let data_sectors = fs.bpb.total_sectors_32.saturating_sub(fs.bpb.data_start_sector());
    if data_sectors == 0 {
        return Err("No data sectors");
    }
    let max_cluster = data_sectors / fs.bpb.sectors_per_cluster as u32;
    if max_cluster < 2 {
        return Err("No clusters available");
    }

    for cluster in 2..max_cluster + 2 {
        let val = read_fat_entry(fs, cluster)?;
        if val == FAT32_FREE {
            let mut cluster_buf = alloc::vec![0u8; fs.bpb.cluster_size_bytes()];
            for b in cluster_buf.iter_mut() {
                *b = 0;
            }
            fs.write_cluster(cluster, &cluster_buf)?;
            return Ok(cluster);
        }
    }

    Err("No free clusters")
}
