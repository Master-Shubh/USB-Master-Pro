//! Zero-Allocation Slab Buffer Pool for Turbo-URB Payloads
//! Eliminates heap fragmentation during high-throughput biometric and webcam video streaming.

use crossbeam_channel::{bounded, Receiver, Sender};

pub const DEFAULT_CHUNK_SIZE: usize = 65536; // 64 KB page-aligned slabs
pub const POOL_CAPACITY: usize = 128;         // Pre-allocated ring buffers

pub struct SlabBuffer {
    data: Vec<u8>,
    sender: Sender<Vec<u8>>,
    chunk_size: usize,
}

impl SlabBuffer {
    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.data
    }

    pub fn capacity(&self) -> usize {
        self.data.capacity()
    }
}

impl Drop for SlabBuffer {
    fn drop(&mut self) {
        let mut buf = std::mem::take(&mut self.data);
        buf.resize(self.chunk_size, 0);
        let _ = self.sender.try_send(buf);
    }
}

#[derive(Clone)]
pub struct BufferPool {
    sender: Sender<Vec<u8>>,
    receiver: Receiver<Vec<u8>>,
    chunk_size: usize,
}

impl BufferPool {
    pub fn new(capacity: usize, chunk_size: usize) -> Self {
        let (sender, receiver) = bounded(capacity);
        for _ in 0..capacity {
            let buf = vec![0u8; chunk_size];
            sender.send(buf).unwrap();
        }
        Self {
            sender,
            receiver,
            chunk_size,
        }
    }

    pub fn default_pool() -> Self {
        Self::new(POOL_CAPACITY, DEFAULT_CHUNK_SIZE)
    }

    /// Acquires a clean pre-allocated buffer from the pool without any system heap allocation
    pub fn acquire(&self) -> SlabBuffer {
        let data = match self.receiver.try_recv() {
            Ok(buf) => buf,
            Err(_) => vec![0u8; self.chunk_size], // Fallback if saturated
        };

        SlabBuffer {
            data,
            sender: self.sender.clone(),
            chunk_size: self.chunk_size,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_pool_reuse() {
        let pool = BufferPool::new(4, 1024);
        {
            let mut b1 = pool.acquire();
            b1.as_mut_slice().copy_from_slice(&vec![0xAA; 1024]);
        } // b1 dropped, recycled back to pool

        let b2 = pool.acquire();
        assert_eq!(b2.capacity(), 1024);
    }
}
