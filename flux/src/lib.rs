#![no_std]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

pub struct FluxManager {
    installed_packages: Vec<Package>,
}

struct Package {
    name: String,
    version: String,
    description: String,
}

impl FluxManager {
    pub fn new() -> Self {
        FluxManager {
            installed_packages: Vec::new(),
        }
    }

    pub fn install(&mut self, _package: &str) -> Result<(), &'static str> {
        Err("Not implemented")
    }

    pub fn remove(&mut self, _package: &str) -> Result<(), &'static str> {
        Err("Not implemented")
    }

    pub fn update(&mut self) -> Result<(), &'static str> {
        Err("Not implemented")
    }

    pub fn list(&self) {
    }
}
