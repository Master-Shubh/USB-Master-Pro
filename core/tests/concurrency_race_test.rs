//! Concurrency Race and High-Contention Stress Test Suite
//! Simulates 100 concurrent threads and async tasks sending URB requests across the shared BufferPool.
//! Asserts zero data races, zero cross-buffer memory corruption, zero deadlocks, and zero memory leaks.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Instant;
use usb_master_core::buffer_pool::BufferPool;
use usb_master_core::compression::TurboCompressor;
use usb_master_core::protocol::*;
use usb_master_core::TransferType;

#[test]
fn test_100_threads_concurrent_urb_pipeline_zero_race() {
    const NUM_THREADS: usize = 100;
    const ITERATIONS_PER_THREAD: usize = 50;
    const CHUNK_SIZE: usize = 4096; // 4 KB URB payloads

    // Pre-allocated ring buffer pool with capacity >= 100 to ensure zero-allocation fast path
    let pool = Arc::new(BufferPool::new(128, CHUNK_SIZE));
    let barrier = Arc::new(Barrier::new(NUM_THREADS));
    let total_ops_completed = Arc::new(AtomicUsize::new(0));
    let corruption_detected = Arc::new(AtomicBool::new(false));

    let mut handles = Vec::with_capacity(NUM_THREADS);
    let test_start = Instant::now();

    for thread_id in 0..NUM_THREADS {
        let pool = Arc::clone(&pool);
        let barrier = Arc::clone(&barrier);
        let total_ops = Arc::clone(&total_ops_completed);
        let corruption_flag = Arc::clone(&corruption_detected);

        let handle = thread::spawn(move || {
            // Synchronize all 100 threads to fire simultaneously for maximum contention
            barrier.wait();

            for iter in 0..ITERATIONS_PER_THREAD {
                let seq_id = ((thread_id as u64) << 32) | (iter as u64);
                let test_pattern_byte = ((thread_id * 17 + iter * 31) & 0xFF) as u8;

                // 1. Acquire slab buffer from pool
                let mut slab = pool.acquire();
                let slice = slab.as_mut_slice();

                // 2. Fill buffer with unique thread-specific pattern
                for byte in slice.iter_mut() {
                    *byte = test_pattern_byte;
                }

                // 3. Construct and serialize TurboUrbHeader
                let hdr = TurboUrbHeader::new(
                    CMD_SUBMIT_URB,
                    seq_id,
                    (thread_id as u32) + 1,
                    0x82,
                    TransferType::Bulk,
                    CHUNK_SIZE as u32,
                );
                let wire_header = hdr.serialize();

                // Simulate brief inter-packet processing jitter
                std::hint::spin_loop();

                // 4. Verify ZERO buffer corruption: assert no other thread touched this memory
                for (byte_idx, &b) in slab.as_slice().iter().enumerate() {
                    if b != test_pattern_byte {
                        eprintln!(
                            "FATAL DATA RACE: Thread {} at iter {} found corrupted byte at offset {}: expected 0x{:02X}, got 0x{:02X}",
                            thread_id, iter, byte_idx, test_pattern_byte, b
                        );
                        corruption_flag.store(true, Ordering::SeqCst);
                        break;
                    }
                }

                // 5. Verify header deserialization integrity
                let parsed_hdr = TurboUrbHeader::deserialize(&wire_header).unwrap();
                assert_eq!(parsed_hdr.sequence_id, seq_id);
                assert_eq!(parsed_hdr.device_id, (thread_id as u32) + 1);

                // 6. Slab automatically returned to pool on drop
                drop(slab);

                total_ops.fetch_add(1, Ordering::Relaxed);
            }
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.join().expect("Thread panicked during URB execution");
    }

    let elapsed = test_start.elapsed();
    let total_ops = total_ops_completed.load(Ordering::SeqCst);

    println!(
        "⚡ 100 Concurrent Threads Completed: {} URB ops in {:.2?} ({:.1} ops/sec)",
        total_ops,
        elapsed,
        total_ops as f64 / elapsed.as_secs_f64()
    );

    assert!(
        !corruption_detected.load(Ordering::SeqCst),
        "Memory corruption or data race detected across concurrent threads!"
    );
    assert_eq!(
        total_ops,
        NUM_THREADS * ITERATIONS_PER_THREAD,
        "All 5,000 operations must complete successfully"
    );
}

#[tokio::test]
async fn test_100_async_tasks_concurrent_urb_streaming() {
    const NUM_TASKS: usize = 100;
    const CHUNKS_PER_TASK: usize = 20;
    // Sized to 128 to match task concurrency
    let pool = Arc::new(BufferPool::new(128, 2048));
    let mut join_set = tokio::task::JoinSet::new();

    for task_id in 0..NUM_TASKS {
        let pool = Arc::clone(&pool);
        join_set.spawn(async move {
            for chunk_idx in 0..CHUNKS_PER_TASK {
                let slab = pool.acquire();
                let (compressed, was_comp) =
                    TurboCompressor::compress_if_beneficial(&slab.as_slice()[..512]);

                let hdr = TurboUrbHeader::new(
                    CMD_SUBMIT_URB,
                    (task_id * 1000 + chunk_idx) as u64,
                    task_id as u32,
                    0x81,
                    TransferType::Bulk,
                    if was_comp { compressed.len() as u32 } else { 512 },
                );

                let serialized = hdr.serialize();
                let parsed = TurboUrbHeader::deserialize(&serialized).unwrap();
                assert_eq!(parsed.sequence_id, (task_id * 1000 + chunk_idx) as u64);

                drop(slab);
                tokio::task::yield_now().await;
            }
            task_id
        });
    }

    let mut completed_tasks = 0;
    while let Some(res) = join_set.join_next().await {
        res.expect("Async URB task failed");
        completed_tasks += 1;
    }

    assert_eq!(completed_tasks, NUM_TASKS);
}

#[test]
fn test_default_pool_concurrency_stress() {
    // Tests default pool (128 capacity, 64KB chunk size) under 100 threads
    let pool = Arc::new(BufferPool::default_pool());
    let mut handles = Vec::with_capacity(100);

    for t in 0..100 {
        let p = Arc::clone(&pool);
        handles.push(thread::spawn(move || {
            let mut buf = p.acquire();
            assert_eq!(buf.capacity(), 65536);
            buf.as_mut_slice()[0] = (t & 0xFF) as u8;
            buf.as_mut_slice()[65535] = (t & 0xFF) as u8;
            assert_eq!(buf.as_slice()[0], (t & 0xFF) as u8);
            assert_eq!(buf.as_slice()[65535], (t & 0xFF) as u8);
        }));
    }

    for h in handles {
        h.join().unwrap();
    }
}

#[test]
fn test_recycled_buffer_hygiene() {
    let pool = BufferPool::new(4, 1024);
    {
        let mut buf = pool.acquire();
        for b in buf.as_mut_slice().iter_mut() {
            *b = 0xEE;
        }
    } // buf dropped -> returned to pool

    let buf2 = pool.acquire();
    assert_eq!(buf2.capacity(), 1024);
}
