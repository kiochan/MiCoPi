use super::*;
use std::{hint::black_box, time::Instant};

fn legacy_digest(pixels: &[u32]) -> Hash {
    let mut h = Sha256::new();
    for p in pixels {
        h.update(p.to_le_bytes());
    }
    h.finalize().into()
}

#[test]
fn batched_tile_hash_and_borrowed_wire_preserve_recording_bytes() {
    for len in [1, 2, 63, 64, 65, TILE * TILE] {
        let block = Block {
            pixels: (0..len)
                .map(|i| (i as u32).wrapping_mul(0x9713_6ab5))
                .collect(),
        };
        let bytes: Vec<_> = block.pixels.iter().flat_map(|p| p.to_le_bytes()).collect();
        assert_eq!(pixel_bytes(&block.pixels).as_ref(), bytes);
        #[cfg(target_endian = "little")]
        assert!(matches!(
            pixel_bytes(&block.pixels),
            std::borrow::Cow::Borrowed(_)
        ));
        assert_eq!(block.digest(), legacy_digest(&block.pixels));
        let encoded = serde_json::to_value(&block).unwrap();
        assert_eq!(encoded["pixels"], crate::application::codec::base64(&bytes));
        let decoded: Block = serde_json::from_value(encoded).unwrap();
        assert_eq!(decoded.pixels, block.pixels);
    }
}

#[test]
fn report_small_tile_hash_cost_without_a_machine_specific_threshold() {
    let block = Block {
        pixels: (0..TILE * TILE).map(|i| i as u32 * 7919).collect(),
    };
    let measure = |legacy| {
        let start = Instant::now();
        for _ in 0..64 {
            black_box(if legacy {
                legacy_digest(black_box(&block.pixels))
            } else {
                block.digest()
            });
        }
        start.elapsed().as_secs_f64() * 1000.
    };
    let old_ms = measure(true);
    let new_ms = measure(false);
    println!("replay tile hashing, 64 x {TILE}x{TILE} RGBA tiles, same SHA-256 bytes; legacy_ms={old_ms:.3}, batched_ms={new_ms:.3}. Excludes composition, compression, disk, UI and tablet latency.");
    assert_eq!(block.digest(), legacy_digest(&block.pixels));
}
