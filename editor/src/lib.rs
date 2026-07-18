#![no_std]

extern crate alloc;

pub mod editor_core;

pub struct ArcEditor {
    file_path: Option<alloc::string::String>,
    content: alloc::vec::Vec<alloc::string::String>,
    cursor_x: usize,
    cursor_y: usize,
    dirty: bool,
}

impl ArcEditor {
    pub fn new() -> Self {
        ArcEditor {
            file_path: None,
            content: alloc::vec::Vec::new(),
            cursor_x: 0,
            cursor_y: 0,
            dirty: false,
        }
    }

    pub fn open(&mut self, path: &str) -> Result<(), &'static str> {
        self.file_path = Some(alloc::string::String::from(path));
        Ok(())
    }

    pub fn save(&self) -> Result<(), &'static str> {
        Err("Not implemented")
    }
}
