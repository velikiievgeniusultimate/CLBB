#[cfg(target_os = "linux")]
pub mod linux;

#[derive(Clone, Debug)]
pub struct Region {
    pub start: u64,
    pub end: u64,
    pub readable: bool,
    pub writable: bool,
    pub name: String,
}

pub trait Memory {
    fn regions(&self) -> Result<Vec<Region>, String>;
    fn read(&self, address: u64, length: usize) -> Result<Vec<u8>, String>;
    fn read_into(&self, address: u64, bytes: &mut [u8]) -> Result<(), String>;
}
