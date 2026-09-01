//! Minimal WAV chunk-layout probe used to debug the LIST-chunk test.

fn main() {
    // Case 1: wav_bytes + format-tag patch at 36..38
    let mut b: Vec<u8> = Vec::new();
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&0u32.to_le_bytes());
    b.extend_from_slice(b"WAVE");
    b.extend_from_slice(b"fmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&16000i32.to_le_bytes());
    b.extend_from_slice(&(16000i32 * 1 * 16 / 8).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&4u32.to_le_bytes());
    b.extend_from_slice(&0i16.to_le_bytes());
    b.extend_from_slice(&0i16.to_le_bytes());
    let riff_size = (b.len() - 8) as u32;
    b[4..8].copy_from_slice(&riff_size.to_le_bytes());
    println!("canonical: len={} bytes[34..38]={:?}", b.len(), &b[34..38]);
    let mut bytes = b.clone();
    bytes[36..38].copy_from_slice(&3u16.to_le_bytes());
    walk(&bytes);

    // Case 2: list chunk test
    let data = [100i16, -100];
    let mut b2: Vec<u8> = Vec::new();
    b2.extend_from_slice(b"RIFF");
    b2.extend_from_slice(&1234u32.to_le_bytes());
    b2.extend_from_slice(b"WAVEfmt ");
    b2.extend_from_slice(&16u32.to_le_bytes());
    b2.extend_from_slice(&1u16.to_le_bytes());
    b2.extend_from_slice(&1u16.to_le_bytes());
    b2.extend_from_slice(&16000i32.to_le_bytes());
    b2.extend_from_slice(&32000i32.to_le_bytes());
    b2.extend_from_slice(&2u16.to_le_bytes());
    b2.extend_from_slice(&16u16.to_le_bytes());
    b2.extend_from_slice(b"LIST");
    b2.extend_from_slice(&4u32.to_le_bytes());
    b2.extend_from_slice(b"INFO");
    b2.extend_from_slice(b"data");
    b2.extend_from_slice(&(std::mem::size_of_val(&data) as u32).to_le_bytes());
    for s in data {
        b2.extend_from_slice(&s.to_le_bytes());
    }
    let riff_size = (b2.len() - 8) as u32;
    b2[4..8].copy_from_slice(&riff_size.to_le_bytes());
    println!("list-case: len={}", b2.len());
    walk(&b2);
}

fn walk(bytes: &[u8]) {
    let mut pos = 12usize;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32::from_le_bytes([
            bytes[pos + 4],
            bytes[pos + 5],
            bytes[pos + 6],
            bytes[pos + 7],
        ]) as usize;
        let body = pos + 8;
        println!(
            "  id={:?} size={size} body={body}",
            String::from_utf8_lossy(id)
        );
        pos = body + size + (size & 1);
    }
}
