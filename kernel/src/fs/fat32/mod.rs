pub mod bpb;
pub mod dir;
pub mod fat;

use alloc::vec::Vec;
use crate::block::BlockDevice;
use self::bpb::Fat32Bpb;

pub struct Fat32Fs {
    pub bpb: Fat32Bpb,
    pub device: &'static dyn BlockDevice,
    pub partition_lba: u32,
}

impl Fat32Fs {
    pub fn mount(
        device: &'static dyn BlockDevice,
        partition_lba: u32,
    ) -> Result<Self, &'static str> {
        let mut buf = alloc::vec![0u8; 512];
        device
            .read_sector(partition_lba, &mut buf)
            .map_err(|_| "Failed to read boot sector")?;

        let bpb = Fat32Bpb::from_bytes(&buf).ok_or("Not a FAT32 filesystem")?;

        Ok(Fat32Fs {
            bpb,
            device,
            partition_lba,
        })
    }

    pub fn read_sector_buf(&self, lba: u32, buf: &mut [u8]) -> Result<(), &'static str> {
        self.device
            .read_sector(self.partition_lba + lba, buf)
            .map_err(|_| "Sector read failed")
    }

    pub fn write_sector_buf(&self, lba: u32, buf: &[u8]) -> Result<(), &'static str> {
        self.device
            .write_sector(self.partition_lba + lba, buf)
            .map_err(|_| "Sector write failed")
    }

    pub fn read_cluster(&self, cluster: u32, buf: &mut [u8]) -> Result<(), &'static str> {
        let cluster_size = self.bpb.cluster_size_bytes();
        if buf.len() < cluster_size {
            return Err("Buffer too small for cluster");
        }
        let sector = self.bpb.cluster_to_sector(cluster);
        let spc = self.bpb.sectors_per_cluster as usize;
        for i in 0..spc {
            let offset = i * self.bpb.bytes_per_sector();
            self.read_sector_buf(
                sector + i as u32,
                &mut buf[offset..offset + self.bpb.bytes_per_sector()],
            )?;
        }
        Ok(())
    }

    pub fn write_cluster(&self, cluster: u32, buf: &[u8]) -> Result<(), &'static str> {
        let cluster_size = self.bpb.cluster_size_bytes();
        if buf.len() < cluster_size {
            return Err("Buffer too small for cluster");
        }
        let sector = self.bpb.cluster_to_sector(cluster);
        let spc = self.bpb.sectors_per_cluster as usize;
        for i in 0..spc {
            let offset = i * self.bpb.bytes_per_sector();
            self.write_sector_buf(
                sector + i as u32,
                &buf[offset..offset + self.bpb.bytes_per_sector()],
            )?;
        }
        Ok(())
    }

    pub fn read_file(
        &self,
        start_cluster: u32,
        size: u32,
        buf: &mut [u8],
    ) -> Result<usize, &'static str> {
        let chain = fat::read_chain(self, start_cluster)?;
        let cluster_size = self.bpb.cluster_size_bytes();
        let mut offset = 0usize;
        let mut remaining = size as usize;

        for &cluster in &chain {
            if remaining == 0 || offset >= buf.len() {
                break;
            }
            let mut cluster_buf = alloc::vec![0u8; cluster_size];
            self.read_cluster(cluster, &mut cluster_buf)?;
            let to_copy = remaining.min(cluster_size).min(buf.len() - offset);
            buf[offset..offset + to_copy].copy_from_slice(&cluster_buf[..to_copy]);
            offset += to_copy;
            remaining -= to_copy;
        }

        Ok(offset)
    }

    pub fn write_file_data(
        &self,
        start_cluster: u32,
        data: &[u8],
    ) -> Result<(), &'static str> {
        let cluster_size = self.bpb.cluster_size_bytes();
        let clusters_needed = if data.is_empty() {
            0
        } else {
            (data.len() + cluster_size - 1) / cluster_size
        };

        let existing = fat::read_chain(self, start_cluster)?;
        let mut clusters: Vec<u32> = existing.iter().copied().collect();

        while clusters.len() < clusters_needed {
            let new_cluster = fat::alloc_cluster(self)?;
            clusters.push(new_cluster);
        }

        for (i, &cluster) in clusters.iter().enumerate() {
            let mut cluster_buf = alloc::vec![0u8; cluster_size];
            let byte_offset = i * cluster_size;
            if byte_offset < data.len() {
                let to_copy = (data.len() - byte_offset).min(cluster_size);
                cluster_buf[..to_copy]
                    .copy_from_slice(&data[byte_offset..byte_offset + to_copy]);
            }
            self.write_cluster(cluster, &cluster_buf)?;
        }

        for i in 0..clusters.len() {
            let next = if i + 1 < clusters.len() {
                clusters[i + 1]
            } else {
                bpb::FAT32_EOC
            };
            fat::write_fat_entry(self, clusters[i], next)?;
        }

        if clusters.len() < existing.len() {
            for &cluster in &existing[clusters.len()..] {
                fat::write_fat_entry(self, cluster, bpb::FAT32_FREE)?;
            }
        }

        Ok(())
    }

    pub fn allocate_and_write(&self, data: &[u8]) -> Result<u32, &'static str> {
        let first = fat::alloc_cluster(self)?;
        fat::write_fat_entry(self, first, bpb::FAT32_EOC)?;

        if !data.is_empty() {
            self.write_file_data(first, data)?;
        }

        Ok(first)
    }
}
