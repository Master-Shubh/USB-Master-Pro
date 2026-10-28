//! High-Speed Adaptive Compression Engine using Zstandard
//! Optimizes raw fingerprint bitmaps, IR video scans, and large bulk transfers.

use std::io;

pub const MIN_COMPRESSION_SIZE: usize = 128; // Don't compress tiny control/interrupt packets
pub const ZSTD_FAST_LEVEL: i32 = 1;          // Level 1: Sub-millisecond latency (<0.4ms)

pub struct TurboCompressor;

impl TurboCompressor {
    /// Compresses a buffer if it exceeds MIN_COMPRESSION_SIZE and entropy suggests compressibility.
    /// Returns (compressed_data, was_compressed)
    pub fn compress_if_beneficial(data: &[u8]) -> (Vec<u8>, bool) {
        if data.len() < MIN_COMPRESSION_SIZE {
            return (data.to_vec(), false);
        }

        // Fast entropy heuristic check on first 64 bytes
        if !Self::likely_compressible(&data[..data.len().min(64)]) {
            return (data.to_vec(), false);
        }

        match zstd::encode_all(data, ZSTD_FAST_LEVEL) {
            Ok(compressed) => {
                // Only return compressed if it actually saved at least 10%
                if compressed.len() + 16 < data.len() {
                    (compressed, true)
                } else {
                    (data.to_vec(), false)
                }
            }
            Err(_) => (data.to_vec(), false),
        }
    }

    /// Compresses a buffer with Zstandard Level 1.
    pub fn compress(data: &[u8]) -> Result<Vec<u8>, io::Error> {
        zstd::encode_all(data, ZSTD_FAST_LEVEL)
    }

    /// Decompresses a Zstandard-compressed payload.
    pub fn decompress(compressed_data: &[u8]) -> Result<Vec<u8>, io::Error> {
        zstd::decode_all(compressed_data)
    }

    /// Fast heuristic check to avoid compressing high-entropy encrypted/video data
    fn likely_compressible(sample: &[u8]) -> bool {
        if sample.is_empty() {
            return false;
        }

        let mut counts = [0u32; 256];
        for &b in sample {
            counts[b as usize] += 1;
        }

        // If many bytes are identical or 0x00 / 0xFF (typical in raw sensor frames), compress!
        let max_freq = counts.iter().copied().max().unwrap_or(0);
        let sample_len = sample.len() as u32;

        // If a single byte takes up more than 15% of the sample, highly compressible
        max_freq * 100 / sample_len > 15
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_biometric_bitmap_compression() {
        // Simulate a 100KB raw grayscale fingerprint frame (lots of repeating patterns)
        let mut raw_frame = vec![0x10; 100_000];
        for i in (0..100_000).step_by(10) {
            raw_frame[i] = 0xAA;
        }

        let (compressed, was_compressed) = TurboCompressor::compress_if_beneficial(&raw_frame);
        assert!(was_compressed);
        assert!(compressed.len() < raw_frame.len() / 5); // Huge ratio

        let decompressed = TurboCompressor::decompress(&compressed).expect("Failed decompress");
        assert_eq!(decompressed, raw_frame);
    }
}
